mod package;
mod xml;

use std::io::{Cursor, Read, Write};

use super::{BlockType, DocumentBlock};
use crate::domain::RuntimeError;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

pub const PARSER_VERSION: &str = "epub-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpineEntry {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analysis {
    pub blocks: Vec<DocumentBlock>,
    pub diagnostics: Vec<String>,
    pub spine: Vec<SpineEntry>,
}

pub fn analyze(bytes: &[u8]) -> Result<Analysis, RuntimeError> {
    let package = package::read_package(bytes)?;
    let mut blocks = Vec::new();
    let mut diagnostics = package.diagnostics.clone();
    for spine_entry in &package.spine {
        let document = package.entry(&spine_entry.path)?;
        let parsed = xml::parse_document(document, &spine_entry.path)?;
        blocks.extend(parsed.blocks);
        diagnostics.extend(parsed.diagnostics);
    }
    Ok(Analysis {
        blocks,
        diagnostics,
        spine: package.spine,
    })
}

pub fn export(source: &[u8], blocks: &[DocumentBlock]) -> Result<Vec<u8>, RuntimeError> {
    let package = package::read_package(source)?;
    let blocks = coalesce_segments(blocks)?;
    let mut archive = ZipArchive::new(Cursor::new(source))
        .map_err(|error| RuntimeError::InvalidInput(format!("invalid EPUB package: {error}")))?;
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| RuntimeError::InvalidInput(format!("invalid EPUB entry: {error}")))?;
        let name = entry.name().to_string();
        if package.spine.iter().any(|spine| spine.path == name) {
            let mut content = Vec::new();
            entry.read_to_end(&mut content).map_err(|error| {
                RuntimeError::InvalidInput(format!("cannot read EPUB entry {name}: {error}"))
            })?;
            let document_blocks = blocks
                .iter()
                .filter(|block| block.id.starts_with(&format!("{name}#")))
                .cloned()
                .collect::<Vec<_>>();
            let rewritten = xml::rewrite_document(&content, &name, &document_blocks)?;
            output
                .start_file(name, SimpleFileOptions::default())
                .map_err(zip_error)?;
            output.write_all(&rewritten).map_err(write_error)?;
        } else {
            output.raw_copy_file(entry).map_err(zip_error)?;
        }
    }

    output
        .finish()
        .map_err(zip_error)
        .map(|cursor| cursor.into_inner())
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
            "EPUB job has untranslated blocks".into(),
        ));
    }
    Ok(result)
}

fn zip_error(error: zip::result::ZipError) -> RuntimeError {
    RuntimeError::Connection(format!("EPUB ZIP error: {error}"))
}

fn write_error(error: std::io::Error) -> RuntimeError {
    RuntimeError::Connection(format!("EPUB write error: {error}"))
}

