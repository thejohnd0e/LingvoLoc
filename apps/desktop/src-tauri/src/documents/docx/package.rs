use std::collections::HashSet;
use std::io::{Cursor, Read, Write};

use crate::domain::RuntimeError;
use zip::write::{SimpleFileOptions, ZipWriter};
use zip::{CompressionMethod, ZipArchive};

const MAX_PACKAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 48 * 1024 * 1024;
const MAX_DOCUMENT_BYTES: u64 = 8 * 1024 * 1024;

pub fn read_main_document(bytes: &[u8]) -> Result<Vec<u8>, RuntimeError> {
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err(RuntimeError::InvalidInput(
            "DOCX package is too large".into(),
        ));
    }
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|error| RuntimeError::InvalidInput(format!("invalid DOCX package: {error}")))?;
    if archive.len() > MAX_ENTRIES {
        return Err(RuntimeError::InvalidInput(
            "DOCX package has too many entries".into(),
        ));
    }

    let mut names = HashSet::with_capacity(archive.len());
    let mut total = 0_u64;
    let mut main = None;
    let mut has_content_types = false;
    let mut has_root_relationships = false;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| RuntimeError::InvalidInput(format!("invalid DOCX entry: {error}")))?;
        let name = entry.name().to_string();
        validate_name(&name)?;
        if !names.insert(name.clone()) {
            return Err(RuntimeError::InvalidInput(format!(
                "duplicate DOCX entry: {name}"
            )));
        }
        if entry.encrypted() {
            return Err(RuntimeError::InvalidInput(format!(
                "encrypted DOCX entry: {name}"
            )));
        }
        let declared = entry.size();
        if declared > MAX_ENTRY_BYTES || total.saturating_add(declared) > MAX_TOTAL_BYTES {
            return Err(RuntimeError::InvalidInput(
                "DOCX package exceeds size limits".into(),
            ));
        }
        total = total.saturating_add(declared);
        let mut content = Vec::new();
        entry.read_to_end(&mut content).map_err(|error| {
            RuntimeError::InvalidInput(format!("cannot read DOCX entry {name}: {error}"))
        })?;
        if content.len() as u64 > MAX_ENTRY_BYTES {
            return Err(RuntimeError::InvalidInput(
                "DOCX entry exceeds size limit".into(),
            ));
        }
        match name.as_str() {
            "[Content_Types].xml" => has_content_types = true,
            "_rels/.rels" => has_root_relationships = true,
            "word/document.xml" => {
                if content.len() as u64 > MAX_DOCUMENT_BYTES {
                    return Err(RuntimeError::InvalidInput(
                        "word/document.xml exceeds size limit".into(),
                    ));
                }
                main = Some(content);
            }
            _ => {}
        }
    }

    if !has_content_types || !has_root_relationships {
        return Err(RuntimeError::InvalidInput(
            "DOCX package is missing required OPC entries".into(),
        ));
    }
    main.ok_or_else(|| {
        RuntimeError::InvalidInput("DOCX package is missing word/document.xml".into())
    })
}

fn validate_name(name: &str) -> Result<(), RuntimeError> {
    if name.is_empty()
        || name.contains('\\')
        || name.starts_with('/')
        || name.split('/').any(|part| part == "..")
    {
        return Err(RuntimeError::InvalidInput(format!(
            "unsafe DOCX entry name: {name}"
        )));
    }
    Ok(())
}

