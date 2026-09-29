#[cfg(test)]
mod tests {
    use super::super::super::{BlockType, DocumentBlock};

    #[test]
    fn xml_parser_boundary_will_supply_ordered_blocks() {
        let _ = super::parse_document;
    }

    #[test]
    fn extracts_only_xhtml_blocks_and_reports_atomic_unsupported_subtrees() {
        let xml = br#"<html xmlns="http://www.w3.org/1999/xhtml"><body>
            <h1> Heading <em>one</em> </h1>
            <p>First <a href="https://example.test">linked</a> paragraph.</p>
            <ul><li>One</li><li>Two</li></ul>
            <table><tr><th>Head</th><td>Cell</td></tr></table>
            <p>before <script>do not expose</script> after</p>
            <foreign xmlns="urn:foreign"><h1>foreign heading</h1></foreign>
        </body></html>"#;

        let parsed = super::parse_document(xml, "OPS/chapter.xhtml").unwrap();
        assert_eq!(
            parsed
                .blocks
                .iter()
                .map(|block| block.source_text.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Heading one",
                "First linked paragraph.",
                "One",
                "Two",
                "Head",
                "Cell"
            ]
        );
        assert_eq!(parsed.blocks[0].block_type, BlockType::Heading);
        assert!(parsed
            .diagnostics
            .iter()
            .any(|item| item.contains("script")));
        assert!(parsed
            .diagnostics
            .iter()
            .any(|item| item.contains("foreign")));
        assert!(!parsed
            .blocks
            .iter()
            .any(|block| block.source_text.contains("foreign")));
    }

    #[test]
    fn rewrites_text_nodes_with_escaping_and_preserves_markup_and_whitespace() {
        let xml = br#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><body><p id="keep">Original <a href="link.xhtml#anchor">link</a>.</p></body></html>"#;
        let mut block = super::parse_document(xml, "OPS/chapter.xhtml")
            .unwrap()
            .blocks
            .remove(0);
        block.translated_text = Some(" translated & <kept> ".into());

        let rewritten = super::rewrite_document(xml, "OPS/chapter.xhtml", &[block]).unwrap();
        let text = String::from_utf8(rewritten).unwrap();
        assert!(text.contains("id=\"keep\""));
        assert!(text.contains("href=\"link.xhtml#anchor\""));
        assert!(text.contains("xml:space=\"preserve\""));
        assert!(text.contains("translated &amp;") || text.contains("&amp;"));
        assert!(text.contains("&lt;") && text.contains("&gt;"));
    }

    #[test]
    fn rejects_missing_or_untranslated_rewrite_blocks() {
        let xml =
            br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Original</p></body></html>"#;
        let block = DocumentBlock {
            id: "OPS/chapter.xhtml#0".into(),
            ordinal: 0,
            block_type: BlockType::Paragraph,
            source_text: "Original".into(),
            translated_text: None,
        };
        assert!(super::rewrite_document(xml, "OPS/chapter.xhtml", &[block]).is_err());
    }

