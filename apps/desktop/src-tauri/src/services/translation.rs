use crate::adapters::translategemma::TranslateGemmaAdapter;
use crate::domain::{
    LocalModel, ModelRuntime, RuntimeError, RuntimeStatus, Settings, TranslationModelAdapter,
    TranslationRequest, TranslationResult,
};
use crate::runtimes::lm_studio::LmStudioRuntime;
use std::time::Instant;

pub fn runtime(settings: &Settings) -> Result<LmStudioRuntime, RuntimeError> {
    LmStudioRuntime::new(&settings.endpoint)
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
    if request.adapter_id != settings.adapter_id {
        return Err(RuntimeError::UnsupportedAdapter(request.adapter_id));
    }
    let adapter = TranslateGemmaAdapter;
    if adapter.id() != settings.adapter_id {
        return Err(RuntimeError::UnsupportedAdapter(
            settings.adapter_id.clone(),
        ));
    }
    let runtime = runtime(settings)?;
    let started = Instant::now();
    let completion = runtime.complete(adapter.build_request(&request)?)?;
    let text = adapter.parse_response(completion)?;
    Ok(TranslationResult {
        text,
        model_id: request.model_id,
        adapter_id: request.adapter_id,
        latency_ms: started.elapsed().as_millis(),
    })
}
