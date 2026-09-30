mod xml;

use super::{DocumentBlock, RuntimeError};

pub const PARSER_VERSION: &str = "fb2-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analysis {
    pub blocks: Vec<DocumentBlock>,
    pub diagnostics: Vec<String>,
}

pub fn analyze(source: &[u8]) -> Result<Analysis, RuntimeError> {
    xml::analyze(source)
}

pub fn export(source: &[u8], blocks: &[DocumentBlock]) -> Result<Vec<u8>, RuntimeError> {
    xml::export(source, &coalesce_segments(blocks)?)
}

fn coalesce_segments(blocks: &[DocumentBlock]) -> Result<Vec<DocumentBlock>, RuntimeError> {
    let mut result: Vec<DocumentBlock> = Vec::new();
    for block in blocks {
        let Some((parent_id, _)) = block.id.rsplit_once("::part-") else {
            result.push(block.clone());
            continue;
        };
        if let Some(parent) = result.iter_mut().find(|item| item.id == parent_id) {
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
            "FB2 job has untranslated blocks".into(),
        ));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{analyze, export};
    use crate::documents::BlockType;

    const FIXTURE: &[u8] = include_bytes!("../../../../../docs/fixtures/fb2/sample.fb2");

    #[test]
    fn analysis_collects_section_headings_paragraphs_epigraphs_and_notes_in_order() {
        let analysis = analyze(FIXTURE).unwrap();

        assert_eq!(
            analysis
                .blocks
                .iter()
                .map(|block| block.source_text.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Signal Path",
                "Measure twice, translate once.",
                "The receiver converts the input signal before the second stage.",
                "Keep ADC-02 unchanged.",
                "Note",
                "[1] This is a self-authored fixture.",
                "Translator note",
                "The note remains linked to its reference."
            ]
        );
        assert_eq!(analysis.blocks[0].block_type, BlockType::Heading);
        assert_eq!(analysis.blocks[1].block_type, BlockType::Paragraph);
        assert_eq!(analysis.blocks[7].id, "fb2#7");
    }

    #[test]
    fn export_rewrites_book_text_and_preserves_metadata_links_and_binary_resources() {
        let analysis = analyze(FIXTURE).unwrap();
        let mut blocks = analysis.blocks;
        for block in &mut blocks {
            block.translated_text = Some(format!("translated {}", block.ordinal));
        }

        let output = export(FIXTURE, &blocks).unwrap();
        let exported = String::from_utf8(output).unwrap();
        let reanalyzed = analyze(exported.as_bytes()).unwrap();

        assert_eq!(reanalyzed.blocks[0].source_text, "translated 0");
        assert_eq!(reanalyzed.blocks[7].source_text, "translated 7");
        assert!(exported.contains("<book-title>Signal Path Fixture</book-title>"));
        assert!(exported.contains("l:href=\"#n1\""));
        assert!(exported.contains("l:href=\"#cover\""));
        assert!(
            exported.contains("<binary id=\"cover\" content-type=\"image/png\">aGVsbG8=</binary>")
        );
    }

    #[test]
    fn leaves_external_entity_paragraph_untranslated_without_resolving_it() {
        let source = br#"<?xml version="1.0"?><!DOCTYPE FictionBook SYSTEM "https://example.test/fb2.dtd"><FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0"><body><section><p>Text with &external; entity.</p><p>Safe paragraph.</p></section></body></FictionBook>"#;
        let analysis = analyze(source).unwrap();

        assert_eq!(analysis.blocks.len(), 1);
        assert_eq!(analysis.blocks[0].source_text, "Safe paragraph.");
        assert!(analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("entity")));

        let mut blocks = analysis.blocks;
        blocks[0].translated_text = Some("Safe translation.".into());
        let output = export(source, &blocks).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("&external;"));
        assert!(output.contains("Safe translation."));
    }

    #[test]
    fn unsupported_poetry_is_diagnosed_and_preserved_on_export() {
        let source = br#"<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0"><body><section><p>Supported paragraph.</p><poem><stanza><v>Unsupported verse.</v></stanza></poem></section></body></FictionBook>"#;
        let analysis = analyze(source).unwrap();
        assert_eq!(analysis.blocks.len(), 1);
        assert!(analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("poem")));

        let mut blocks = analysis.blocks;
        blocks[0].translated_text = Some("Translated paragraph.".into());
        let output = export(source, &blocks).unwrap();
        let output = String::from_utf8(output).unwrap();

        assert!(output.contains("<v>Unsupported verse.</v>"));
        assert!(output.contains("Translated paragraph."));
    }

    #[test]
    fn does_not_translate_paragraphs_inside_tables_or_unknown_containers() {
        let source = br#"<!DOCTYPE FictionBook SYSTEM "fb2.dtd"><FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0"><body><section><table><tr><td><p>Table cell.</p></td></tr></table><custom><p>Custom paragraph.</p></custom><annotation>&external;</annotation><p>Regular paragraph.</p></section></body></FictionBook>"#;

        let analysis = analyze(source).unwrap();

        assert_eq!(
            analysis
                .blocks
                .iter()
                .map(|block| block.source_text.as_str())
                .collect::<Vec<_>>(),
            vec!["Regular paragraph."]
        );
        assert!(analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("table")));
        assert!(analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("custom")));
        assert!(analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("annotation")));

        let mut blocks = analysis.blocks;
        blocks[0].translated_text = Some("Обычный абзац.".into());
        let output = export(source, &blocks).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("Table cell."));
        assert!(output.contains("Custom paragraph."));
        assert!(output.contains("&external;"));
        assert!(output.contains("Обычный абзац."));
    }

    #[test]
    fn diagnoses_paragraphs_with_inline_formatting_boundaries() {
        let source = br#"<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0"><body><section><p>Keep <strong>ADC-02</strong> unchanged.</p></section></body></FictionBook>"#;

        let analysis = analyze(source).unwrap();

        assert_eq!(analysis.blocks[0].source_text, "Keep ADC-02 unchanged.");
        assert!(analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("formatting boundaries")));

        let mut blocks = analysis.blocks;
        blocks[0].translated_text = Some("Preserve the ADC-02 technical identifier.".into());
        let output = export(source, &blocks).unwrap();
        let output = String::from_utf8(output).unwrap();
        let translated = analyze(output.as_bytes()).unwrap();

        assert_eq!(
            translated.blocks[0].source_text,
            "Preserve the ADC-02 technical identifier."
        );
        assert!(output.contains("<strong>"));
    }

    #[test]
    fn export_coalesces_translated_segments_before_rewriting_paragraph() {
        let analysis = analyze(FIXTURE).unwrap();
        let first_id = analysis.blocks[2].id.clone();
        let first_ordinal = analysis.blocks[2].ordinal;
        let first_type = analysis.blocks[2].block_type;
        let mut blocks = analysis.blocks;
        blocks.remove(2);
        blocks.push(crate::documents::DocumentBlock {
            id: first_id.clone() + "::part-0000",
            ordinal: first_ordinal * 1_000_000,
            block_type: first_type,
            source_text: "first part".into(),
            translated_text: Some("первая".into()),
        });
        blocks.push(crate::documents::DocumentBlock {
            id: first_id + "::part-0001",
            ordinal: first_ordinal * 1_000_000 + 1,
            block_type: first_type,
            source_text: "second part".into(),
            translated_text: Some("вторая".into()),
        });
        for block in &mut blocks {
            if block.translated_text.is_none() {
                block.translated_text = Some(block.source_text.clone());
            }
        }

        let output = export(FIXTURE, &blocks).unwrap();
        let reanalyzed = analyze(&output).unwrap();

        assert_eq!(reanalyzed.blocks[2].source_text, "первая вторая");
    }
}