    #[test]
    fn reports_and_preserves_unsupported_ordinary_text_elements() {
        let xml = br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><blockquote>Quoted text</blockquote><p>Supported</p></body></html>"#;

        let parsed = super::parse_document(xml, "OPS/chapter.xhtml").unwrap();
        assert_eq!(parsed.blocks.len(), 1);
        assert_eq!(parsed.blocks[0].source_text, "Supported");
        assert!(parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("blockquote")));
        let mut blocks = parsed.blocks;
        blocks[0].translated_text = Some("translated".into());
        let rewritten = super::rewrite_document(xml, "OPS/chapter.xhtml", &blocks).unwrap();
        assert!(String::from_utf8(rewritten)
            .unwrap()
            .contains("<blockquote>Quoted text</blockquote>"));
    }

    #[test]
    fn inherited_foreign_namespace_is_diagnostic_and_not_translated() {
        let xml = br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><section xmlns="urn:foreign"><p>Foreign text</p></section><p>Supported</p></body></html>"#;

        let parsed = super::parse_document(xml, "OPS/chapter.xhtml").unwrap();
        assert_eq!(parsed.blocks.len(), 1);
        assert_eq!(parsed.blocks[0].source_text, "Supported");
        assert!(parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("section")));
        let mut blocks = parsed.blocks;
        blocks[0].translated_text = Some("translated".into());
        let rewritten = super::rewrite_document(xml, "OPS/chapter.xhtml", &blocks).unwrap();
        let text = String::from_utf8(rewritten).unwrap();
        assert!(text.contains("<p>Foreign text</p>"));
        assert!(text.contains("<p>translated</p>"));
    }

    #[test]
    fn rejects_external_xhtml_resource_references_during_analysis() {
        for resource in [
            r#"<img src="https://example.test/image.png"/>"#,
            r#"<link rel="stylesheet" href="//example.test/style.css"/>"#,
            r#"<object data="data:text/plain,remote"/>"#,
        ] {
            let xml = format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>{resource}</body></html>"#
            );
            assert!(super::parse_document(xml.as_bytes(), "OPS/chapter.xhtml").is_err());
        }
    }

    #[test]
    fn rejects_external_xhtml_resource_references_during_rewrite() {
        let xml = br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Text</p><img src="https://example.test/image.png"/></body></html>"#;
        let blocks = [super::super::super::DocumentBlock {
            id: "OPS/chapter.xhtml#0".into(),
            ordinal: 0,
            block_type: BlockType::Paragraph,
            source_text: "Text".into(),
            translated_text: Some("Translated".into()),
        }];
        let error = super::rewrite_document(xml, "OPS/chapter.xhtml", &blocks).unwrap_err();
        assert!(error.to_string().contains("external EPUB XHTML resource"));
    }

    #[test]
    fn rejects_oversized_and_deep_xhtml_during_analysis_and_rewrite() {
        let oversized = vec![b'x'; super::MAX_DOCUMENT_BYTES + 1];
        assert!(super::parse_document(&oversized, "OPS/chapter.xhtml").is_err());
        assert!(super::rewrite_document(&oversized, "OPS/chapter.xhtml", &[]).is_err());

        let mut deep = String::from("<html xmlns=\"http://www.w3.org/1999/xhtml\">");
        for _ in 0..=super::MAX_DEPTH {
            deep.push_str("<div>");
        }
        deep.push_str("text");
        for _ in 0..=super::MAX_DEPTH {
            deep.push_str("</div>");
        }
        deep.push_str("</html>");
        assert!(super::parse_document(deep.as_bytes(), "OPS/chapter.xhtml").is_err());
        assert!(super::rewrite_document(deep.as_bytes(), "OPS/chapter.xhtml", &[]).is_err());
    }
}
use std::collections::HashMap;

use quick_xml::events::{BytesText, Event};
use quick_xml::name::{Namespace, ResolveResult};
use quick_xml::{NsReader, Reader, Writer};

use super::super::{BlockType, DocumentBlock};
use super::block;
use crate::domain::RuntimeError;
use quick_xml::XmlVersion;

