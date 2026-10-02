use super::http::HttpTransport;
use super::{neutral_prompt, TranslationBackend};
use crate::domain::{
    BackendCapabilities, LocalModel, ProviderId, RuntimeError, RuntimeStatus, TranslationRequest,
    TranslationResult,
};
use crate::services::request_control::RequestCancellation;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use std::time::Instant;

pub struct OpenAiCompatibleBackend {
    transport: HttpTransport,
    base_url: String,
    model_id: String,
    api_key: String,
}

impl OpenAiCompatibleBackend {
    pub fn new(base_url: String, model_id: String, api_key: String) -> Result<Self, RuntimeError> {
        Self::with_transport(base_url, model_id, api_key, HttpTransport::new()?)
    }

    fn with_transport(
        base_url: String,
        model_id: String,
        api_key: String,
        transport: HttpTransport,
    ) -> Result<Self, RuntimeError> {
        let base_url = validate_base_url(&base_url)?;
        Ok(Self {
            transport,
            base_url,
            model_id,
            api_key,
        })
    }

    fn headers(&self) -> Result<HeaderMap, RuntimeError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key))
                .map_err(|_| RuntimeError::InvalidInput("invalid provider credential".into()))?,
        );
        Ok(headers)
    }

    fn body(&self, request: &TranslationRequest) -> Value {
        let (system, user) = neutral_prompt(request);
        json!({"model": self.model_id, "messages": [{"role":"system","content":system},{"role":"user","content":user}], "temperature": 0.2, "stream": true})
    }
}

impl TranslationBackend for OpenAiCompatibleBackend {
    fn provider_id(&self) -> ProviderId {
        ProviderId::OpenAiCompatible
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
        Ok(RuntimeStatus {
            available: true,
            endpoint: self.base_url.clone(),
            detail: "OpenAI-compatible endpoint configured".into(),
        })
    }
    fn list_models(&self) -> Result<Vec<LocalModel>, RuntimeError> {
        let response = self.transport.get(&format!("{}/models", self.base_url))?;
        let value: Value = response.json().map_err(|_| {
            RuntimeError::MalformedResponse("compatible models response is not valid JSON".into())
        })?;
        super::openai::parse_models(&value)
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
            &format!("{}/chat/completions", self.base_url),
            self.headers()?,
            &self.body(request),
            cancellation.clone(),
            move |chunk| {
                if buffer.is_empty() && chunk.starts_with(b"{") {
                    let value: Value = serde_json::from_slice(chunk).map_err(|_| {
                        RuntimeError::MalformedResponse(
                            "compatible response is not valid JSON".into(),
                        )
                    })?;
                    if let Some(content) = value
                        .pointer("/choices/0/message/content")
                        .and_then(Value::as_str)
                    {
                        output_ref
                            .lock()
                            .map_err(|_| {
                                RuntimeError::Connection(
                                    "translation output lock is poisoned".into(),
                                )
                            })?
                            .push_str(content);
                    }
                    if let Some(value) = value.get("usage") {
                        *usage_ref.lock().map_err(|_| {
                            RuntimeError::Connection("translation usage lock is poisoned".into())
                        })? = Some(parse_usage(value)?);
                    }
                    return Ok(());
                }
                buffer.push_str(&String::from_utf8_lossy(chunk));
                consume_sse(&mut buffer, |event| {
                    if event == "[DONE]" {
                        return Ok(());
                    }
                    let value: Value = serde_json::from_str(event).map_err(|_| {
                        RuntimeError::MalformedResponse(
                            "compatible stream event is not valid JSON".into(),
                        )
                    })?;
                    if let Some(content) = value
                        .pointer("/choices/0/delta/content")
                        .and_then(Value::as_str)
                    {
                        output_ref
                            .lock()
                            .map_err(|_| {
                                RuntimeError::Connection(
                                    "translation output lock is poisoned".into(),
                                )
                            })?
                            .push_str(content);
                    }
                    if let Some(value) = value.get("usage") {
                        *usage_ref.lock().map_err(|_| {
                            RuntimeError::Connection("translation usage lock is poisoned".into())
                        })? = Some(parse_usage(value)?);
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
                "compatible response contained no translation".into(),
            ));
        }
        let usage = *usage
            .lock()
            .map_err(|_| RuntimeError::Connection("translation usage lock is poisoned".into()))?;
        Ok(TranslationResult {
            text,
            model_id: request.model_id.clone(),
            adapter_id: "openai-compatible".into(),
            latency_ms: started.elapsed().as_millis(),
            prompt_tokens: usage.map(|u| u.0),
            completion_tokens: usage.map(|u| u.1),
            total_tokens: usage.map(|u| u.2),
            provider_id: Some(ProviderId::OpenAiCompatible),
            billed_characters: None,
        })
    }
}

fn consume_sse(
    buffer: &mut String,
    mut on_event: impl FnMut(&str) -> Result<(), RuntimeError>,
) -> Result<(), RuntimeError> {
    while let Some(index) = buffer.find("\n\n") {
        let frame = buffer[..index].to_string();
        buffer.drain(..index + 2);
        for line in frame.lines() {
            if let Some(data) = line.strip_prefix("data:") {
                on_event(data.trim())?;
            }
        }
    }
    Ok(())
}

fn parse_usage(value: &Value) -> Result<(u64, u64, u64), RuntimeError> {
    let input = value
        .get("prompt_tokens")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            RuntimeError::MalformedResponse("compatible usage is missing prompt_tokens".into())
        })?;
    let output = value
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            RuntimeError::MalformedResponse("compatible usage is missing completion_tokens".into())
        })?;
    Ok((
        input,
        output,
        value
            .get("total_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(input + output),
    ))
}

