use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};

use super::SpineEntry;
use crate::domain::RuntimeError;
use zip::ZipArchive;

const MAX_PACKAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_ENTRY_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 48 * 1024 * 1024;

pub(super) struct Package {
    entries: HashMap<String, Vec<u8>>,
    pub(super) spine: Vec<SpineEntry>,
    pub(super) diagnostics: Vec<String>,
}

impl Package {
    pub(super) fn entry(&self, path: &str) -> Result<&[u8], RuntimeError> {
        self.entries
            .get(path)
            .map(Vec::as_slice)
            .ok_or_else(|| RuntimeError::InvalidInput(format!("missing EPUB entry: {path}")))
    }
}

pub(super) fn read_package(bytes: &[u8]) -> Result<Package, RuntimeError> {
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err(RuntimeError::InvalidInput(
            "EPUB package is too large".into(),
        ));
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| RuntimeError::InvalidInput(format!("invalid EPUB package: {error}")))?;
    if archive.len() > MAX_ENTRIES {
        return Err(RuntimeError::InvalidInput(
            "EPUB package has too many entries".into(),
        ));
    }

    let mut entries = HashMap::with_capacity(archive.len());
    let mut names = HashSet::with_capacity(archive.len());
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| RuntimeError::InvalidInput(format!("invalid EPUB entry: {error}")))?;
        let name = entry.name().to_string();
        validate_name(&name)?;
        if !names.insert(name.clone()) {
            return Err(RuntimeError::InvalidInput(format!(
                "duplicate EPUB entry: {name}"
            )));
        }
        if entry.encrypted() {
            return Err(RuntimeError::InvalidInput(format!(
                "encrypted EPUB entry: {name}"
            )));
        }
        validate_limits(entry.size(), total)?;
        total = total.saturating_add(entry.size());
        let mut content = Vec::new();
        entry.read_to_end(&mut content).map_err(|error| {
            RuntimeError::InvalidInput(format!("cannot read EPUB entry {name}: {error}"))
        })?;
        validate_limits(content.len() as u64, total.saturating_sub(entry.size()))?;
        entries.insert(name, content);
    }

    if let Some(mimetype) = entries.get("mimetype") {
        if mimetype.as_slice() != b"application/epub+zip" {
            return Err(RuntimeError::InvalidInput("invalid EPUB mimetype".into()));
        }
    }
    let container = entries.get("META-INF/container.xml").ok_or_else(|| {
        RuntimeError::InvalidInput("EPUB package is missing META-INF/container.xml".into())
    })?;
    let rootfile = super::xml::parse_container(container)?;
    let opf = entries.get(&rootfile).ok_or_else(|| {
        RuntimeError::InvalidInput(format!("EPUB package is missing rootfile: {rootfile}"))
    })?;
    let (manifest, spine) = super::xml::parse_opf(opf, &rootfile)?;
    let mut resolved_spine = Vec::with_capacity(spine.len());
    for id in spine {
        let item = manifest.get(&id).ok_or_else(|| {
            RuntimeError::InvalidInput(format!("EPUB spine references missing manifest item: {id}"))
        })?;
        if item.media_type != "application/xhtml+xml" && item.media_type != "text/html" {
            return Err(RuntimeError::InvalidInput(format!(
                "EPUB spine item is not XHTML: {id}"
            )));
        }
        let path = resolve_path(&rootfile, &item.href)?;
        if !entries.contains_key(&path) {
            return Err(RuntimeError::InvalidInput(format!(
                "EPUB spine resource is missing: {path}"
            )));
        }
        resolved_spine.push(SpineEntry { id, path });
    }
    Ok(Package {
        entries,
        spine: resolved_spine,
        diagnostics: Vec::new(),
    })
}

fn validate_name(name: &str) -> Result<(), RuntimeError> {
    if name.is_empty()
        || name.contains('\\')
        || name.starts_with('/')
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(RuntimeError::InvalidInput(format!(
            "unsafe EPUB entry name: {name}"
        )));
    }
    Ok(())
}

fn validate_limits(entry: u64, total: u64) -> Result<(), RuntimeError> {
    if entry > MAX_ENTRY_BYTES || total.saturating_add(entry) > MAX_TOTAL_BYTES {
        return Err(RuntimeError::InvalidInput(
            "EPUB package exceeds size limits".into(),
        ));
    }
    Ok(())
}

