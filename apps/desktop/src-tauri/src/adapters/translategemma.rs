use crate::domain::{
    ChatMessage, CompletionRequest, CompletionResponse, RuntimeError, TranslationModelAdapter,
    TranslationRequest,
};

pub struct TranslateGemmaAdapter;

impl TranslationModelAdapter for TranslateGemmaAdapter {
    fn id(&self) -> &'static str {
        "translategemma"
    }

    fn build_request(
        &self,
        request: &TranslationRequest,
    ) -> Result<CompletionRequest, RuntimeError> {
        validate(request)?;
        Ok(CompletionRequest {
            model: request.model_id.clone(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: format!(
                    "Translate the following text from {} ({}) to {} ({}). Return only the translation in the target language. Do not answer in English unless English is the target language.\n{}",
                    language_name(&request.source_language),
                    request.source_language,
                    language_name(&request.target_language),
                    request.target_language,
                    request.text
                ),
            }],
            temperature: 0.0,
            max_tokens: 2048,
        })
    }

    fn parse_response(&self, response: CompletionResponse) -> Result<String, RuntimeError> {
        let text = response.content.trim().trim_matches('"').trim();
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
}

fn language_name(code: &str) -> &str {
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

fn validate(request: &TranslationRequest) -> Result<(), RuntimeError> {
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

fn strip_markdown_wrapper(text: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_explicit_language_prompt() {
        let request = TranslationRequest {
            model_id: "translategemma-4b-it@q8_0".into(),
            adapter_id: "translategemma".into(),
            source_language: "en".into(),
            target_language: "fr".into(),
            text: "Hello".into(),
        };
        let built = TranslateGemmaAdapter
            .build_request(&request)
            .expect("request should build");
        assert_eq!(built.temperature, 0.0);
        assert!(built.messages[0]
            .content
            .contains("from English (en) to French (fr)"));
    }

    #[test]
    fn normalizes_quotes_and_markdown() {
        let response = CompletionResponse {
            model: "model".into(),
            content: "```text\n\"Bonjour\"\n```".into(),
        };
        assert_eq!(
            TranslateGemmaAdapter.parse_response(response).unwrap(),
            "\"Bonjour\""
        );
    }

    #[test]
    fn rejects_corrupted_unicode_output() {
        let response = CompletionResponse {
            model: "model".into(),
            content: "Прив�т".into(),
        };
        assert!(matches!(
            TranslateGemmaAdapter.parse_response(response),
            Err(RuntimeError::MalformedResponse(detail)) if detail.contains("invalid Unicode")
        ));
    }
}