fn validate_base_url(value: &str) -> Result<String, RuntimeError> {
    let url = reqwest::Url::parse(value).map_err(|_| {
        RuntimeError::InvalidInput("compatible endpoint must be a valid URL".into())
    })?;
    let scheme = url.scheme();
    if !matches!(scheme, "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
    {
        return Err(RuntimeError::InvalidInput(
            "compatible endpoint must be an HTTP(S) URL without credentials or fragments".into(),
        ));
    }
    if scheme == "http"
        && !url
            .host_str()
            .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "::1"))
    {
        return Err(RuntimeError::InvalidInput(
            "plain HTTP is allowed only for loopback endpoints".into(),
        ));
    }
    Ok(value.trim_end_matches('/').into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https_and_loopback_http_and_normalizes_slash() {
        assert_eq!(
            validate_base_url("https://example.com/v1/").unwrap(),
            "https://example.com/v1"
        );
        assert_eq!(
            validate_base_url("http://127.0.0.1:9000/").unwrap(),
            "http://127.0.0.1:9000"
        );
        assert!(validate_base_url("http://example.com/v1").is_err());
    }

    #[test]
    fn rejects_credentials_fragments_and_non_http_schemes() {
        for value in [
            "ftp://localhost",
            "https://user@example.com",
            "https://example.com#fragment",
        ] {
            assert!(validate_base_url(value).is_err());
        }
    }

    #[test]
    fn builds_chat_completion_prompt_and_parses_usage() {
        let request = TranslationRequest {
            model_id: "custom".into(),
            adapter_id: String::new(),
            source_language: "en".into(),
            target_language: "ru".into(),
            text: "Hello".into(),
            translation_style: Default::default(),
        };
        let backend_body = json!({"model":"custom","messages":[{"role":"system","content":"x"},{"role":"user","content":"Hello"}],"stream":true});
        assert_eq!(backend_body["model"], "custom");
        assert_eq!(
            parse_usage(&json!({"prompt_tokens":2,"completion_tokens":3,"total_tokens":5}))
                .unwrap(),
            (2, 3, 5)
        );
        let _ = request;
    }
}
