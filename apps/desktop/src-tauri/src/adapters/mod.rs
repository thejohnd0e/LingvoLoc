pub mod chat;
pub mod hunyuan_mt;
pub mod translategemma;

use crate::domain::{RuntimeError, TranslationModelAdapter, TranslationRequest, TranslationStyle};

/// Prompt/template family of a model, derived from its id or file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    TranslateGemma,
    HunyuanMt,
    /// Any other instruction-tuned chat model (Qwen, Llama, Mistral, ...).
    Chat,
}

impl Family {
    pub fn from_model_id(model_id: &str) -> Self {
        let id = model_id.to_lowercase();
        if id.contains("gemma") {
            Self::TranslateGemma
        } else if id.contains("hunyuan") {
            Self::HunyuanMt
        } else {
            Self::Chat
        }
    }

    pub fn adapter(self, model_id: &str) -> Box<dyn TranslationModelAdapter> {
        match self {
            Self::TranslateGemma => Box::new(translategemma::TranslateGemmaAdapter),
            Self::HunyuanMt => Box::new(hunyuan_mt::HunyuanMtAdapter),
            Self::Chat => Box::new(chat::ChatAdapter {
                no_think: model_id.to_lowercase().contains("qwen3"),
            }),
        }
    }
}

pub(crate) fn language_name(code: &str) -> &str {
    match code {
        "en" => "English",
        "ru" => "Russian",
        "de" => "German",
        "es" => "Spanish",
        "fr" => "French",
        "it" => "Italian",
        "pt" => "Portuguese",
        "pl" => "Polish",
        "uk" => "Ukrainian",
        "zh" => "Chinese",
        "ko" => "Korean",
        "th" => "Thai",
        other => other,
    }
}

pub(crate) fn style_instruction(style: TranslationStyle) -> Option<&'static str> {
    match style {
        TranslationStyle::Neutral => None,
        TranslationStyle::Literary => Some(
            "Use polished literary prose while preserving the author's tone, imagery, dialogue, paragraphing, and meaning. Do not add, omit, or embellish content.",
        ),
        TranslationStyle::Technical => Some(
            "Use precise, concise technical language and consistent terminology. Preserve identifiers, units, numbers, code, and formatting exactly. Do not omit technical detail.",
        ),
        TranslationStyle::Conversational => Some(
            "Use natural conversational language appropriate to the source register. Preserve meaning and formality; do not invent slang or add content.",
        ),
    }
}

pub(crate) fn chinese_style_instruction(style: TranslationStyle) -> Option<&'static str> {
    match style {
        TranslationStyle::Neutral => None,
        TranslationStyle::Literary => Some(
            "使用自然流畅的文学语言，保留原文的语气、意象、对话、段落和含义。不要添加、删减或改写原文没有的内容。",
        ),
        TranslationStyle::Technical => Some(
            "使用准确、简洁的技术语言和一致的术语。准确保留标识符、单位、数字、代码和格式，不要省略技术细节。",
        ),
        TranslationStyle::Conversational => Some(
            "使用自然的日常口语，保持原文的语气和正式程度，不要编造俚语或添加内容。",
        ),
    }
}

pub(crate) fn validate(request: &TranslationRequest) -> Result<(), RuntimeError> {
    if request.model_id.trim().is_empty()
        || request.source_language.trim().is_empty()
        || request.target_language.trim().is_empty()
    {
        return Err(RuntimeError::InvalidInput(
            "model and language codes are required".into(),
        ));
    }
    if request.text.trim().is_empty() {
        return Err(RuntimeError::InvalidInput("text must not be empty".into()));
    }
    Ok(())
}

pub(crate) fn strip_markdown_wrapper(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() >= 2
        && lines
            .first()
            .is_some_and(|line| line.trim().starts_with("```"))
        && lines.last().is_some_and(|line| line.trim() == "```")
    {
        return lines[1..lines.len() - 1].join("\n").trim().to_string();
    }
    text.to_string()
}

/// Removes a leading `<think>...</think>` block emitted by reasoning-capable models.
pub(crate) fn strip_think_block(text: &str) -> &str {
    let trimmed = text.trim_start();
    if trimmed.starts_with("<think>") {
        if let Some(end) = trimmed.find("</think>") {
            return trimmed[end + "</think>".len()..].trim();
        }
    } else if let Some(end) = trimmed.find("</think>") {
        // Some templates pre-fill the opening tag, so only the closing one is returned.
        return trimmed[end + "</think>".len()..].trim();
    }
    text.trim()
}

/// Shared response cleanup for every adapter.
pub(crate) fn clean_response(content: &str) -> Result<String, RuntimeError> {
    let text = strip_think_block(content).trim_matches('"').trim();
    if text.is_empty() {
        return Err(RuntimeError::MalformedResponse(
            "translation content was empty".into(),
        ));
    }
    if text.contains('\u{fffd}') {
        return Err(RuntimeError::MalformedResponse(
            "translation contains invalid Unicode replacement characters".into(),
        ));
    }
    Ok(strip_markdown_wrapper(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_family_from_model_name() {
        assert_eq!(
            Family::from_model_id("translategemma-12b-it.Q5_K_M.gguf"),
            Family::TranslateGemma
        );
        assert_eq!(
            Family::from_model_id("Hunyuan-MT-7B-Q6_K.gguf"),
            Family::HunyuanMt
        );
        assert_eq!(Family::from_model_id("Qwen3-14B-Q4_K_M.gguf"), Family::Chat);
    }

    #[test]
    fn strips_reasoning_blocks() {
        assert_eq!(strip_think_block("<think>\n\n</think>\n\nПривет"), "Привет");
        assert_eq!(strip_think_block("рассуждение</think>Привет"), "Привет");
        assert_eq!(strip_think_block("Привет"), "Привет");
    }
}
