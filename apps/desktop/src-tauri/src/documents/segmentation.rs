use super::{BlockType, DocumentBlock};
use crate::domain::RuntimeError;

/// Conservative character budget used when a runtime tokenizer is unavailable.
/// The reserve covers the adapter prompt and the model completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentationLimits {
    pub context_characters: usize,
    pub prompt_reserve: usize,
    pub output_reserve: usize,
}

impl Default for SegmentationLimits {
    fn default() -> Self {
        Self {
            context_characters: 24_000,
            prompt_reserve: 4_000,
            output_reserve: 8_000,
        }
    }
}

impl SegmentationLimits {
    fn input_budget(self) -> usize {
        self.context_characters
            .saturating_sub(self.prompt_reserve)
            .saturating_sub(self.output_reserve)
            .max(1)
    }
}

pub fn segment_block(
    block: &DocumentBlock,
    limits: SegmentationLimits,
) -> Result<Vec<DocumentBlock>, RuntimeError> {
    if block.source_text.is_empty() || block.source_text.chars().count() <= limits.input_budget() {
        return Ok(vec![block.clone()]);
    }
    if matches!(
        block.block_type,
        BlockType::Code | BlockType::Formula | BlockType::Image
    ) {
        return Err(RuntimeError::InvalidInput(format!(
            "{} block exceeds the document context budget and cannot be safely subdivided",
            block.block_type.block_type_name()
        )));
    }

    let pieces = split_text(&block.source_text, limits.input_budget());
    if pieces.len() > 999_999 {
        return Err(RuntimeError::InvalidInput(
            "document block has too many segments".into(),
        ));
    }
    let base = block
        .ordinal
        .checked_mul(1_000_000)
        .ok_or_else(|| RuntimeError::InvalidInput("document block ordinal is too large".into()))?;
    pieces
        .into_iter()
        .enumerate()
        .map(|(index, source_text)| {
            let ordinal = base.checked_add(index as i64).ok_or_else(|| {
                RuntimeError::InvalidInput("document block ordinal overflow".into())
            })?;
            Ok(DocumentBlock {
                id: format!("{}::part-{index:04}", block.id),
                ordinal,
                block_type: block.block_type,
                source_text,
                translated_text: None,
            })
        })
        .collect()
}

fn split_text(text: &str, budget: usize) -> Vec<String> {
    let sentences = text
        .split_inclusive(['.', '!', '?', '\n'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if sentences.is_empty() {
        return split_words(text, budget);
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    for sentence in sentences {
        if sentence.chars().count() > budget {
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            chunks.extend(split_words(sentence, budget));
        } else if current.chars().count() + sentence.chars().count() < budget {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(sentence);
        } else {
            chunks.push(std::mem::take(&mut current));
            current.push_str(sentence);
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn split_words(text: &str, budget: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if word.chars().count() > budget {
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            chunks.extend(split_chars(word, budget));
        } else if current.chars().count() + word.chars().count() < budget {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        } else {
            chunks.push(std::mem::take(&mut current));
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn split_chars(text: &str, budget: usize) -> Vec<String> {
    text.chars()
        .collect::<Vec<_>>()
        .chunks(budget)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

trait BlockTypeName {
    fn block_type_name(&self) -> &'static str;
}

impl BlockTypeName for BlockType {
    fn block_type_name(&self) -> &'static str {
        match self {
            BlockType::Heading => "heading",
            BlockType::Paragraph => "paragraph",
            BlockType::ListItem => "list item",
            BlockType::TableCell => "table cell",
            BlockType::Caption => "caption",
            BlockType::Footnote => "footnote",
            BlockType::Code => "code",
            BlockType::Formula => "formula",
            BlockType::Image => "image",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(block_type: BlockType, text: &str) -> DocumentBlock {
        DocumentBlock {
            id: "block-7".into(),
            ordinal: 7,
            block_type,
            source_text: text.into(),
            translated_text: None,
        }
    }

    #[test]
    fn keeps_small_blocks_unchanged() {
        let original = block(BlockType::Paragraph, "Short text.");
        assert_eq!(
            segment_block(&original, SegmentationLimits::default()).unwrap(),
            vec![original]
        );
    }

    #[test]
    fn splits_on_sentence_boundaries_with_stable_order() {
        let original = block(
            BlockType::Paragraph,
            "One sentence. Two sentence! Three sentence?",
        );
        let limits = SegmentationLimits {
            context_characters: 28,
            prompt_reserve: 4,
            output_reserve: 4,
        };
        let parts = segment_block(&original, limits).unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].id, "block-7::part-0000");
        assert!(parts[0].ordinal < parts[1].ordinal);
        assert!(parts
            .iter()
            .all(|part| part.source_text.chars().count() <= 20));
    }

    #[test]
    fn falls_back_to_words_and_then_characters() {
        let original = block(BlockType::Paragraph, "abcdefghij other");
        let limits = SegmentationLimits {
            context_characters: 10,
            prompt_reserve: 2,
            output_reserve: 2,
        };
        let parts = segment_block(&original, limits).unwrap();
        assert_eq!(
            parts
                .iter()
                .map(|part| part.source_text.len())
                .collect::<Vec<_>>(),
            vec![6, 4, 5]
        );
    }

    #[test]
    fn does_not_split_protected_blocks() {
        let error = segment_block(
            &block(BlockType::Code, "a very long code fragment"),
            SegmentationLimits {
                context_characters: 10,
                prompt_reserve: 2,
                output_reserve: 2,
            },
        )
        .unwrap_err();
        assert!(matches!(error, RuntimeError::InvalidInput(_)));
    }
}
