pub mod anthropic;
pub mod deepl;
pub mod gemini;
pub mod http;
pub mod local;
pub mod openai;
pub mod openai_compatible;

use crate::domain::TranslationStyle;
use crate::domain::{
    BackendCapabilities, LocalModel, ProviderId, RuntimeError, RuntimeStatus, Settings,
    TranslationRequest, TranslationResult,
};
use crate::services::credentials::{map_error, CredentialStore};
use crate::services::request_control::RequestCancellation;

pub(crate) fn neutral_prompt(request: &TranslationRequest) -> (String, String) {
    let style = match request.translation_style {
        TranslationStyle::Neutral => "neutral",
        TranslationStyle::Literary => "literary",
        TranslationStyle::Technical => "technical",
        TranslationStyle::Conversational => "conversational",
    };
    (
        format!(
            "You are a precise translator. Translate from {} to {}. Use a {} style. Return only the translation.",
            request.source_language, request.target_language, style
        ),
        request.text.clone(),
    )
}

pub trait TranslationBackend: Send + Sync {
    fn provider_id(&self) -> ProviderId;
    fn capabilities(&self) -> BackendCapabilities;
    fn status(&self) -> Result<RuntimeStatus, RuntimeError>;
    fn list_models(&self) -> Result<Vec<LocalModel>, RuntimeError>;
    fn translate(
        &self,
        request: &TranslationRequest,
        cancellation: &RequestCancellation,
    ) -> Result<TranslationResult, RuntimeError>;
}

pub fn for_settings(settings: &Settings) -> Result<Box<dyn TranslationBackend>, RuntimeError> {
    for_settings_with_key(settings, String::new())
}

pub fn for_settings_with_credentials(
    settings: &Settings,
    credentials: &CredentialStore,
) -> Result<Box<dyn TranslationBackend>, RuntimeError> {
    let key = match settings.runtime_mode {
        crate::domain::RuntimeMode::OpenAi => credentials.get(crate::domain::ProviderId::OpenAi),
        crate::domain::RuntimeMode::Anthropic => {
            credentials.get(crate::domain::ProviderId::Anthropic)
        }
        crate::domain::RuntimeMode::Gemini => credentials.get(crate::domain::ProviderId::Gemini),
        crate::domain::RuntimeMode::DeepL => credentials.get(crate::domain::ProviderId::DeepL),
        crate::domain::RuntimeMode::OpenAiCompatible => {
            credentials.get(crate::domain::ProviderId::OpenAiCompatible)
        }
        crate::domain::RuntimeMode::LmStudio | crate::domain::RuntimeMode::Standalone => {
            Ok(String::new())
        }
    }
    .map_err(map_error)?;
    for_settings_with_key(settings, key)
}

fn for_settings_with_key(
    settings: &Settings,
    key: String,
) -> Result<Box<dyn TranslationBackend>, RuntimeError> {
    match settings.runtime_mode {
        crate::domain::RuntimeMode::LmStudio | crate::domain::RuntimeMode::Standalone => {
            Ok(Box::new(local::LocalBackend::new(settings.clone())))
        }
        crate::domain::RuntimeMode::OpenAi => Ok(Box::new(openai::OpenAiBackend::new(
            settings.endpoint.clone(),
            settings.cloud.open_ai.model_id.clone(),
            key,
        )?)),
        crate::domain::RuntimeMode::Anthropic => Ok(Box::new(anthropic::AnthropicBackend::new(
            settings.cloud.anthropic.model_id.clone(),
            key,
        )?)),
        crate::domain::RuntimeMode::Gemini => Ok(Box::new(gemini::GeminiBackend::new(
            settings.cloud.gemini.model_id.clone(),
            key,
        )?)),
        crate::domain::RuntimeMode::DeepL => Ok(Box::new(deepl::DeepLBackend::new(
            &settings.cloud.deep_l.plan,
            key,
        )?)),
        crate::domain::RuntimeMode::OpenAiCompatible => {
            Ok(Box::new(openai_compatible::OpenAiCompatibleBackend::new(
                settings.cloud.open_ai_compatible.endpoint.clone(),
                settings.cloud.open_ai_compatible.model_id.clone(),
                key,
            )?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{local::LocalBackend, TranslationBackend};
    use crate::domain::{ProviderId, RuntimeMode, Settings};

    #[test]
    fn local_backend_reports_the_selected_provider() {
        let standalone = LocalBackend::new(settings(RuntimeMode::Standalone));
        assert_eq!(standalone.provider_id(), ProviderId::LlamaCpp);
        let lm_studio = LocalBackend::new(settings(RuntimeMode::LmStudio));
        assert_eq!(lm_studio.provider_id(), ProviderId::LmStudio);
    }

    #[test]
    fn local_backend_exposes_model_and_style_capabilities() {
        let capabilities = LocalBackend::new(settings(RuntimeMode::Standalone)).capabilities();
        assert!(capabilities.model_list);
        assert!(!capabilities.custom_model_id);
        assert!(capabilities.translation_styles);
        assert!(capabilities.token_usage);
    }

    fn settings(runtime_mode: RuntimeMode) -> Settings {
        Settings {
            runtime_mode,
            models_directory: String::new(),
            llama_server_path: String::new(),
            endpoint: "http://127.0.0.1:1234/v1".into(),
            model_id: "translategemma-4b.gguf".into(),
            adapter_id: "informational".into(),
            source_language: "en".into(),
            target_language: "ru".into(),
            primary_language: "en".into(),
            secondary_language: "ru".into(),
            translation_style: Default::default(),
            cloud: Default::default(),
        }
    }
}