const XHTML_NS: &[u8] = b"http://www.w3.org/1999/xhtml";
const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
const MAX_DEPTH: usize = 128;

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
    validate_document_size(bytes)?;
    let mut reader = NsReader::from_reader(bytes);
    let mut buffer = Vec::new();
    let mut depth = 0_usize;
    let mut current: Option<Candidate> = None;
    let mut skipped: Option<Skipped> = None;
    let mut ordinal = 0_usize;
    let mut blocks = Vec::new();
    let mut diagnostics = Vec::new();
    loop {
        let (namespace, event) = reader
            .read_resolved_event_into(&mut buffer)
            .map_err(xml_error)?;
        match event {
            Event::Start(element) => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(RuntimeError::InvalidInput(
                        "EPUB XHTML nesting is too deep".into(),
                    ));
                }
                let name = local_name(element.name().as_ref()).to_string();
                validate_resource_references(&element, &name)?;
                if let Some(skip) = &mut skipped {
                    let _ = skip;
                } else if !is_xhtml(&namespace) || unsupported(&name) {
                    skipped = Some(Skipped {
                        depth,
                        name,
                        has_text: false,
                    });
                    if let Some(candidate) = &mut current {
                        candidate.unsupported = true;
                    }
                } else if current.is_none() {
                    if let Some(kind) = semantic_type(&name) {
                        current = Some(Candidate {
                            depth,
                            kind,
                            text: String::new(),
                            text_nodes: 0,
                            unsupported: false,
                        });
                    }
                }
            }
            Event::Empty(element) => {
                let qualified_name = element.name();
                let name = local_name(qualified_name.as_ref());
                validate_resource_references(&element, name)?;
                if (!is_xhtml(&namespace) && has_text_attribute(&element)) || unsupported(name) {
                    diagnostics.push(format!("unsupported EPUB XHTML element: {name}"));
                    if let Some(candidate) = &mut current {
                        candidate.unsupported = true;
                    }
                }
            }
            Event::Text(text) => {
                if let Some(skip) = &mut skipped {
                    skip.has_text = true;
                } else if let Some(candidate) = &mut current {
                    candidate.text_nodes += 1;
                    candidate
                        .text
                        .push_str(&text.xml_content(XmlVersion::Implicit1_0));
                }
            }
            Event::End(_element) => {
                if skipped.as_ref().is_some_and(|skip| skip.depth == depth) {
                    let skip = skipped.take().unwrap();
                    if skip.has_text {
                        diagnostics.push(format!("unsupported EPUB XHTML element: {}", skip.name));
                    }
                } else if current
                    .as_ref()
                    .is_some_and(|candidate| candidate.depth == depth)
                {
                    let candidate = current.take().unwrap();
                    if !candidate.unsupported && !candidate.text.trim().is_empty() {
                        blocks.push(block(
                            format!("{path}#{ordinal}"),
                            ordinal,
                            candidate.kind,
                            candidate.text.trim().to_string(),
                        ));
                        if candidate.text_nodes > 1 {
                            diagnostics.push(
                                "translation may cross EPUB XHTML formatting boundaries in a multi-node block"
                                    .into(),
                            );
                        }
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

#[derive(Debug)]
struct Candidate {
    depth: usize,
    kind: BlockType,
    text: String,
    text_nodes: usize,
    unsupported: bool,
}

#[derive(Debug)]
struct Skipped {
    depth: usize,
    name: String,
    has_text: bool,
}

pub(super) fn rewrite_document(
    source: &[u8],
    path: &str,
    blocks: &[DocumentBlock],
) -> Result<Vec<u8>, RuntimeError> {
    validate_document_size(source)?;
    let mut reader = NsReader::from_reader(source);
    let mut writer = Writer::new(Vec::new());
    let mut buffer = Vec::new();
    let mut depth = 0_usize;
    let mut element = Vec::new();
    let mut element_depth = 0_usize;
    let mut element_has_foreign = false;
    let mut block_index = 0_usize;

    loop {
        let (namespace, event) = reader
            .read_resolved_event_into(&mut buffer)
            .map_err(xml_error)?;
        let is_eof = matches!(event, Event::Eof);
        let is_start = matches!(&event, Event::Start(_));
        let is_end = matches!(&event, Event::End(_));
        let is_foreign =
            matches!(&event, Event::Start(_) | Event::Empty(_) if !is_xhtml(&namespace));
        if is_start {
            if depth + 1 > MAX_DEPTH {
                return Err(RuntimeError::InvalidInput(
                    "EPUB XHTML nesting is too deep".into(),
                ));
            }
            depth += 1;
        }
        if let Event::Start(element) | Event::Empty(element) = &event {
            validate_resource_references(element, local_name(element.name().as_ref()))?;
        }
        if !element.is_empty() {
            let closes_element = is_end && depth == element_depth;
            element_has_foreign |= is_foreign;
            element.push(event.into_owned());
            if closes_element {
                rewrite_element(
                    &mut writer,
                    &element,
                    element_has_foreign,
                    path,
                    blocks,
                    &mut block_index,
                )?;
                element.clear();
                element_has_foreign = false;
            }
        } else if matches!(&event, Event::Start(value) if is_xhtml(&namespace) && semantic_type(local_name(value.name().as_ref())).is_some())
        {
            element_depth = depth;
            element.push(event.into_owned());
        } else {
            writer.write_event(event.into_owned()).map_err(xml_write)?;
        }
        if is_end {
            depth = depth.saturating_sub(1);
        }
        if is_eof {
            break;
        }
        buffer.clear();
    }
    if !element.is_empty() {
        return Err(RuntimeError::InvalidInput(
            "unterminated EPUB XHTML element".into(),
        ));
    }
    if block_index != blocks.len() {
        return Err(RuntimeError::InvalidInput(format!(
            "missing translated EPUB block {}",
            block_index
        )));
    }
    Ok(writer.into_inner())
}

fn validate_document_size(bytes: &[u8]) -> Result<(), RuntimeError> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(RuntimeError::InvalidInput(
            "EPUB XHTML exceeds size limit".into(),
        ));
    }
    Ok(())
}

fn validate_resource_references(
    element: &quick_xml::events::BytesStart<'_>,
    name: &str,
) -> Result<(), RuntimeError> {
    for attribute in element.attributes().flatten() {
        let key = local_name(attribute.key.as_ref());
        let is_resource = matches!(
            (name, key),
            ("link", "href")
                | ("img", "src")
                | ("img", "srcset")
                | ("object", "data")
                | ("iframe", "src")
                | ("script", "src")
                | ("audio", "src")
                | ("video", "src" | "poster")
                | ("source", "src")
                | ("source", "srcset")
                | ("track", "src")
                | ("embed", "src")
                | ("input", "src")
                | ("image", "href")
        );
        if !is_resource {
            continue;
        }
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| {
                RuntimeError::InvalidInput(format!("invalid EPUB resource URL: {error}"))
            })?;
        if value.split(',').any(|candidate| {
            is_external_reference(candidate.split_whitespace().next().unwrap_or_default())
        }) {
            return Err(RuntimeError::InvalidInput(format!(
                "external EPUB XHTML resource: {value}"
            )));
        }
    }
    Ok(())
}

