pub mod local;

use crate::domain::{
    BackendCapabilities, LocalModel, ProviderId, RuntimeError, RuntimeStatus, Settings,
    TranslationRequest, TranslationResult,
};

#[derive(Debug, Default)]
pub struct RequestCancellation;

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
    match settings.runtime_mode {
        crate::domain::RuntimeMode::LmStudio | crate::domain::RuntimeMode::Standalone => {
            Ok(Box::new(local::LocalBackend::new(settings.clone())))
        }
        _ => Err(RuntimeError::UnsupportedAdapter(
            "cloud backend is not registered yet".into(),
        )),
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
