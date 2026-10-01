use super::{
    chinese_style_instruction, clean_response, language_name, style_instruction, validate,
};
use crate::domain::{
    ChatMessage, CompletionRequest, CompletionResponse, RuntimeError, TranslationModelAdapter,
    TranslationRequest,
};

/// Tencent Hunyuan-MT: a single user message, Chinese prompt for ZH<->XX pairs.
pub struct HunyuanMtAdapter;

impl TranslationModelAdapter for HunyuanMtAdapter {
    fn id(&self) -> &'static str {
        "hunyuan-mt"
    }

    fn build_request(
        &self,
        request: &TranslationRequest,
    ) -> Result<CompletionRequest, RuntimeError> {
        validate(request)?;
        let target = language_name(&request.target_language);
        let content = if request.source_language == "zh" || request.target_language == "zh" {
            let target = if request.target_language == "zh" {
                "中文"
            } else {
                target
            };
            format!(
                "把下面的文本翻译成{target}，不要额外解释。{}\n\n{}",
                chinese_style_instruction(request.translation_style)
                    .map(|instruction| format!("\n{instruction}"))
                    .unwrap_or_default(),
                request.text
            )
        } else {
            format!(
                "Translate the following segment into {target}, without additional explanation.{}\n\n{}",
                style_instruction(request.translation_style)
                    .map(|instruction| format!("\n{instruction}"))
                    .unwrap_or_default(),
                request.text
            )
        };
        Ok(CompletionRequest {
            model: request.model_id.clone(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content,
            }],
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

    fn request(source: &str, target: &str) -> TranslationRequest {
        TranslationRequest {
            model_id: "Hunyuan-MT-7B-Q6_K.gguf".into(),
            adapter_id: "hunyuan-mt".into(),
            source_language: source.into(),
            target_language: target.into(),
            text: "Hello".into(),
            translation_style: crate::domain::TranslationStyle::Neutral,
        }
    }

    #[test]
    fn uses_english_prompt_for_non_chinese_pairs() {
        let built = HunyuanMtAdapter
            .build_request(&request("en", "ru"))
            .unwrap();
        assert!(built.messages[0]
            .content
            .starts_with("Translate the following segment into Russian,"));
    }

    #[test]
    fn uses_chinese_prompt_when_chinese_is_involved() {
        let built = HunyuanMtAdapter
            .build_request(&request("ru", "zh"))
            .unwrap();
        assert!(built.messages[0]
            .content
            .starts_with("把下面的文本翻译成中文"));
    }

    #[test]
    fn adds_english_style_instruction_for_non_chinese_pair() {
        let mut request = request("en", "ru");
        request.translation_style = crate::domain::TranslationStyle::Conversational;
        let content = HunyuanMtAdapter.build_request(&request).unwrap().messages[0]
            .content
            .clone();
        assert!(content.contains("natural conversational language"));
        assert_eq!(content.matches("Hello").count(), 1);
    }

    #[test]
    fn adds_chinese_style_instruction_for_chinese_pair() {
        let mut request = request("zh", "ru");
        request.translation_style = crate::domain::TranslationStyle::Technical;
        let content = HunyuanMtAdapter.build_request(&request).unwrap().messages[0]
            .content
            .clone();
        assert!(content.contains("使用准确、简洁的技术语言"));
        assert_eq!(content.matches("Hello").count(), 1);
    }

    #[test]
    fn preserves_neutral_prompts_for_both_language_paths() {
        let english = HunyuanMtAdapter
            .build_request(&request("en", "ru"))
            .unwrap()
            .messages[0]
            .content
            .clone();
        let chinese = HunyuanMtAdapter
            .build_request(&request("ru", "zh"))
            .unwrap()
            .messages[0]
            .content
            .clone();

        assert_eq!(
            english,
            "Translate the following segment into Russian, without additional explanation.\n\nHello"
        );
        assert_eq!(chinese, "把下面的文本翻译成中文，不要额外解释。\n\nHello");
    }
}