fn is_external_reference(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() || value.starts_with('#') || value.starts_with('/') {
        return value.starts_with("//") || value.starts_with('/');
    }
    value.find(':').is_some_and(|colon| {
        value[..colon].bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic()
                || (index > 0 && (byte == b'+' || byte == b'-' || byte == b'.'))
        })
    })
}

fn rewrite_element(
    writer: &mut Writer<Vec<u8>>,
    events: &[Event<'static>],
    has_foreign_namespace: bool,
    path: &str,
    blocks: &[DocumentBlock],
    block_index: &mut usize,
) -> Result<(), RuntimeError> {
    if has_foreign_namespace {
        for event in events {
            writer.write_event(event.clone()).map_err(xml_write)?;
        }
        return Ok(());
    }
    let Some(info) = element_info(events) else {
        for event in events {
            writer.write_event(event.clone()).map_err(xml_write)?;
        }
        return Ok(());
    };
    let Some(block) = blocks.get(*block_index) else {
        return Err(RuntimeError::InvalidInput(format!(
            "missing translated EPUB block {}",
            block_index
        )));
    };
    if !block.id.starts_with(&format!("{path}#")) {
        return Err(RuntimeError::InvalidInput(format!(
            "unexpected translated EPUB block id: {}",
            block.id
        )));
    }
    let replacement = block
        .translated_text
        .as_deref()
        .ok_or_else(|| RuntimeError::InvalidInput("EPUB job has untranslated blocks".into()))?;
    let replacement = preserve_outer_whitespace(&info.text, replacement);
    let lengths = info.lengths;
    let total = lengths.iter().sum::<usize>().max(1);
    let mut offset = 0_usize;
    let mut source_offset = 0_usize;
    let mut text_index = 0_usize;
    let preserve = replacement.trim() != replacement;
    for event in events {
        match event {
            Event::Start(start) if text_index == 0 => {
                let mut start = start.clone();
                if preserve && !has_xml_space(&start) {
                    start.push_attribute(("xml:space", "preserve"));
                }
                writer.write_event(Event::Start(start)).map_err(xml_write)?;
            }
            Event::Text(_) => {
                let end = if lengths.len() == 1 || text_index + 1 == lengths.len() {
                    replacement.chars().count()
                } else {
                    replacement.chars().count() * (source_offset + lengths[text_index]) / total
                };
                let value = replacement
                    .chars()
                    .skip(offset)
                    .take(end.saturating_sub(offset))
                    .collect::<String>();
                writer
                    .write_event(Event::Text(BytesText::new(&value)))
                    .map_err(xml_write)?;
                offset = end;
                source_offset += lengths[text_index];
                text_index += 1;
            }
            other => writer.write_event(other.clone()).map_err(xml_write)?,
        }
    }
    *block_index += 1;
    Ok(())
}

struct ElementInfo {
    text: String,
    lengths: Vec<usize>,
}

fn element_info(events: &[Event<'_>]) -> Option<ElementInfo> {
    let mut text = String::new();
    let mut lengths = Vec::new();
    let mut unsupported_element = false;
    for event in events {
        match event {
            Event::Start(element) | Event::Empty(element) => {
                let qualified_name = element.name();
                let name = local_name(qualified_name.as_ref());
                if unsupported(name) || has_foreign_namespace(element) {
                    unsupported_element = true;
                }
            }
            Event::Text(value) => {
                let value = value.xml_content(XmlVersion::Implicit1_0).into_owned();
                lengths.push(value.chars().count());
                text.push_str(&value);
            }
            _ => {}
        }
    }
    (!unsupported_element && !text.trim().is_empty()).then_some(ElementInfo { text, lengths })
}

fn preserve_outer_whitespace(source: &str, replacement: &str) -> String {
    let leading = source.len() - source.trim_start().len();
    let trailing = source.len() - source.trim_end().len();
    if leading == 0 && trailing == 0 {
        return replacement.to_string();
    }
    format!(
        "{}{}{}",
        &source[..leading],
        replacement.trim(),
        &source[source.len() - trailing..]
    )
}

fn is_xhtml(namespace: &ResolveResult<'_>) -> bool {
    match namespace {
        ResolveResult::Bound(Namespace(value)) => value.as_bytes() == XHTML_NS,
        ResolveResult::Unbound => true,
        ResolveResult::Unknown(_) => false,
    }
}

fn has_text_attribute(element: &quick_xml::events::BytesStart<'_>) -> bool {
    element
        .attributes()
        .flatten()
        .any(|attribute| attribute.key.as_ref() == "alt" || attribute.key.as_ref() == "title")
}

fn has_foreign_namespace(element: &quick_xml::events::BytesStart<'_>) -> bool {
    element.attributes().flatten().any(|attribute| {
        attribute.key.as_ref() == "xmlns"
            && attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .map(|value| value.as_bytes() != XHTML_NS)
                .unwrap_or(true)
    })
}

fn has_xml_space(element: &quick_xml::events::BytesStart<'_>) -> bool {
    element
        .attributes()
        .flatten()
        .any(|attribute| attribute.key.as_ref() == "xml:space" || attribute.key.as_ref() == "space")
}

fn xml_write(error: std::io::Error) -> RuntimeError {
    RuntimeError::Connection(format!("write EPUB XML: {error}"))
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
        "blockquote" | "script" | "style" | "foreign" | "svg" | "math" | "object" | "iframe"
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