pub fn rewrite_package(source: &[u8], document: &[u8]) -> Result<Vec<u8>, RuntimeError> {
    let mut input = ZipArchive::new(Cursor::new(source))
        .map_err(|error| RuntimeError::InvalidInput(format!("invalid DOCX package: {error}")))?;
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for index in 0..input.len() {
        let entry = input
            .by_index(index)
            .map_err(|error| RuntimeError::InvalidInput(format!("invalid DOCX entry: {error}")))?;
        let name = entry.name().to_string();
        if name == "word/document.xml" {
            output.start_file(name, options).map_err(|error| {
                RuntimeError::Connection(format!("start DOCX document entry: {error}"))
            })?;
            output.write_all(document).map_err(|error| {
                RuntimeError::Connection(format!("write DOCX document entry: {error}"))
            })?;
        } else {
            output
                .raw_copy_file(entry)
                .map_err(|error| RuntimeError::Connection(format!("copy DOCX entry: {error}")))?;
        }
    }
    output
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(|error| RuntimeError::Connection(format!("finish DOCX package: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    const CONTENT_TYPES: &[u8] =
        include_bytes!("../../../../../../docs/fixtures/docx/[Content_Types].xml");
    const ROOT_RELS: &[u8] = include_bytes!("../../../../../../docs/fixtures/docx/_rels/.rels");
    const DOCUMENT: &[u8] =
        include_bytes!("../../../../../../docs/fixtures/docx/word/document.xml");
    const DOCUMENT_RELS: &[u8] =
        include_bytes!("../../../../../../docs/fixtures/docx/word/_rels/document.xml.rels");
    const IMAGE: &[u8] =
        include_bytes!("../../../../../../docs/fixtures/docx/word/media/diagram.svg");
    const NUMBERING: &[u8] =
        include_bytes!("../../../../../../docs/fixtures/docx/word/numbering.xml");

    fn fixture_package(include_numbering: bool) -> Vec<u8> {
        let mut output = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        let entries = [
            ("[Content_Types].xml", CONTENT_TYPES),
            ("_rels/.rels", ROOT_RELS),
            ("word/document.xml", DOCUMENT),
            ("word/_rels/document.xml.rels", DOCUMENT_RELS),
            ("word/media/diagram.svg", IMAGE),
        ];
        for (name, content) in entries {
            output.start_file(name, options).unwrap();
            output.write_all(content).unwrap();
        }
        if include_numbering {
            output.start_file("word/numbering.xml", options).unwrap();
            output.write_all(NUMBERING).unwrap();
        }
        output.finish().unwrap().into_inner()
    }

    #[test]
    fn fixture_builder_keeps_structural_entries_and_optional_numbering() {
        let with_numbering = fixture_package(true);
        let mut archive = ZipArchive::new(Cursor::new(with_numbering)).unwrap();
        let names = (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().to_string())
            .collect::<HashSet<_>>();
        assert!(names.contains("[Content_Types].xml"));
        assert!(names.contains("_rels/.rels"));
        assert!(names.contains("word/document.xml"));
        assert!(names.contains("word/_rels/document.xml.rels"));
        assert!(names.contains("word/media/diagram.svg"));
        assert!(names.contains("word/numbering.xml"));

        let without_numbering = fixture_package(false);
        assert!(crate::documents::docx::analyze(&without_numbering).is_ok());
    }

    #[test]
    fn rejects_unsafe_entry_names() {
        for name in [
            "../word/document.xml",
            "/word/document.xml",
            "word\\document.xml",
        ] {
            assert!(validate_name(name).is_err(), "accepted unsafe name: {name}");
        }
    }

    #[test]
    fn rejects_duplicate_and_missing_required_entries() {
        let mut duplicate = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        duplicate
            .start_file("[Content_Types].xml", options)
            .unwrap();
        duplicate.write_all(CONTENT_TYPES).unwrap();
        assert!(duplicate
            .start_file("[Content_Types].xml", options)
            .is_err());

        let mut missing = ZipWriter::new(Cursor::new(Vec::new()));
        missing.start_file("[Content_Types].xml", options).unwrap();
        missing.write_all(CONTENT_TYPES).unwrap();
        missing.start_file("_rels/.rels", options).unwrap();
        missing.write_all(ROOT_RELS).unwrap();
        assert!(read_main_document(&missing.finish().unwrap().into_inner()).is_err());
    }

    #[test]
    fn export_preserves_non_document_entries() {
        let source = fixture_package(true);
        let main_xml = read_main_document(&source).unwrap();
        let mut analysis = super::super::xml::parse_document(&main_xml).unwrap();
        for block in &mut analysis.blocks {
            block.translated_text = Some(format!("translated: {}", block.source_text));
        }
        let output = crate::documents::docx::export(&source, &analysis.blocks).unwrap();
        let mut original = ZipArchive::new(Cursor::new(source)).unwrap();
        let mut rewritten = ZipArchive::new(Cursor::new(output)).unwrap();
        assert_eq!(original.len(), rewritten.len());
        for index in 0..original.len() {
            let mut left = Vec::new();
            let mut right = Vec::new();
            let left_name = original.by_index(index).unwrap().name().to_string();
            original
                .by_index(index)
                .unwrap()
                .read_to_end(&mut left)
                .unwrap();
            rewritten
                .by_name(&left_name)
                .unwrap()
                .read_to_end(&mut right)
                .unwrap();
            if left_name != "word/document.xml" {
                assert_eq!(left, right, "entry changed unexpectedly: {left_name}");
            }
        }
    }
}
