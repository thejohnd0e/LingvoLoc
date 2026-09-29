use super::{BlockType, DocumentBlock};
use crate::domain::RuntimeError;

pub const PARSER_VERSION: &str = "txt-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxtDocument {
    pub paragraphs: Vec<DocumentBlock>,
}

pub fn decode(bytes: &[u8]) -> Result<String, RuntimeError> {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return String::from_utf8(bytes[3..].to_vec())
            .map_err(|_| RuntimeError::InvalidInput("TXT is not valid UTF-8".into()));
    }
    if bytes.starts_with(&[0xff, 0xfe]) {
        return decode_utf16(&bytes[2..], true);
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        return decode_utf16(&bytes[2..], false);
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| {
        RuntimeError::InvalidInput(
            "TXT encoding is ambiguous or unsupported; use UTF-8 or UTF-16 with BOM".into(),
        )
    })
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String, RuntimeError> {
    let (pairs, remainder) = bytes.as_chunks::<2>();
    if !remainder.is_empty() {
        return Err(RuntimeError::InvalidInput(
            "TXT UTF-16 data has an incomplete code unit".into(),
        ));
    }
    let units = pairs
        .iter()
        .map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        })
        .collect::<Vec<_>>();
    String::from_utf16(&units)
        .map_err(|_| RuntimeError::InvalidInput("TXT is not valid UTF-16".into()))
}

pub fn parse(text: &str) -> TxtDocument {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let paragraphs = normalized
        .split("\n\n")
        .enumerate()
        .filter_map(|(ordinal, value)| {
            let source_text = value.trim().to_string();
            (!source_text.is_empty()).then(|| DocumentBlock {
                id: format!("paragraph-{ordinal:08}"),
                ordinal: ordinal as i64,
                block_type: BlockType::Paragraph,
                source_text,
                translated_text: None,
            })
        })
        .collect();
    TxtDocument { paragraphs }
}

pub fn export(blocks: &[DocumentBlock]) -> Result<String, RuntimeError> {
    if blocks.iter().any(|block| block.translated_text.is_none()) {
        return Err(RuntimeError::InvalidInput(
            "TXT job has untranslated blocks".into(),
        ));
    }
    Ok(blocks
        .iter()
        .map(|block| block.translated_text.as_deref().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_utf8_with_and_without_bom() {
        assert_eq!(decode("one\r\ntwo".as_bytes()).unwrap(), "one\r\ntwo");
        assert_eq!(decode(&[0xef, 0xbb, 0xbf, b'o', b'k']).unwrap(), "ok");
    }

    #[test]
    fn decodes_both_utf16_byte_orders() {
        assert_eq!(decode(&[0xff, 0xfe, b'o', 0, b'k', 0]).unwrap(), "ok");
        assert_eq!(decode(&[0xfe, 0xff, 0, b'o', 0, b'k']).unwrap(), "ok");
    }

    #[test]
    fn rejects_ambiguous_bytes_and_invalid_utf16() {
        assert!(decode(&[0xff, 0xfe, 0]).is_err());
        assert!(decode(&[0x80]).is_err());
    }

    #[test]
    fn preserves_paragraph_order_and_normalizes_line_endings() {
        let document = parse(" first\r\nline\r\n\r\nsecond \n\n\nthird ");
        assert_eq!(document.paragraphs.len(), 3);
        assert_eq!(document.paragraphs[0].source_text, "first\nline");
        assert_eq!(document.paragraphs[2].ordinal, 2);
    }

    #[test]
    fn refuses_partial_export() {
        let document = parse("one\n\ntwo");
        assert!(export(&document.paragraphs).is_err());
    }
}
