use super::{clean_response, language_name, validate};
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
                    "Translate the following text from {} ({}) to {} ({}). Return only the translation in the target language. Preserve every paragraph break and line break from the input. Do not answer in English unless English is the target language.\n{}",
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
        clean_response(&response.content)
    }
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
            completion_tokens: None,
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
            completion_tokens: None,
        };
        assert!(matches!(
            TranslateGemmaAdapter.parse_response(response),
            Err(RuntimeError::MalformedResponse(detail)) if detail.contains("invalid Unicode")
        ));
    }
}