fn resolve_path(rootfile: &str, href: &str) -> Result<String, RuntimeError> {
    if href.is_empty() || href.contains('#') || href.contains('\\') || href.starts_with('/') {
        return Err(RuntimeError::InvalidInput(format!(
            "unsafe EPUB resource path: {href}"
        )));
    }
    let base = rootfile
        .rsplit_once('/')
        .map(|(base, _)| base)
        .unwrap_or("");
    let path = if base.is_empty() {
        href.to_string()
    } else {
        format!("{base}/{href}")
    };
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(RuntimeError::InvalidInput(format!(
                        "EPUB resource escapes package: {href}"
                    )));
                }
            }
            value if value.contains(':') => {
                return Err(RuntimeError::InvalidInput(format!(
                    "external EPUB resource: {href}"
                )));
            }
            value => parts.push(value),
        }
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
pub(super) fn fixture_bytes() -> Vec<u8> {
    use std::io::{Cursor, Write};
    use zip::{write::SimpleFileOptions, ZipWriter};
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in [
        (
            "OPS/chapter-two.xhtml",
            include_bytes!("../../../../../../docs/fixtures/epub/OPS/chapter-two.xhtml").as_slice(),
        ),
        (
            "OPS/styles.css",
            include_bytes!("../../../../../../docs/fixtures/epub/OPS/styles.css").as_slice(),
        ),
        (
            "OPS/images/diagram.svg",
            include_bytes!("../../../../../../docs/fixtures/epub/OPS/images/diagram.svg")
                .as_slice(),
        ),
        (
            "OPS/content.opf",
            include_bytes!("../../../../../../docs/fixtures/epub/OPS/content.opf").as_slice(),
        ),
        (
            "OPS/chapter-one.xhtml",
            include_bytes!("../../../../../../docs/fixtures/epub/OPS/chapter-one.xhtml").as_slice(),
        ),
        (
            "META-INF/container.xml",
            include_bytes!("../../../../../../docs/fixtures/epub/META-INF/container.xml")
                .as_slice(),
        ),
        ("mimetype", b"application/epub+zip".as_slice()),
    ] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(content).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_has_required_package_entries() {
        let package = read_package(&fixture_bytes()).unwrap();
        assert_eq!(package.spine[0].path, "OPS/chapter-one.xhtml");
        assert_eq!(package.spine[1].path, "OPS/chapter-two.xhtml");
    }

    #[test]
    fn rejects_missing_container_and_opf() {
        assert_invalid_archive(&[("mimetype", b"application/epub+zip")]);
        assert_invalid_archive(&[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", b"<container/>"),
        ]);
    }

    #[test]
    fn rejects_unsafe_and_duplicate_names() {
        for name in [
            "../OPS/chapter.xhtml",
            "/OPS/chapter.xhtml",
            "OPS\\chapter.xhtml",
        ] {
            assert!(validate_name(name).is_err(), "accepted unsafe path {name}");
        }

        assert!(archive_with_duplicate_name().is_err());
    }

    #[test]
    fn rejects_encrypted_entries() {
        let mut archive = fixture_archive(&[("mimetype", b"application/epub+zip")]);
        mark_first_entry_encrypted(&mut archive);
        assert!(read_package(&archive).is_err());
    }

    #[test]
    fn rejects_entry_and_total_limits_before_reading_content() {
        assert!(validate_limits(MAX_ENTRY_BYTES + 1, 0).is_err());
        assert!(validate_limits(1, MAX_TOTAL_BYTES).is_err());
    }

    #[test]
    fn rejects_remote_or_missing_spine_resources() {
        let opf = b"<package xmlns=\"http://www.idpf.org/2007/opf\"><manifest><item id=\"c\" href=\"https://example.com/c.xhtml\" media-type=\"application/xhtml+xml\"/></manifest><spine><itemref idref=\"c\"/></spine></package>";
        let archive = fixture_archive(&[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", CONTAINER),
            ("OPS/content.opf", opf),
        ]);
        assert!(read_package(&archive).is_err());
    }

    const CONTAINER: &[u8] = br#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;

    fn assert_invalid_archive(entries: &[(&str, &[u8])]) {
        assert!(read_package(&fixture_archive(entries)).is_err());
    }

    fn fixture_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::{Cursor, Write};
        use zip::{write::SimpleFileOptions, ZipWriter};
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn archive_with_duplicate_name() -> Result<Vec<u8>, zip::result::ZipError> {
        use std::io::{Cursor, Write};
        use zip::{write::SimpleFileOptions, ZipWriter};
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("mimetype", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"application/epub+zip").unwrap();
        writer.start_file("mimetype", SimpleFileOptions::default())?;
        writer.write_all(b"application/epub+zip").unwrap();
        Ok(writer.finish().unwrap().into_inner())
    }

    fn mark_first_entry_encrypted(bytes: &mut [u8]) {
        let local = bytes
            .windows(4)
            .position(|window| window == b"PK\x03\x04")
            .unwrap();
        let central = bytes
            .windows(4)
            .position(|window| window == b"PK\x01\x02")
            .unwrap();
        bytes[local + 6] |= 1;
        bytes[central + 8] |= 1;
    }
}
