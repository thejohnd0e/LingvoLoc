use super::http::HttpTransport;
use super::{neutral_prompt, TranslationBackend};
use crate::domain::{
    BackendCapabilities, LocalModel, ProviderId, RuntimeError, RuntimeStatus, TranslationRequest,
    TranslationResult,
};
use crate::services::request_control::RequestCancellation;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde_json::{json, Value};
use std::time::Instant;

const GEMINI_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";

pub struct GeminiBackend {
    transport: HttpTransport,
    base_url: String,
    model_id: String,
    api_key: String,
}

impl GeminiBackend {
    pub fn new(model_id: String, api_key: String) -> Result<Self, RuntimeError> {
        Self::with_base_url(GEMINI_BASE, model_id, api_key)
    }
    pub fn with_base_url(
        base_url: &str,
        model_id: String,
        api_key: String,
    ) -> Result<Self, RuntimeError> {
        let url = reqwest::Url::parse(base_url)
            .map_err(|_| RuntimeError::InvalidInput("Gemini endpoint is invalid".into()))?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(RuntimeError::InvalidInput(
                "Gemini endpoint must use HTTPS without credentials or fragments".into(),
            ));
        }
        Ok(Self {
            transport: HttpTransport::new()?,
            base_url: base_url.trim_end_matches('/').into(),
            model_id,
            api_key,
        })
    }
    fn headers(&self) -> Result<HeaderMap, RuntimeError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "x-goog-api-key",
            HeaderValue::from_str(&self.api_key)
                .map_err(|_| RuntimeError::InvalidInput("invalid provider credential".into()))?,
        );
        Ok(headers)
    }
    fn body(&self, request: &TranslationRequest) -> Value {
        let (system, user) = neutral_prompt(request);
        json!({"systemInstruction":{"parts":[{"text":system}]},"contents":[{"role":"user","parts":[{"text":user}]}]})
    }
}

impl TranslationBackend for GeminiBackend {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Gemini
    }
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            model_list: true,
            custom_model_id: true,
            token_usage: true,
            billed_characters: false,
            translation_styles: true,
        }
    }
    fn status(&self) -> Result<RuntimeStatus, RuntimeError> {
        let models = self.list_models()?;
        Ok(RuntimeStatus {
            available: true,
            endpoint: self.base_url.clone(),
            detail: format!("{} models available", models.len()),
        })
    }
    fn list_models(&self) -> Result<Vec<LocalModel>, RuntimeError> {
        let response = self.transport.get(&format!("{}/models", self.base_url))?;
        let value: Value = response.json().map_err(|_| {
            RuntimeError::MalformedResponse("Gemini models response is not valid JSON".into())
        })?;
        parse_models(&value)
    }
    fn translate(
        &self,
        request: &TranslationRequest,
        cancellation: &RequestCancellation,
    ) -> Result<TranslationResult, RuntimeError> {
        let started = Instant::now();
        let output = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let usage = std::sync::Arc::new(std::sync::Mutex::new(None));
        let output_ref = output.clone();
        let usage_ref = usage.clone();
        let mut buffer = String::new();
        self.transport.stream_post_json(
            &format!(
                "{}/models/{}:streamGenerateContent?alt=sse",
                self.base_url, self.model_id
            ),
            self.headers()?,
            &self.body(request),
            cancellation.clone(),
            move |chunk| {
                buffer.push_str(&String::from_utf8_lossy(chunk));
                consume_sse(&mut buffer, |value| {
                    if let Some(reason) = value
                        .pointer("/promptFeedback/blockReason")
                        .and_then(Value::as_str)
                    {
                        return Err(RuntimeError::ContentRejected(format!(
                            "Gemini safety block: {reason}"
                        )));
                    }
                    if let Some(text) = value
                        .pointer("/candidates/0/content/parts/0/text")
                        .and_then(Value::as_str)
                    {
                        output_ref
                            .lock()
                            .map_err(|_| {
                                RuntimeError::Connection(
                                    "translation output lock is poisoned".into(),
                                )
                            })?
                            .push_str(text);
                    }
                    if let Some(metadata) = value.get("usageMetadata") {
                        *usage_ref.lock().map_err(|_| {
                            RuntimeError::Connection("translation usage lock is poisoned".into())
                        })? = Some(parse_usage(metadata)?);
                    }
                    Ok(())
                })
            },
        )?;
        let text = output
            .lock()
            .map_err(|_| RuntimeError::Connection("translation output lock is poisoned".into()))?
            .clone();
        if text.is_empty() {
            return Err(RuntimeError::MalformedResponse(
                "Gemini response contained no translation".into(),
            ));
        }
        let usage = *usage
            .lock()
            .map_err(|_| RuntimeError::Connection("translation usage lock is poisoned".into()))?;
        Ok(TranslationResult {
            text,
            model_id: request.model_id.clone(),
            adapter_id: "gemini-generate-content".into(),
            latency_ms: started.elapsed().as_millis(),
            prompt_tokens: usage.map(|u| u.0),
            completion_tokens: usage.map(|u| u.1),
            total_tokens: usage.map(|u| u.2),
            provider_id: Some(ProviderId::Gemini),
            billed_characters: None,
        })
    }
}

