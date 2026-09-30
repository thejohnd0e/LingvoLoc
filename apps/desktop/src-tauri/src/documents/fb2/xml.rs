mod blocks;
mod rewrite;

use quick_xml::events::Event;
use quick_xml::name::{Namespace, ResolveResult};
use quick_xml::{NsReader, Writer};

use super::super::{BlockType, DocumentBlock, RuntimeError};
use super::Analysis;
use blocks::{finish_candidate, Candidate, Unsupported};

const FB2_NS: &[u8] = b"http://www.gribuser.ru/xml/fictionbook/2.0";
const MAX_DOCUMENT_BYTES: usize = 64 * 1024 * 1024;
const MAX_DEPTH: usize = 128;
const UNSUPPORTED_BODY_ELEMENTS: &[&str] = &[
    "annotation",
    "cite",
    "poem",
    "stanza",
    "subtitle",
    "table",
    "text-author",
    "v",
];
const MAX_DIAGNOSTICS: usize = 100;

pub(super) fn analyze(source: &[u8]) -> Result<Analysis, RuntimeError> {
    let (analysis, _) = transform(source, None)?;
    Ok(analysis)
}

pub(super) fn export(source: &[u8], blocks: &[DocumentBlock]) -> Result<Vec<u8>, RuntimeError> {
    let (_, output) = transform(source, Some(blocks))?;
    Ok(output)
}

fn transform(
    source: &[u8],
    replacements: Option<&[DocumentBlock]>,
) -> Result<(Analysis, Vec<u8>), RuntimeError> {
    if source.len() > MAX_DOCUMENT_BYTES {
        return Err(RuntimeError::InvalidInput(
            "FB2 document exceeds size limit".into(),
        ));
    }

    let mut reader = NsReader::from_reader(source);
    let mut writer = Writer::new(Vec::new());
    let mut buffer = Vec::new();
    let mut blocks = Vec::new();
    let mut diagnostics = Vec::new();
    let mut depth = 0_usize;
    let mut body_depth = None;
    let mut body_elements: Vec<Option<String>> = Vec::new();
    let mut title_depth = None;
    let mut candidate: Option<Candidate> = None;
    let mut unsupported: Option<Unsupported> = None;
    let mut root_seen = false;

    loop {
        let (namespace, event) = reader
            .read_resolved_event_into(&mut buffer)
            .map_err(rewrite::xml_error)?;
        if matches!(event, Event::Eof) {
            break;
        }

        let is_start = matches!(&event, Event::Start(_));
        let is_end = matches!(&event, Event::End(_));
        let is_fb2 = is_fb2_namespace(&namespace);
        if is_start {
            depth += 1;
            if depth > MAX_DEPTH {
                return Err(RuntimeError::InvalidInput(
                    "FB2 XML nesting is too deep".into(),
                ));
            }
        }

        if let Event::Start(element) = &event {
            let name = local_name(element.name().as_ref());
            let parent_name = body_elements.last().and_then(Option::as_deref);
            if depth == 1 {
                if name != "FictionBook" || !is_fb2 {
                    return Err(RuntimeError::InvalidInput(
                        "source is not an FB2 FictionBook document".into(),
                    ));
                }
                root_seen = true;
            }

            if let Some(active) = &mut candidate {
                active.events.push(event.into_owned());
            } else if unsupported.is_some() {
                writer
                    .write_event(event.into_owned())
                    .map_err(rewrite::xml_write)?;
            } else if is_fb2 && name == "body" {
                body_depth = Some(depth);
                writer
                    .write_event(event.into_owned())
                    .map_err(rewrite::xml_write)?;
            } else if is_fb2 && body_depth.is_some() && name == "title" {
                title_depth = Some(depth);
                writer
                    .write_event(event.into_owned())
                    .map_err(rewrite::xml_write)?;
            } else if is_fb2
                && body_depth.is_some()
                && UNSUPPORTED_BODY_ELEMENTS.contains(&name.as_str())
            {
                unsupported = Some(Unsupported {
                    depth,
                    name: name.clone(),
                    has_text: false,
                });
                writer
                    .write_event(event.into_owned())
                    .map_err(rewrite::xml_write)?;
            } else if is_fb2
                && body_depth.is_some()
                && name == "p"
                && matches!(parent_name, Some("section" | "title" | "epigraph"))
            {
                candidate = Some(Candidate {
                    depth,
                    block_type: if title_depth.is_some() {
                        BlockType::Heading
                    } else {
                        BlockType::Paragraph
                    },
                    events: vec![event.into_owned()],
                });
            } else if is_fb2 && body_depth.is_some() && name == "p" {
                unsupported = Some(Unsupported {
                    depth,
                    name: parent_name.unwrap_or("p").to_string(),
                    has_text: false,
                });
                writer
                    .write_event(event.into_owned())
                    .map_err(rewrite::xml_write)?;
            } else {
                writer
                    .write_event(event.into_owned())
                    .map_err(rewrite::xml_write)?;
            }
            if body_depth.is_some() {
                body_elements.push(is_fb2.then_some(name));
            }
        } else if let Event::End(element) = &event {
            let name = local_name(element.name().as_ref());
            if let Some(active) = &mut candidate {
                active.events.push(event.into_owned());
                if active.depth == depth {
                    let completed = candidate.take().ok_or_else(|| {
                        RuntimeError::InvalidInput("invalid FB2 paragraph state".into())
                    })?;
                    finish_candidate(
                        completed,
                        replacements,
                        &mut blocks,
                        &mut diagnostics,
                        &mut writer,
                    )?;
                }
            } else {
                writer
                    .write_event(event.into_owned())
                    .map_err(rewrite::xml_write)?;
            }
            if unsupported
                .as_ref()
                .is_some_and(|value| value.depth == depth)
            {
                let item = unsupported.take().ok_or_else(|| {
                    RuntimeError::InvalidInput("invalid FB2 unsupported-element state".into())
                })?;
                if item.has_text {
                    push_diagnostic(
                        &mut diagnostics,
                        format!("unsupported FB2 text element: {}", item.name),
                    );
                }
            }
            if title_depth == Some(depth) && name == "title" {
                title_depth = None;
            }
            if body_depth == Some(depth) && name == "body" {
                body_depth = None;
            }
            if body_depth.is_some() || name == "body" {
                body_elements.pop();
            }
        } else if let Some(active) = &mut candidate {
            active.events.push(event.into_owned());
        } else {
            if let Some(item) = &mut unsupported {
                item.has_text |= is_non_whitespace_text(&event)?;
            } else if body_depth.is_some() && is_non_whitespace_text(&event)? {
                let name = body_elements
                    .last()
                    .and_then(Option::as_deref)
                    .unwrap_or("body");
                let diagnostic = if matches!(event, Event::GeneralRef(_)) {
                    format!("unresolved FB2 entity in <{name}>; content left unchanged")
                } else {
                    format!("unsupported FB2 text in <{name}>; content left unchanged")
                };
                push_diagnostic(&mut diagnostics, diagnostic);
            }
            writer
                .write_event(event.into_owned())
                .map_err(rewrite::xml_write)?;
        }

        if is_end {
            depth = depth.saturating_sub(1);
        }
        buffer.clear();
    }

    if !root_seen || candidate.is_some() || unsupported.is_some() {
        return Err(RuntimeError::InvalidInput("FB2 XML is incomplete".into()));
    }
    if depth != 0 {
        return Err(RuntimeError::InvalidInput("FB2 XML is incomplete".into()));
    }
    if replacements.is_some_and(|items| items.len() != blocks.len()) {
        return Err(RuntimeError::InvalidInput(
            "FB2 job has missing or extra translated blocks".into(),
        ));
    }

    Ok((
        Analysis {
            blocks,
            diagnostics,
        },
        writer.into_inner(),
    ))
}

