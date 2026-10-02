use super::TranslationBackend;
use crate::adapters::Family;
use crate::domain::{
    BackendCapabilities, LocalModel, ModelRuntime, ProviderId, RuntimeError, RuntimeMode,
    RuntimeStatus, Settings, TranslationRequest, TranslationResult,
};
use crate::runtimes::{llama_server::StandaloneRuntime, lm_studio::LmStudioRuntime};
use crate::services::request_control::RequestCancellation;
use crate::trace::runtime_event as trace_runtime;
use std::time::Instant;

pub struct LocalBackend {
    settings: Settings,
}

impl LocalBackend {
    pub fn new(settings: Settings) -> Self {
        Self { settings }
    }

    fn runtime(&self, model_id: &str) -> Result<Box<dyn ModelRuntime>, RuntimeError> {
        match self.settings.runtime_mode {
            RuntimeMode::LmStudio => Ok(Box::new(LmStudioRuntime::new(&self.settings.endpoint)?)),
            RuntimeMode::Standalone => Ok(Box::new(StandaloneRuntime::new(
                &self.settings.models_directory,
                &self.settings.llama_server_path,
                model_id,
            ))),
            _ => Err(RuntimeError::UnsupportedAdapter(
                "local backend requires a local runtime".into(),
            )),
        }
    }

    pub(crate) fn build_request(
        &self,
        request: &TranslationRequest,
    ) -> Result<crate::domain::CompletionRequest, RuntimeError> {
        Family::from_model_id(&request.model_id)
            .adapter(&request.model_id)
            .build_request(request)
    }
}

impl TranslationBackend for LocalBackend {
    fn provider_id(&self) -> ProviderId {
        match self.settings.runtime_mode {
            RuntimeMode::LmStudio => ProviderId::LmStudio,
            RuntimeMode::Standalone => ProviderId::LlamaCpp,
            _ => ProviderId::LlamaCpp,
        }
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            model_list: true,
            custom_model_id: false,
            token_usage: true,
            billed_characters: false,
            translation_styles: true,
        }
    }

    fn status(&self) -> Result<RuntimeStatus, RuntimeError> {
        self.runtime(&self.settings.model_id)?.status()
    }

    fn list_models(&self) -> Result<Vec<LocalModel>, RuntimeError> {
        self.runtime(&self.settings.model_id)?.list_models()
    }

    fn translate(
        &self,
        request: &TranslationRequest,
        _cancellation: &RequestCancellation,
    ) -> Result<TranslationResult, RuntimeError> {
        let adapter = Family::from_model_id(&request.model_id).adapter(&request.model_id);
        let runtime = self.runtime(&request.model_id)?;
        let started = Instant::now();
        let built = self.build_request(request)?;
        trace_runtime(
            "translate_begin",
            &format!("chars={}", request.text.chars().count()),
        );
        let completion = runtime.complete(built)?;
        trace_runtime(
            "translate_complete_returned",
            &format!("ms={}", started.elapsed().as_millis()),
        );
        let usage = completion.usage.clone();
        let text = adapter.parse_response(completion)?;
        Ok(TranslationResult {
            text,
            model_id: request.model_id.clone(),
            adapter_id: adapter.id().into(),
            latency_ms: started.elapsed().as_millis(),
            prompt_tokens: usage.as_ref().map(|usage| usage.input_tokens),
            completion_tokens: usage.as_ref().map(|usage| usage.output_tokens),
            total_tokens: usage
                .as_ref()
                .map(|usage| usage.input_tokens + usage.output_tokens),
            provider_id: Some(self.provider_id()),
            billed_characters: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::LocalBackend;
    use crate::adapters::Family;
    use crate::domain::{RuntimeMode, Settings, TranslationRequest};

    #[test]
    fn translate_gemma_request_keeps_the_existing_adapter_shape() {
        let request = TranslationRequest {
            model_id: "translategemma-4b.gguf".into(),
            adapter_id: "informational".into(),
            source_language: "en".into(),
            target_language: "ru".into(),
            text: "Hello".into(),
            translation_style: Default::default(),
        };
        let settings = Settings {
            runtime_mode: RuntimeMode::Standalone,
            models_directory: String::new(),
            llama_server_path: String::new(),
            endpoint: String::new(),
            model_id: request.model_id.clone(),
            adapter_id: String::new(),
            source_language: String::new(),
            target_language: String::new(),
            primary_language: String::new(),
            secondary_language: String::new(),
            translation_style: Default::default(),
            cloud: Default::default(),
        };
        let backend = LocalBackend::new(settings);
        let expected = Family::from_model_id(&request.model_id)
            .adapter(&request.model_id)
            .build_request(&request)
            .unwrap();
        assert_eq!(backend.build_request(&request).unwrap(), expected);
    }
}
