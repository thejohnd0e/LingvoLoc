use super::{clean_response, language_name, validate};
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
                "把下面的文本翻译成{target}，不要额外解释。\n\n{}",
                request.text
            )
        } else {
            format!(
                "Translate the following segment into {target}, without additional explanation.\n\n{}",
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
}