#[allow(dead_code)]
fn block(id: String, ordinal: usize, block_type: BlockType, source_text: String) -> DocumentBlock {
    DocumentBlock {
        id,
        ordinal: ordinal as i64,
        block_type,
        source_text,
        translated_text: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_is_analyzed_in_spine_order() {
        let analysis = analyze(&super::package::fixture_bytes()).unwrap();

        assert_eq!(
            analysis
                .spine
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>(),
            vec!["OPS/chapter-one.xhtml", "OPS/chapter-two.xhtml"]
        );
        assert_eq!(analysis.blocks[0].id, "OPS/chapter-one.xhtml#0");
        assert_eq!(analysis.blocks[0].source_text, "First chapter");
        assert_eq!(analysis.blocks[1].source_text, "An important paragraph.");
        assert_eq!(analysis.blocks[2].source_text, "First list item");
        assert_eq!(analysis.blocks[4].source_text, "First cell");
        assert_eq!(
            analysis.blocks.last().unwrap().id,
            "OPS/chapter-two.xhtml#1"
        );
        assert!(analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("script")));
        assert!(analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("foreign")));
    }

    #[test]
    fn export_preserves_non_xhtml_entries_and_rewrites_both_spine_documents() {
        let source = super::package::fixture_bytes();
        let analysis = analyze(&source).unwrap();
        let mut blocks = analysis.blocks.clone();
        for block in &mut blocks {
            block.translated_text = Some(format!("<translated {}>", block.ordinal));
        }

        let exported = export(&source, &blocks).unwrap();
        let mut original = zip::ZipArchive::new(std::io::Cursor::new(source)).unwrap();
        let mut rewritten = zip::ZipArchive::new(std::io::Cursor::new(exported)).unwrap();
        for name in [
            "mimetype",
            "OPS/content.opf",
            "OPS/styles.css",
            "OPS/images/diagram.svg",
        ] {
            let mut before = Vec::new();
            let mut after = Vec::new();
            original
                .by_name(name)
                .unwrap()
                .read_to_end(&mut before)
                .unwrap();
            rewritten
                .by_name(name)
                .unwrap()
                .read_to_end(&mut after)
                .unwrap();
            assert_eq!(before, after, "changed untouched EPUB entry {name}");
        }
        let mut chapter_two = String::new();
        rewritten
            .by_name("OPS/chapter-two.xhtml")
            .unwrap()
            .read_to_string(&mut chapter_two)
            .unwrap();
        assert!(chapter_two.contains("&lt;translated 0&gt;"));
    }

    #[test]
    fn coalesces_segmented_epub_blocks_before_export() {
        let blocks = vec![
            DocumentBlock {
                id: "OPS/chapter.xhtml#1::part-0000".into(),
                ordinal: 1_000_000,
                block_type: BlockType::Paragraph,
                source_text: "one".into(),
                translated_text: Some("first".into()),
            },
            DocumentBlock {
                id: "OPS/chapter.xhtml#1::part-0001".into(),
                ordinal: 1_000_001,
                block_type: BlockType::Paragraph,
                source_text: "two".into(),
                translated_text: Some("second".into()),
            },
            DocumentBlock {
                id: "OPS/chapter.xhtml#2".into(),
                ordinal: 2,
                block_type: BlockType::Paragraph,
                source_text: "later".into(),
                translated_text: Some("later translated".into()),
            },
        ];
        let combined = coalesce_segments(&blocks).unwrap();
        assert_eq!(
            combined
                .iter()
                .map(|block| block.id.as_str())
                .collect::<Vec<_>>(),
            vec!["OPS/chapter.xhtml#1", "OPS/chapter.xhtml#2"]
        );
        assert_eq!(combined[0].ordinal, 1);
        assert_eq!(combined[0].translated_text.as_deref(), Some("first second"));
    }

    #[test]
    fn export_keeps_multiple_segmented_blocks_in_document_order() {
        let source = super::package::fixture_bytes();
        let analysis = analyze(&source).unwrap();
        let mut blocks = analysis.blocks;
        let segmented = blocks.remove(1);
        let later = blocks[1].clone();
        blocks.push(DocumentBlock {
            id: segmented.id.clone() + "::part-0000",
            ordinal: 1_000_000,
            block_type: segmented.block_type,
            source_text: "first".into(),
            translated_text: Some("first translated".into()),
        });
        blocks.push(DocumentBlock {
            id: segmented.id + "::part-0001",
            ordinal: 1_000_001,
            block_type: later.block_type,
            source_text: "second".into(),
            translated_text: Some("second translated".into()),
        });
        for block in &mut blocks {
            if block.translated_text.is_none() {
                block.translated_text = Some(block.source_text.clone());
            }
        }

        let exported = export(&source, &blocks).unwrap();
        let chapter = zip::ZipArchive::new(std::io::Cursor::new(exported))
            .unwrap()
            .by_name("OPS/chapter-one.xhtml")
            .unwrap()
            .bytes()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let parsed = super::xml::parse_document(&chapter, "OPS/chapter-one.xhtml").unwrap();
        assert_eq!(
            parsed.blocks[1].source_text,
            "first translated second translated"
        );
        assert_eq!(parsed.blocks[2].source_text, later.source_text);
    }
}