fn is_non_whitespace_text(event: &Event<'_>) -> Result<bool, RuntimeError> {
    if matches!(event, Event::GeneralRef(_)) {
        return Ok(true);
    }
    Ok(rewrite::event_text(event)?.is_some_and(|text| !text.trim().is_empty()))
}

pub(super) fn push_diagnostic(diagnostics: &mut Vec<String>, diagnostic: String) {
    if diagnostics.len() < MAX_DIAGNOSTICS - 1 {
        diagnostics.push(diagnostic);
    } else if diagnostics.len() == MAX_DIAGNOSTICS - 1 {
        diagnostics.push("additional FB2 diagnostics omitted".into());
    }
}

fn is_fb2_namespace(namespace: &ResolveResult<'_>) -> bool {
    matches!(namespace, ResolveResult::Bound(Namespace(value)) if value.as_bytes() == FB2_NS)
}

fn local_name(name: &str) -> String {
    name.rsplit(':').next().unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::{push_diagnostic, MAX_DIAGNOSTICS};

    #[test]
    fn diagnostic_count_includes_the_omitted_summary() {
        let mut diagnostics = Vec::new();
        for index in 0..MAX_DIAGNOSTICS * 2 {
            push_diagnostic(&mut diagnostics, format!("diagnostic {index}"));
        }

        assert_eq!(diagnostics.len(), MAX_DIAGNOSTICS);
        assert_eq!(
            diagnostics.last().map(String::as_str),
            Some("additional FB2 diagnostics omitted")
        );
    }
}
