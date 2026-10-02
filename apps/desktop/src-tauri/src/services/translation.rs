use crate::backends;
use crate::domain::{
    LocalModel, RuntimeError, RuntimeStatus, Settings, TranslationRequest, TranslationResult,
};
use crate::services::credentials::CredentialStore;
use crate::services::request_control::RequestCancellation;

pub fn status(settings: &Settings) -> Result<RuntimeStatus, RuntimeError> {
    backends::for_settings(settings)?.status()
}

pub fn status_with_credentials(
    settings: &Settings,
    credentials: &CredentialStore,
) -> Result<RuntimeStatus, RuntimeError> {
    backends::for_settings_with_credentials(settings, credentials)?.status()
}

pub fn list_models(settings: &Settings) -> Result<Vec<LocalModel>, RuntimeError> {
    backends::for_settings(settings)?.list_models()
}

pub fn list_models_with_credentials(
    settings: &Settings,
    credentials: &CredentialStore,
) -> Result<Vec<LocalModel>, RuntimeError> {
    backends::for_settings_with_credentials(settings, credentials)?.list_models()
}

pub fn translate(
    settings: &Settings,
    request: TranslationRequest,
) -> Result<TranslationResult, RuntimeError> {
    translate_with_cancellation(settings, request, &RequestCancellation::default())
}

pub fn translate_with_cancellation(
    settings: &Settings,
    request: TranslationRequest,
    cancellation: &RequestCancellation,
) -> Result<TranslationResult, RuntimeError> {
    backends::for_settings(settings)?.translate(&request, cancellation)
}

pub fn translate_with_cancellation_and_credentials(
    settings: &Settings,
    request: TranslationRequest,
    cancellation: &RequestCancellation,
    credentials: &CredentialStore,
) -> Result<TranslationResult, RuntimeError> {
    backends::for_settings_with_credentials(settings, credentials)?
        .translate(&request, cancellation)
}
