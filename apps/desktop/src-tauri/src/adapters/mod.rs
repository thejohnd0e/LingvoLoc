pub mod chat;
pub mod hunyuan_mt;
pub mod translategemma;

use crate::domain::{RuntimeError, TranslationModelAdapter, TranslationRequest};

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
