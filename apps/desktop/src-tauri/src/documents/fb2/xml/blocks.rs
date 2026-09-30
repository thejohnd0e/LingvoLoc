use quick_xml::events::Event;
use quick_xml::Writer;

use super::super::super::{BlockType, DocumentBlock, RuntimeError};
use super::{push_diagnostic, rewrite};

pub(super) struct Candidate {
    pub(super) depth: usize,
    pub(super) block_type: BlockType,
    pub(super) events: Vec<Event<'static>>,
}

pub(super) struct Unsupported {
    pub(super) depth: usize,
    pub(super) name: String,
    pub(super) has_text: bool,
}

pub(super) fn finish_candidate(
    candidate: Candidate,
    replacements: Option<&[DocumentBlock]>,
    blocks: &mut Vec<DocumentBlock>,
    diagnostics: &mut Vec<String>,
    writer: &mut Writer<Vec<u8>>,
) -> Result<(), RuntimeError> {
    let Some(text) = rewrite::candidate_text(&candidate.events)? else {
        push_diagnostic(
            diagnostics,
            "unsupported FB2 entity in paragraph; paragraph left unchanged".into(),
        );
        return write_events(writer, &candidate.events);
    };
    if text.is_empty() {
        return write_events(writer, &candidate.events);
    }
    if rewrite::text_node_count(&candidate.events)? > 1 {
        push_diagnostic(
            diagnostics,
            "FB2 translation may cross inline formatting boundaries in paragraph".into(),
        );
    }

    let ordinal = blocks.len();
    let mut block = DocumentBlock {
        id: format!("fb2#{ordinal}"),
        ordinal: i64::try_from(ordinal).map_err(|_| {
            RuntimeError::InvalidInput("FB2 document contains too many blocks".into())
        })?,
        block_type: candidate.block_type,
        source_text: text,
        translated_text: None,
    };

    if let Some(replacements) = replacements {
        let replacement = replacements.get(ordinal).ok_or_else(|| {
            RuntimeError::InvalidInput(format!("missing translated FB2 block {ordinal}"))
        })?;
        if replacement.id != block.id {
            return Err(RuntimeError::InvalidInput(format!(
                "unexpected translated FB2 block id: {}",
                replacement.id
            )));
        }
        let translated = replacement
            .translated_text
            .as_deref()
            .ok_or_else(|| RuntimeError::InvalidInput("FB2 job has untranslated blocks".into()))?;
        rewrite::rewrite_candidate(writer, &candidate.events, translated)?;
        block.translated_text = Some(translated.to_string());
    } else {
        write_events(writer, &candidate.events)?;
    }
    blocks.push(block);
    Ok(())
}

fn write_events(
    writer: &mut Writer<Vec<u8>>,
    events: &[Event<'static>],
) -> Result<(), RuntimeError> {
    for event in events {
        writer
            .write_event(event.clone())
            .map_err(rewrite::xml_write)?;
    }
    Ok(())
}
