use crate::adapters::Family;
use crate::domain::{
    LocalModel, ModelRuntime, RuntimeError, RuntimeMode, RuntimeStatus, Settings,
    TranslationRequest, TranslationResult,
};
use crate::runtimes::{llama_server::StandaloneRuntime, lm_studio::LmStudioRuntime};
use crate::trace::runtime_event as trace_runtime;
use std::time::Instant;

pub fn runtime(settings: &Settings) -> Result<Box<dyn ModelRuntime>, RuntimeError> {
    match settings.runtime_mode {
        RuntimeMode::LmStudio => Ok(Box::new(LmStudioRuntime::new(&settings.endpoint)?)),
        RuntimeMode::Standalone => Ok(Box::new(StandaloneRuntime::new(
            &settings.models_directory,
            &settings.llama_server_path,
            &settings.model_id,
        ))),
        RuntimeMode::OpenAi
        | RuntimeMode::Anthropic
        | RuntimeMode::Gemini
        | RuntimeMode::DeepL
        | RuntimeMode::OpenAiCompatible => Err(RuntimeError::UnsupportedAdapter(
            "cloud backend is not registered yet".into(),
        )),
    }
}

pub fn status(settings: &Settings) -> Result<RuntimeStatus, RuntimeError> {
    runtime(settings)?.status()
}

pub fn list_models(settings: &Settings) -> Result<Vec<LocalModel>, RuntimeError> {
    runtime(settings)?.list_models()
}

pub fn translate(
    settings: &Settings,
    request: TranslationRequest,
) -> Result<TranslationResult, RuntimeError> {
    // The adapter follows the selected model, so the stored adapter id is informational.
    let adapter = Family::from_model_id(&request.model_id).adapter(&request.model_id);
    let mut settings = settings.clone();
    settings.model_id = request.model_id.clone();
    let runtime = runtime(&settings)?;
    let started = Instant::now();
    let built = adapter.build_request(&request)?;
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
        model_id: request.model_id,
        adapter_id: adapter.id().into(),
        latency_ms: started.elapsed().as_millis(),
        prompt_tokens: usage.as_ref().map(|usage| usage.input_tokens),
        completion_tokens: usage.as_ref().map(|usage| usage.output_tokens),
        total_tokens: usage
            .as_ref()
            .map(|usage| usage.input_tokens + usage.output_tokens),
        provider_id: None,
        billed_characters: None,
    })
}
