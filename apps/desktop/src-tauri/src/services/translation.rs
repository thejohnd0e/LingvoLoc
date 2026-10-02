use crate::backends::{self, RequestCancellation};
use crate::domain::{
    LocalModel, RuntimeError, RuntimeStatus, Settings, TranslationRequest, TranslationResult,
};

pub fn status(settings: &Settings) -> Result<RuntimeStatus, RuntimeError> {
    backends::for_settings(settings)?.status()
}

pub fn list_models(settings: &Settings) -> Result<Vec<LocalModel>, RuntimeError> {
    backends::for_settings(settings)?.list_models()
}

pub fn translate(
    settings: &Settings,
    request: TranslationRequest,
) -> Result<TranslationResult, RuntimeError> {
    backends::for_settings(settings)?.translate(&request, &RequestCancellation::default())
}
