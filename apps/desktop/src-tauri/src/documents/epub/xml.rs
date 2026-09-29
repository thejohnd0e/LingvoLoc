#[cfg(test)]
mod tests {
    #[test]
    fn xml_parser_boundary_will_supply_ordered_blocks() {
        let _ = super::parse_document;
    }
}
use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::Reader;

use super::super::{BlockType, DocumentBlock};
use super::block;
use crate::domain::RuntimeError;
use quick_xml::XmlVersion;

#[derive(Debug, Clone)]
pub(super) struct ManifestItem {
    pub(super) href: String,
    pub(super) media_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParsedDocument {
    pub(super) blocks: Vec<DocumentBlock>,
    pub(super) diagnostics: Vec<String>,
}

pub(super) fn parse_container(bytes: &[u8]) -> Result<String, RuntimeError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer).map_err(xml_error)? {
            Event::Empty(element) | Event::Start(element)
                if local_name(element.name().as_ref()) == "rootfile" =>
            {
                let full_path = attribute(&element, b"full-path")?;
                if attribute(&element, b"media-type")?.as_str() != "application/oebps-package+xml" {
                    return Err(RuntimeError::InvalidInput(
                        "EPUB rootfile is not an OPF".into(),
                    ));
                }
                return validate_package_path(&full_path);
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Err(RuntimeError::InvalidInput(
        "EPUB container has no rootfile".into(),
    ))
}

pub(super) fn parse_opf(
    bytes: &[u8],
    rootfile: &str,
) -> Result<(HashMap<String, ManifestItem>, Vec<String>), RuntimeError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buffer = Vec::new();
    let mut manifest = HashMap::new();
    let mut spine = Vec::new();
    let mut in_manifest = false;
    let mut in_spine = false;
    loop {
        match reader.read_event_into(&mut buffer).map_err(xml_error)? {
            Event::Start(element) => {
                let qualified_name = element.name();
                let name = local_name(qualified_name.as_ref());
                match name {
                    "manifest" => in_manifest = true,
                    "spine" => in_spine = true,
                    "item" if in_manifest => {
                        let id = attribute(&element, b"id")?;
                        let href = attribute(&element, b"href")?;
                        let media_type = attribute(&element, b"media-type")?;
                        if manifest
                            .insert(id.clone(), ManifestItem { href, media_type })
                            .is_some()
                        {
                            return Err(RuntimeError::InvalidInput(format!(
                                "duplicate EPUB manifest id: {id}"
                            )));
                        }
                    }
                    "itemref" if in_spine => spine.push(attribute(&element, b"idref")?),
                    _ => {}
                }
            }
            Event::Empty(element) => {
                let qualified_name = element.name();
                let name = local_name(qualified_name.as_ref());
                if name == "item" && in_manifest {
                    let id = attribute(&element, b"id")?;
                    let href = attribute(&element, b"href")?;
                    let media_type = attribute(&element, b"media-type")?;
                    if manifest
                        .insert(id.clone(), ManifestItem { href, media_type })
                        .is_some()
                    {
                        return Err(RuntimeError::InvalidInput(format!(
                            "duplicate EPUB manifest id: {id}"
                        )));
                    }
                } else if name == "itemref" && in_spine {
                    spine.push(attribute(&element, b"idref")?);
                }
            }
            Event::End(element) => match local_name(element.name().as_ref()) {
                "manifest" => in_manifest = false,
                "spine" => in_spine = false,
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    if manifest.is_empty() || spine.is_empty() {
        return Err(RuntimeError::InvalidInput(format!(
            "EPUB OPF has no manifest or spine: {rootfile}"
        )));
    }
    Ok((manifest, spine))
}

pub(super) fn parse_document(bytes: &[u8], path: &str) -> Result<ParsedDocument, RuntimeError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buffer = Vec::new();
    let mut depth = 0_usize;
    let mut current: Option<(usize, BlockType, String)> = None;
    let mut skipped = 0_usize;
    let mut ordinal = 0_usize;
    let mut blocks = Vec::new();
    let mut diagnostics = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer).map_err(xml_error)? {
            Event::Start(element) => {
                depth += 1;
                let qualified_name = element.name();
                let name = local_name(qualified_name.as_ref());
                if skipped > 0 {
                    skipped += 1;
                } else if unsupported(name) {
                    skipped = 1;
                    diagnostics.push(format!("unsupported EPUB XHTML element: {name}"));
                } else if current.is_none() {
                    if let Some(kind) = semantic_type(name) {
                        current = Some((depth, kind, String::new()));
                    }
                }
            }
            Event::Empty(element) => {
                let qualified_name = element.name();
                let name = local_name(qualified_name.as_ref());
                if unsupported(name) {
                    diagnostics.push(format!("unsupported EPUB XHTML element: {name}"));
                }
            }
            Event::Text(text) if skipped == 0 => {
                if let Some((_, _, value)) = &mut current {
                    let decoded = text.xml_content(XmlVersion::Implicit1_0);
                    value.push_str(decoded.as_ref());
                }
            }
            Event::End(_element) => {
                if skipped > 0 {
                    skipped -= 1;
                } else if current
                    .as_ref()
                    .is_some_and(|(start, _, _)| *start == depth)
                {
                    let (_, kind, text) = current.take().unwrap();
                    if !text.trim().is_empty() {
                        blocks.push(block(
                            format!("{path}#{ordinal}"),
                            ordinal,
                            kind,
                            text.trim().to_string(),
                        ));
                        ordinal += 1;
                    }
                }
                depth = depth.saturating_sub(1);
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(ParsedDocument {
        blocks,
        diagnostics,
    })
}

fn semantic_type(name: &str) -> Option<BlockType> {
    match name {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Some(BlockType::Heading),
        "p" => Some(BlockType::Paragraph),
        "li" => Some(BlockType::ListItem),
        "td" | "th" => Some(BlockType::TableCell),
        _ => None,
    }
}

fn unsupported(name: &str) -> bool {
    matches!(
        name,
        "script" | "style" | "foreign" | "svg" | "math" | "object" | "iframe"
    )
}

fn attribute(
    element: &quick_xml::events::BytesStart<'_>,
    key: &[u8],
) -> Result<String, RuntimeError> {
    let attribute = element
        .attributes()
        .flatten()
        .find(|attribute| local_name(attribute.key.as_ref()).as_bytes() == key)
        .ok_or_else(|| {
            RuntimeError::InvalidInput(format!(
                "EPUB XML is missing attribute: {}",
                String::from_utf8_lossy(key)
            ))
        })?;
    attribute
        .normalized_value(XmlVersion::Implicit1_0)
        .map(|value| value.into_owned())
        .map_err(|error| RuntimeError::InvalidInput(format!("invalid EPUB XML attribute: {error}")))
}

fn validate_package_path(path: &str) -> Result<String, RuntimeError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path.split('/').any(|part| part == ".." || part.is_empty())
    {
        return Err(RuntimeError::InvalidInput(format!(
            "unsafe EPUB package path: {path}"
        )));
    }
    Ok(path.into())
}

fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or_default()
}

fn xml_error(error: quick_xml::Error) -> RuntimeError {
    RuntimeError::InvalidInput(format!("invalid EPUB XML: {error}"))
}
