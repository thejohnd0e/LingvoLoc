use quick_xml::events::BytesText;
use quick_xml::events::Event;
use quick_xml::name::{Namespace, ResolveResult};
use quick_xml::NsReader;
use quick_xml::Writer;
use quick_xml::XmlVersion;

use super::super::BlockType;
use super::{block, Analysis, DocumentBlock};
use crate::domain::RuntimeError;

const WORD_NS: &[u8] = b"http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const MAX_DEPTH: usize = 128;

pub fn parse_document(bytes: &[u8]) -> Result<Analysis, RuntimeError> {
    let mut reader = NsReader::from_reader(bytes);
    let mut buffer = Vec::new();
    let mut depth = 0_usize;
    let mut paragraphs = Vec::new();
    let mut diagnostics = Vec::new();
    let mut paragraph_text = String::new();
    let mut paragraph_kind = BlockType::Paragraph;
    let mut in_paragraph = false;
    let mut paragraph_unsupported = false;
    let mut paragraph_text_nodes = 0_usize;
    let mut in_table_cell = false;
    let mut ordinal = 0_usize;

    loop {
        let (namespace, event) = reader
            .read_resolved_event_into(&mut buffer)
            .map_err(|error| {
                RuntimeError::InvalidInput(format!("invalid WordprocessingML: {error}"))
            })?;
        match event {
            Event::Start(element) => {
                let qualified_name = element.name();
                let name = local_name(qualified_name.as_ref());
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(RuntimeError::InvalidInput(
                        "DOCX XML nesting is too deep".into(),
                    ));
                }
                if is_word(&namespace) {
                    match name {
                        "p" => {
                            in_paragraph = true;
                            paragraph_unsupported = false;
                            paragraph_text_nodes = 0;
                            paragraph_text.clear();
                            paragraph_kind = if in_table_cell {
                                BlockType::TableCell
                            } else {
                                BlockType::Paragraph
                            };
                        }
                        "t" if in_paragraph => {}
                        "tc" => in_table_cell = true,
                        "numPr" if in_paragraph => paragraph_kind = BlockType::ListItem,
                        "pStyle" => {
                            if is_heading_style(&element) {
                                paragraph_kind = BlockType::Heading;
                            }
                        }
                        "tbl" | "tr" | "hyperlink" | "r" | "pPr" | "rPr" | "ilvl" | "numId" => {}
                        other => {
                            if in_paragraph {
                                paragraph_unsupported = true;
                            }
                            if other != "document" && other != "body" {
                                diagnostics
                                    .push(format!("unsupported WordprocessingML element: {other}"));
                            }
                        }
                    }
                }
            }
            Event::Empty(element) => {
                if is_word(&namespace) {
                    match local_name(element.name().as_ref()) {
                        "numPr" => paragraph_kind = BlockType::ListItem,
                        "pStyle" => paragraph_kind = BlockType::Heading,
                        "document" | "body" | "tbl" | "tr" | "tc" | "hyperlink" | "r" | "pPr"
                        | "rPr" | "ilvl" | "numId" | "drawing" => {}
                        other => {
                            if in_paragraph {
                                paragraph_unsupported = true;
                            }
                            diagnostics
                                .push(format!("unsupported WordprocessingML element: {other}"));
                        }
                    }
                }
            }
            Event::End(element) => {
                let qualified_name = element.name();
                let name = local_name(qualified_name.as_ref());
                if is_word(&namespace) && name == "p" && in_paragraph {
                    if !paragraph_unsupported && !paragraph_text.trim().is_empty() {
                        if paragraph_text_nodes > 1 {
                            diagnostics.push(
                                "translation may cross DOCX formatting boundaries in a multi-run paragraph"
                                    .into(),
                            );
                        }
                        paragraphs.push(block(
                            ordinal,
                            paragraph_kind,
                            paragraph_text.trim().to_string(),
                        ));
                        ordinal += 1;
                    }
                    in_paragraph = false;
                }
                if is_word(&namespace) && name == "tc" {
                    in_table_cell = false;
                }
                depth = depth.saturating_sub(1);
            }
            Event::Text(text) if in_paragraph => {
                paragraph_text_nodes += 1;
                paragraph_text.push_str(&text.xml_content(XmlVersion::Implicit1_0));
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }

    Ok(Analysis {
        blocks: paragraphs,
        diagnostics,
    })
}

pub fn rewrite_document(source: &[u8], blocks: &[DocumentBlock]) -> Result<Vec<u8>, RuntimeError> {
    if blocks.iter().any(|block| block.translated_text.is_none()) {
        return Err(RuntimeError::InvalidInput(
            "DOCX job has untranslated blocks".into(),
        ));
    }
    let mut reader = NsReader::from_reader(source);
    let mut writer = Writer::new(Vec::new());
    let mut buffer = Vec::new();
    let mut paragraph = Vec::new();
    let mut paragraph_depth = 0_usize;
    let mut block_index = 0_usize;
    loop {
        let event = reader.read_event_into(&mut buffer).map_err(|error| {
            RuntimeError::InvalidInput(format!("invalid WordprocessingML: {error}"))
        })?;
        let is_eof = matches!(&event, Event::Eof);
        if !paragraph.is_empty() {
            let is_end =
                matches!(&event, Event::End(element) if local_name(element.name().as_ref()) == "p");
            paragraph.push(event.into_owned());
            if is_end {
                paragraph_depth = paragraph_depth.saturating_sub(1);
                rewrite_paragraph(&mut writer, &paragraph, blocks, &mut block_index)?;
                paragraph.clear();
            }
        } else if matches!(&event, Event::Start(element) if local_name(element.name().as_ref()) == "p")
        {
            paragraph_depth = 1;
            paragraph.push(event.into_owned());
        } else {
            writer.write_event(event.into_owned()).map_err(xml_write)?;
        }
        if paragraph_depth == 0 && !paragraph.is_empty() {
            rewrite_paragraph(&mut writer, &paragraph, blocks, &mut block_index)?;
            paragraph.clear();
        }
        if is_eof {
            break;
        }
        buffer.clear();
    }
    if !paragraph.is_empty() {
        return Err(RuntimeError::InvalidInput(
            "unterminated DOCX paragraph".into(),
        ));
    }
    if block_index != blocks.len() {
        return Err(RuntimeError::InvalidInput(format!(
            "missing translated DOCX block {}",
            block_index
        )));
    }
    Ok(writer.into_inner())
}

fn rewrite_paragraph(
    writer: &mut Writer<Vec<u8>>,
    events: &[Event<'static>],
    blocks: &[DocumentBlock],
    block_index: &mut usize,
) -> Result<(), RuntimeError> {
    let info = paragraph_info(events);
    if !info.supported || info.text.trim().is_empty() {
        for event in events {
            writer.write_event(event.clone()).map_err(xml_write)?;
        }
        return Ok(());
    }
    let block = blocks.get(*block_index).ok_or_else(|| {
        RuntimeError::InvalidInput(format!("missing translated DOCX block {block_index}"))
    })?;
    let replacement = block.translated_text.as_deref().unwrap_or_default();
    let source_lengths = events
        .iter()
        .filter_map(|event| match event {
            Event::Text(value) => Some(value.xml_content(XmlVersion::Implicit1_0).chars().count()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let total_source_length = source_lengths.iter().sum::<usize>().max(1);
    let replacement_length = replacement.chars().count();
    let preserve_whitespace = replacement.trim() != replacement;
    let mut text_index = 0_usize;
    let mut replacement_offset = 0_usize;
    for event in events {
        match event {
            Event::Start(element)
                if preserve_whitespace && local_name(element.name().as_ref()) == "t" =>
            {
                let mut element = element.clone();
                element.push_attribute(("xml:space", "preserve"));
                writer
                    .write_event(Event::Start(element))
                    .map_err(xml_write)?;
            }
            Event::Text(_) => {
                let value = if source_lengths.len() == 1 {
                    None
                } else {
                    let end = if text_index + 1 == source_lengths.len() {
                        replacement_length
                    } else {
                        replacement_length.saturating_mul(source_lengths[text_index])
                            / total_source_length
                    };
                    let value = replacement
                        .chars()
                        .skip(replacement_offset)
                        .take(end.saturating_sub(replacement_offset))
                        .collect::<String>();
                    replacement_offset = end;
                    text_index += 1;
                    Some(value)
                };
                if let Some(value) = value {
                    writer
                        .write_event(Event::Text(BytesText::new(&value)))
                        .map_err(xml_write)?;
                } else {
                    writer
                        .write_event(Event::Text(BytesText::new(replacement)))
                        .map_err(xml_write)?;
                }
            }
            other => writer.write_event(other.clone()).map_err(xml_write)?,
        }
    }
    *block_index += 1;
    Ok(())
}

struct ParagraphInfo {
    text: String,
    supported: bool,
}

fn paragraph_info(events: &[Event<'_>]) -> ParagraphInfo {
    let mut text = String::new();
    let mut supported = true;
    for event in events {
        match event {
            Event::Start(element) | Event::Empty(element) => {
                let qualified_name = element.name();
                let name = local_name(qualified_name.as_ref());
                if !matches!(
                    name,
                    "p" | "pPr"
                        | "pStyle"
                        | "numPr"
                        | "ilvl"
                        | "numId"
                        | "r"
                        | "rPr"
                        | "t"
                        | "tab"
                        | "br"
                        | "hyperlink"
                ) {
                    supported = false;
                }
            }
            Event::Text(value) => {
                text.push_str(value.xml_content(XmlVersion::Implicit1_0).as_ref());
            }
            _ => {}
        }
    }
    ParagraphInfo { text, supported }
}

fn is_heading_style(element: &quick_xml::events::BytesStart<'_>) -> bool {
    element.attributes().flatten().any(|attribute| {
        local_name(attribute.key.as_ref()) == "val"
            && attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .map(|value| value.starts_with("Heading") || value == "Title")
                .unwrap_or(false)
    })
}

fn xml_write(error: std::io::Error) -> RuntimeError {
    RuntimeError::Connection(format!("write DOCX XML: {error}"))
}

fn is_word(namespace: &ResolveResult<'_>) -> bool {
    match namespace {
        ResolveResult::Bound(Namespace(value)) => value.as_bytes() == WORD_NS,
        ResolveResult::Unbound | ResolveResult::Unknown(_) => true,
    }
}

fn local_name(name: &str) -> &str {
    match name.rsplit_once(':') {
        Some((_, local)) => local,
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_headings_lists_and_unicode_with_stable_ids() {
        let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
            <w:p><w:pPr><w:pStyle w:val="Title"/></w:pPr><w:r><w:t>Заголовок</w:t></w:r></w:p>
            <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Пункт</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let analysis = parse_document(xml.as_bytes()).unwrap();
        assert_eq!(analysis.blocks.len(), 2);
        assert_eq!(analysis.blocks[0].id, "word/document.xml#p-00000000");
        assert_eq!(analysis.blocks[0].block_type, BlockType::Heading);
        assert_eq!(analysis.blocks[0].source_text, "Заголовок");
        assert_eq!(analysis.blocks[1].block_type, BlockType::ListItem);
        assert_eq!(analysis.blocks[1].source_text, "Пункт");
    }

    #[test]
    fn reports_unsupported_word_elements() {
        let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:customThing/></w:body></w:document>"#;
        let analysis = parse_document(xml.as_bytes()).unwrap();
        assert!(!analysis.diagnostics.is_empty());
    }

    #[test]
    fn rewrites_supported_text_after_an_empty_paragraph() {
        let xml = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p></w:p><w:p><w:r><w:t>Original</w:t></w:r></w:p></w:body></w:document>"#;
        let mut analysis = parse_document(xml).unwrap();
        assert_eq!(analysis.blocks.len(), 1);
        analysis.blocks[0].translated_text = Some("Translated".into());

        let rewritten = rewrite_document(xml, &analysis.blocks).unwrap();
        let result = parse_document(&rewritten).unwrap();
        assert_eq!(result.blocks[0].source_text, "Translated");
    }

    #[test]
    fn leaves_unsupported_paragraphs_unchanged() {
        let xml = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:fldSimple w:instr="PAGE"><w:r><w:t>1</w:t></w:r></w:fldSimple></w:r></w:p><w:p><w:r><w:t>Original</w:t></w:r></w:p></w:body></w:document>"#;
        let mut analysis = parse_document(xml).unwrap();
        assert_eq!(analysis.blocks.len(), 1);
        analysis.blocks[0].translated_text = Some("Translated".into());

        let rewritten = rewrite_document(xml, &analysis.blocks).unwrap();
        let text = String::from_utf8(rewritten).unwrap();
        assert!(text.contains("PAGE"));
        assert!(text.contains(">1</w:t>"));
        assert!(text.contains(">Translated</w:t>"));
    }

    #[test]
    fn preserves_all_visible_text_nodes_when_rewriting_multiple_runs() {
        let xml = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Old </w:t></w:r><w:hyperlink><w:r><w:t>link</w:t></w:r></w:hyperlink></w:p></w:body></w:document>"#;
        let mut analysis = parse_document(xml).unwrap();
        analysis.blocks[0].translated_text = Some("New visible link".into());

        let rewritten = rewrite_document(xml, &analysis.blocks).unwrap();
        let reparsed = parse_document(&rewritten).unwrap();
        assert_eq!(reparsed.blocks[0].source_text, "New visible link");
        let text = String::from_utf8(rewritten).unwrap();
        assert!(text.contains("<w:hyperlink>"));
    }

    #[test]
    fn preserves_xml_whitespace_for_translated_text() {
        let xml = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Original</w:t></w:r></w:p></w:body></w:document>"#;
        let mut analysis = parse_document(xml).unwrap();
        analysis.blocks[0].translated_text = Some(" translated ".into());

        let rewritten = rewrite_document(xml, &analysis.blocks).unwrap();
        let text = String::from_utf8(rewritten).unwrap();
        assert!(text.contains("xml:space=\"preserve\""));
        assert!(text.contains("> translated </w:t>"));
    }
}
