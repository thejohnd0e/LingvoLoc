use quick_xml::events::{BytesText, Event};
use quick_xml::{escape, Writer, XmlVersion};

use super::super::super::RuntimeError;

pub(super) fn candidate_text(events: &[Event<'static>]) -> Result<Option<String>, RuntimeError> {
    let mut text = String::new();
    for event in events {
        if let Some(value) = event_text(event)? {
            text.push_str(&value);
        } else if matches!(event, Event::GeneralRef(_)) {
            return Ok(None);
        }
    }
    Ok(Some(text.trim().to_string()))
}

pub(super) fn text_node_count(events: &[Event<'static>]) -> Result<usize, RuntimeError> {
    events.iter().try_fold(0_usize, |count, event| {
        Ok(count + usize::from(event_text(event)?.is_some()))
    })
}

pub(super) fn event_text(event: &Event<'_>) -> Result<Option<String>, RuntimeError> {
    match event {
        Event::Text(text) => Ok(Some(text.xml_content(XmlVersion::Implicit1_0).into_owned())),
        Event::CData(text) => Ok(Some(text.as_ref().to_string())),
        Event::GeneralRef(reference) => {
            let name = reference.xml_content(XmlVersion::Implicit1_0);
            if let Some(character) = reference.resolve_char_ref().map_err(xml_error)? {
                return Ok(Some(character.to_string()));
            }
            Ok(escape::resolve_predefined_entity(&name).map(str::to_string))
        }
        _ => Ok(None),
    }
}

pub(super) fn rewrite_candidate(
    writer: &mut Writer<Vec<u8>>,
    events: &[Event<'static>],
    replacement: &str,
) -> Result<(), RuntimeError> {
    let source = candidate_text(events)?.ok_or_else(|| {
        RuntimeError::InvalidInput("unsupported FB2 entity in translated paragraph".into())
    })?;
    let mut lengths = Vec::new();
    for event in events {
        if let Some(value) = event_text(event)? {
            lengths.push(value.chars().count());
        }
    }
    let replacement = preserve_outer_whitespace(&source, replacement);
    let replacement_length = replacement.chars().count();
    let total_source_length = lengths.iter().sum::<usize>().max(1);
    let mut source_offset = 0_usize;
    let mut output_offset = 0_usize;
    let mut text_index = 0_usize;
    let mut replacement_chars = replacement.chars();

    for event in events {
        match event {
            Event::Start(start) if text_index == 0 => {
                let mut start = start.clone();
                let preserve = replacement.trim() != replacement;
                if preserve && !has_xml_space(&start) {
                    start.push_attribute(("xml:space", "preserve"));
                }
                writer.write_event(Event::Start(start)).map_err(xml_write)?;
            }
            _ if event_text(event)?.is_some() => {
                let source_length = lengths.get(text_index).copied().unwrap_or_default();
                let output_end = if text_index + 1 == lengths.len() {
                    replacement_length
                } else {
                    replacement_length.saturating_mul(source_offset.saturating_add(source_length))
                        / total_source_length
                };
                let take_count = output_end.saturating_sub(output_offset);
                let value = replacement_chars
                    .by_ref()
                    .take(take_count)
                    .collect::<String>();
                writer
                    .write_event(Event::Text(BytesText::new(&value)))
                    .map_err(xml_write)?;
                source_offset = source_offset.saturating_add(source_length);
                output_offset = output_end;
                text_index += 1;
            }
            other => writer.write_event(other.clone()).map_err(xml_write)?,
        }
    }
    Ok(())
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

fn has_xml_space(element: &quick_xml::events::BytesStart<'_>) -> bool {
    element
        .attributes()
        .flatten()
        .any(|attribute| attribute.key.as_ref() == "xml:space" || attribute.key.as_ref() == "space")
}

pub(super) fn xml_error(error: quick_xml::Error) -> RuntimeError {
    RuntimeError::InvalidInput(format!("invalid FB2 XML: {error}"))
}

pub(super) fn xml_write(error: std::io::Error) -> RuntimeError {
    RuntimeError::Connection(format!("write FB2 XML: {error}"))
}
