use super::{clean_response, language_name, style_instruction, validate};
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
                    "Translate the following text from {} ({}) to {} ({}). Return only the translation in the target language. Preserve every paragraph break and line break from the input. Do not answer in English unless English is the target language.{}\n{}",
                    language_name(&request.source_language),
                    request.source_language,
                    language_name(&request.target_language),
                    request.target_language,
                    style_instruction(request.translation_style)
                        .map(|instruction| format!("\n{instruction}"))
                        .unwrap_or_default(),
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
            translation_style: crate::domain::TranslationStyle::Neutral,
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
    fn inserts_style_before_source_text() {
        let request = TranslationRequest {
            model_id: "model".into(),
            adapter_id: "translategemma".into(),
            source_language: "en".into(),
            target_language: "fr".into(),
            text: "Hello".into(),
            translation_style: crate::domain::TranslationStyle::Literary,
        };
        let content = TranslateGemmaAdapter
            .build_request(&request)
            .unwrap()
            .messages[0]
            .content
            .clone();
        assert!(content.contains("polished literary prose"));
        assert!(content.find("polished literary prose").unwrap() < content.find("Hello").unwrap());
        assert_eq!(content.matches("Hello").count(), 1);
    }

    #[test]
    fn preserves_the_neutral_prompt() {
        let request = TranslationRequest {
            model_id: "model".into(),
            adapter_id: "translategemma".into(),
            source_language: "en".into(),
            target_language: "fr".into(),
            text: "Hello".into(),
            translation_style: crate::domain::TranslationStyle::Neutral,
        };
        let content = TranslateGemmaAdapter
            .build_request(&request)
            .unwrap()
            .messages[0]
            .content
            .clone();
        assert_eq!(
            content,
            "Translate the following text from English (en) to French (fr). Return only the translation in the target language. Preserve every paragraph break and line break from the input. Do not answer in English unless English is the target language.\nHello"
        );
    }

    #[test]
    fn normalizes_quotes_and_markdown() {
        let response = CompletionResponse {
            model: "model".into(),
            content: "```text\n\"Bonjour\"\n```".into(),
            usage: None,
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
            usage: None,
        };
        assert!(matches!(
            TranslateGemmaAdapter.parse_response(response),
            Err(RuntimeError::MalformedResponse(detail)) if detail.contains("invalid Unicode")
        ));
    }
}
