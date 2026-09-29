mod package;
mod xml;

use super::{BlockType, DocumentBlock};
use crate::domain::RuntimeError;

pub const PARSER_VERSION: &str = "docx-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analysis {
    pub blocks: Vec<DocumentBlock>,
    pub diagnostics: Vec<String>,
}

pub fn analyze(bytes: &[u8]) -> Result<Analysis, RuntimeError> {
    let main_xml = package::read_main_document(bytes)?;
    xml::parse_document(&main_xml)
}

pub fn export(source: &[u8], blocks: &[DocumentBlock]) -> Result<Vec<u8>, RuntimeError> {
    let main_xml = package::read_main_document(source)?;
    let blocks = coalesce_segments(blocks)?;
    let rewritten = xml::rewrite_document(&main_xml, &blocks)?;
    package::rewrite_package(source, &rewritten)
}

fn coalesce_segments(blocks: &[DocumentBlock]) -> Result<Vec<DocumentBlock>, RuntimeError> {
    let mut result = Vec::new();
    for block in blocks {
        let Some((parent_id, _)) = block.id.rsplit_once("::part-") else {
            result.push(block.clone());
            continue;
        };
        if let Some(parent) = result
            .iter_mut()
            .find(|item: &&mut DocumentBlock| item.id == parent_id)
        {
            if let Some(text) = &block.translated_text {
                let combined = parent.translated_text.get_or_insert_default();
                if !combined.is_empty() {
                    combined.push(' ');
                }
                combined.push_str(text);
            }
        } else {
            let mut parent = block.clone();
            parent.id = parent_id.to_string();
            parent.ordinal = block.ordinal.div_euclid(1_000_000);
            parent.translated_text = block.translated_text.clone();
            result.push(parent);
        }
    }
    result.sort_by_key(|block| block.ordinal);
    if result.iter().any(|block| block.translated_text.is_none()) {
        return Err(RuntimeError::InvalidInput(
            "DOCX job has untranslated blocks".into(),
        ));
    }
    Ok(result)
}

pub(crate) fn block(id_ordinal: usize, block_type: BlockType, text: String) -> DocumentBlock {
    DocumentBlock {
        id: format!("word/document.xml#p-{id_ordinal:08}"),
        ordinal: id_ordinal as i64,
        block_type,
        source_text: text,
        translated_text: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_zip_input() {
        assert!(matches!(
            analyze(b"not a zip"),
            Err(RuntimeError::InvalidInput(_))
        ));
    }

    #[test]
    fn combines_segmented_blocks_before_export() {
        let blocks = vec![
            DocumentBlock {
                id: "word/document.xml#p-00000000::part-0000".into(),
                ordinal: 0,
                block_type: BlockType::Paragraph,
                source_text: "first".into(),
                translated_text: Some("one".into()),
            },
            DocumentBlock {
                id: "word/document.xml#p-00000000::part-0001".into(),
                ordinal: 1,
                block_type: BlockType::Paragraph,
                source_text: "second".into(),
                translated_text: Some("two".into()),
            },
        ];

        let combined = coalesce_segments(&blocks).unwrap();
        assert_eq!(combined.len(), 1);
        assert_eq!(combined[0].id, "word/document.xml#p-00000000");
        assert_eq!(combined[0].translated_text.as_deref(), Some("one two"));
    }

    #[test]
    fn orders_combined_segments_before_later_paragraphs() {
        let blocks = vec![
            DocumentBlock {
                id: "word/document.xml#p-00000001".into(),
                ordinal: 1,
                block_type: BlockType::Paragraph,
                source_text: "later".into(),
                translated_text: Some("later".into()),
            },
            DocumentBlock {
                id: "word/document.xml#p-00000000::part-0000".into(),
                ordinal: 0,
                block_type: BlockType::Paragraph,
                source_text: "first".into(),
                translated_text: Some("first".into()),
            },
        ];

        let combined = coalesce_segments(&blocks).unwrap();
        assert_eq!(combined[0].id, "word/document.xml#p-00000000");
    }
}
