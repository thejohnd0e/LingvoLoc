mod package;
mod xml;

use super::{BlockType, DocumentBlock};
use crate::domain::RuntimeError;

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
}