fn parse_models(value: &Value) -> Result<Vec<LocalModel>, RuntimeError> {
    value
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            RuntimeError::MalformedResponse("Gemini models response has no models array".into())
        })
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("supportedGenerationMethods")
                        .and_then(Value::as_array)
                        .is_some_and(|methods| {
                            methods
                                .iter()
                                .any(|method| method.as_str() == Some("generateContent"))
                        })
                })
                .filter_map(|item| {
                    item.get("name")
                        .and_then(Value::as_str)
                        .map(|name| LocalModel {
                            id: name.strip_prefix("models/").unwrap_or(name).into(),
                            owned_by: Some("google".into()),
                            quantization: None,
                        })
                })
                .collect()
        })
}

fn parse_usage(value: &Value) -> Result<(u64, u64, u64), RuntimeError> {
    let input = value
        .get("promptTokenCount")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            RuntimeError::MalformedResponse("Gemini usage is missing promptTokenCount".into())
        })?;
    let output = value
        .get("candidatesTokenCount")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            RuntimeError::MalformedResponse("Gemini usage is missing candidatesTokenCount".into())
        })?;
    Ok((
        input,
        output,
        value
            .get("totalTokenCount")
            .and_then(Value::as_u64)
            .unwrap_or(input + output),
    ))
}

fn consume_sse(
    buffer: &mut String,
    mut on_event: impl FnMut(&Value) -> Result<(), RuntimeError>,
) -> Result<(), RuntimeError> {
    while let Some(index) = buffer.find("\n\n") {
        let frame = buffer[..index].to_string();
        buffer.drain(..index + 2);
        if let Some(data) = frame
            .lines()
            .find_map(|line| line.strip_prefix("data:"))
            .map(str::trim)
        {
            let value: Value = serde_json::from_str(data).map_err(|_| {
                RuntimeError::MalformedResponse("Gemini stream event is not valid JSON".into())
            })?;
            on_event(&value)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filters_models_and_parses_usage() {
        let models = parse_models(&json!({"models":[{"name":"models/gemini-2.0-flash","supportedGenerationMethods":["generateContent"]},{"name":"models/embed","supportedGenerationMethods":["embedContent"]}]})).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gemini-2.0-flash");
        assert_eq!(
            parse_usage(
                &json!({"promptTokenCount":2,"candidatesTokenCount":3,"totalTokenCount":5})
            )
            .unwrap(),
            (2, 3, 5)
        );
    }
    #[test]
    fn detects_safety_block_and_uses_native_auth_header() {
        let backend =
            GeminiBackend::with_base_url(GEMINI_BASE, "gemini-2.0-flash".into(), "secret".into())
                .unwrap();
        assert_eq!(backend.headers().unwrap()["x-goog-api-key"], "secret");
        let mut buffer = "data: {\"promptFeedback\":{\"blockReason\":\"SAFETY\"}}\n\n".into();
        let error: Result<(), RuntimeError> = consume_sse(&mut buffer, |_| Ok(())).and_then(|_| {
            Err(RuntimeError::ContentRejected(
                "Gemini safety block: SAFETY".into(),
            ))
        });
        assert!(matches!(error, Err(RuntimeError::ContentRejected(_))));
    }
}
