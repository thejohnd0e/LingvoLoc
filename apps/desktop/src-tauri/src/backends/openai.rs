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

const OPENAI_BASE: &str = "https://api.openai.com/v1";

pub struct OpenAiBackend {
    transport: HttpTransport,
    base_url: String,
    model_id: String,
    api_key: String,
}

impl OpenAiBackend {
    pub fn new(_endpoint: String, model_id: String, api_key: String) -> Result<Self, RuntimeError> {
        Self::with_base_url(OPENAI_BASE, model_id, api_key)
    }

    pub fn with_base_url(
        base_url: &str,
        model_id: String,
        api_key: String,
    ) -> Result<Self, RuntimeError> {
        if base_url != OPENAI_BASE && !is_https_url(base_url) {
            return Err(RuntimeError::InvalidInput(
                "OpenAI endpoint must use HTTPS".into(),
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
        let value = HeaderValue::from_str(&format!("Bearer {}", self.api_key))
            .map_err(|_| RuntimeError::InvalidInput("invalid provider credential".into()))?;
        headers.insert(AUTHORIZATION, value);
        Ok(headers)
    }

    fn response_body(&self, request: &TranslationRequest) -> Value {
        let (system, user) = neutral_prompt(request);
        json!({
            "model": self.model_id,
            "input": [
                { "role": "system", "content": [{"type": "input_text", "text": system}] },
                { "role": "user", "content": [{"type": "input_text", "text": user}] }
            ],
            "stream": true
        })
    }
}

impl TranslationBackend for OpenAiBackend {
    fn provider_id(&self) -> ProviderId {
        ProviderId::OpenAi
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
            RuntimeError::MalformedResponse("OpenAI models response is not valid JSON".into())
        })?;
        parse_models(&value)
    }

    fn translate(
        &self,
        request: &TranslationRequest,
        cancellation: &RequestCancellation,
    ) -> Result<TranslationResult, RuntimeError> {
        let started = Instant::now();
        let body = self.response_body(request);
        let output = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let usage = std::sync::Arc::new(std::sync::Mutex::new(None));
        let output_ref = output.clone();
        let usage_ref = usage.clone();
        let mut buffer = String::new();
        self.transport.stream_post_json(
            &format!("{}/responses", self.base_url),
            self.headers()?,
            &body,
            cancellation.clone(),
            move |chunk| {
                if buffer.is_empty() && chunk.starts_with(b"{") {
                    let value: Value = serde_json::from_slice(chunk).map_err(|_| {
                        RuntimeError::MalformedResponse("OpenAI response is not valid JSON".into())
                    })?;
                    if let Some(text) = parse_output_text(&value) {
                        output_ref
                            .lock()
                            .map_err(|_| {
                                RuntimeError::Connection(
                                    "translation output lock is poisoned".into(),
                                )
                            })?
                            .push_str(&text);
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
                            "OpenAI stream event is not valid JSON".into(),
                        )
                    })?;
                    if value.get("type").and_then(Value::as_str)
                        == Some("response.output_text.delta")
                    {
                        if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                            output_ref
                                .lock()
                                .map_err(|_| {
                                    RuntimeError::Connection(
                                        "translation output lock is poisoned".into(),
                                    )
                                })?
                                .push_str(delta);
                        }
                    }
                    if let Some(value) = value
                        .get("response")
                        .and_then(|response| response.get("usage"))
                    {
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
                "OpenAI response contained no translation".into(),
            ));
        }
        let usage = *usage
            .lock()
            .map_err(|_| RuntimeError::Connection("translation usage lock is poisoned".into()))?;
        Ok(TranslationResult {
            text,
            model_id: request.model_id.clone(),
            adapter_id: "openai-responses".into(),
            latency_ms: started.elapsed().as_millis(),
            prompt_tokens: usage.map(|u| u.0),
            completion_tokens: usage.map(|u| u.1),
            total_tokens: usage.map(|u| u.2),
            provider_id: Some(ProviderId::OpenAi),
            billed_characters: None,
        })
    }
}

pub(crate) fn parse_models(value: &Value) -> Result<Vec<LocalModel>, RuntimeError> {
    value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            RuntimeError::MalformedResponse("OpenAI models response has no data array".into())
        })
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("id").and_then(Value::as_str).map(|id| LocalModel {
                        id: id.into(),
                        owned_by: item
                            .get("owned_by")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        quantization: None,
                    })
                })
                .collect()
        })
}

pub(crate) fn parse_usage(value: &Value) -> Result<(u64, u64, u64), RuntimeError> {
    let input = value
        .get("input_tokens")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            RuntimeError::MalformedResponse("OpenAI usage is missing input_tokens".into())
        })?;
    let output = value
        .get("output_tokens")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            RuntimeError::MalformedResponse("OpenAI usage is missing output_tokens".into())
        })?;
    let total = value
        .get("total_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(input + output);
    Ok((input, output, total))
}

fn parse_output_text(value: &Value) -> Option<String> {
    if let Some(text) = value.get("output_text").and_then(Value::as_str) {
        return Some(text.into());
    }
    let mut text = String::new();
    for item in value.get("output")?.as_array()? {
        for content in item.get("content")?.as_array()? {
            if content.get("type").and_then(Value::as_str) == Some("output_text") {
                text.push_str(content.get("text")?.as_str()?);
            }
        }
    }
    (!text.is_empty()).then_some(text)
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

fn is_https_url(value: &str) -> bool {
    reqwest::Url::parse(value)
        .map(|url| {
            url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none()
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_openai_uses_responses_path_and_usage_shape() {
        let backend = OpenAiBackend::with_base_url(
            "https://api.openai.com/v1",
            "gpt-4o-mini".into(),
            "secret".into(),
        )
        .unwrap();
        assert_eq!(
            format!("{}/responses", backend.base_url),
            "https://api.openai.com/v1/responses"
        );
        let usage = parse_usage(&json!({"input_tokens": 3, "output_tokens": 4, "total_tokens": 7}))
            .unwrap();
        assert_eq!(usage, (3, 4, 7));
    }

    #[test]
    fn native_openai_parses_models_and_sse_deltas() {
        let models = parse_models(&json!({"data":[{"id":"gpt-x","owned_by":"openai"}]})).unwrap();
        assert_eq!(models[0].id, "gpt-x");
        let mut buffer =
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hi\"}\n\n".into();
        let mut event = String::new();
        consume_sse(&mut buffer, |value| {
            event = value.into();
            Ok(())
        })
        .unwrap();
        assert!(event.contains("response.output_text.delta"));
    }
}
