use super::{clean_response, language_name, style_instruction, validate};
use crate::domain::{
    ChatMessage, CompletionRequest, CompletionResponse, RuntimeError, TranslationModelAdapter,
    TranslationRequest,
};

/// Generic adapter for instruction-tuned chat models such as Qwen.
pub struct ChatAdapter {
    /// Qwen3 hybrid models think by default; `/no_think` switches that off.
    pub no_think: bool,
}

impl TranslationModelAdapter for ChatAdapter {
    fn id(&self) -> &'static str {
        "chat"
    }

    fn build_request(
        &self,
        request: &TranslationRequest,
    ) -> Result<CompletionRequest, RuntimeError> {
        validate(request)?;
        let style = style_instruction(request.translation_style)
            .map(|instruction| format!(" {instruction}"))
            .unwrap_or_default();
        let suffix = if self.no_think { "\n/no_think" } else { "" };
        Ok(CompletionRequest {
            model: request.model_id.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".into(),
                    content: format!("You are a professional translator. Output only the translation, without explanations, notes or quotation marks. Preserve formatting, line breaks and meaning.{style}"),
                },
                ChatMessage {
                    role: "user".into(),
                    content: format!(
                        "Translate the following text from {} to {}.\n\n{}{}",
                        language_name(&request.source_language),
                        language_name(&request.target_language),
                        request.text,
                        suffix
                    ),
                },
            ],
            temperature: 0.1,
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

    fn request() -> TranslationRequest {
        TranslationRequest {
            model_id: "Qwen3-14B-Q4_K_M.gguf".into(),
            adapter_id: "chat".into(),
            source_language: "en".into(),
            target_language: "ru".into(),
            text: "Hello".into(),
            translation_style: crate::domain::TranslationStyle::Neutral,
        }
    }

    #[test]
    fn builds_system_and_user_messages() {
        let built = ChatAdapter { no_think: true }
            .build_request(&request())
            .unwrap();
        assert_eq!(built.messages[0].role, "system");
        assert!(built.messages[1]
            .content
            .starts_with("Translate the following text from English to Russian."));
        assert!(built.messages[1].content.ends_with("/no_think"));
    }

    #[test]
    fn adds_style_to_system_message_without_repeating_source() {
        let mut request = request();
        request.translation_style = crate::domain::TranslationStyle::Technical;
        let built = ChatAdapter { no_think: false }
            .build_request(&request)
            .unwrap();
        assert!(built.messages[0]
            .content
            .contains("precise, concise technical"));
        assert_eq!(built.messages[1].content.matches("Hello").count(), 1);
        assert!(built.messages[0]
            .content
            .contains("Output only the translation"));
    }

    #[test]
    fn preserves_the_neutral_prompt() {
        let built = ChatAdapter { no_think: false }
            .build_request(&request())
            .unwrap();
        assert_eq!(
            built.messages[0].content,
            "You are a professional translator. Output only the translation, without explanations, notes or quotation marks. Preserve formatting, line breaks and meaning."
        );
        assert_eq!(
            built.messages[1].content,
            "Translate the following text from English to Russian.\n\nHello"
        );
    }

    #[test]
    fn drops_thinking_from_the_answer() {
        let response = CompletionResponse {
            model: "m".into(),
            content: "<think>\n\n</think>\n\nПривет".into(),
            usage: None,
        };
        assert_eq!(
            ChatAdapter { no_think: false }
                .parse_response(response)
                .unwrap(),
            "Привет"
        );
    }
}
