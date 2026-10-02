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

const ANTHROPIC_BASE: &str = "https://api.anthropic.com";

pub struct AnthropicBackend {
    transport: HttpTransport,
    base_url: String,
    model_id: String,
    api_key: String,
}

impl AnthropicBackend {
    pub fn new(model_id: String, api_key: String) -> Result<Self, RuntimeError> {
        Self::with_base_url(ANTHROPIC_BASE, model_id, api_key)
    }

    pub fn with_base_url(
        base_url: &str,
        model_id: String,
        api_key: String,
    ) -> Result<Self, RuntimeError> {
        let url = reqwest::Url::parse(base_url)
            .map_err(|_| RuntimeError::InvalidInput("Anthropic endpoint is invalid".into()))?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(RuntimeError::InvalidInput(
                "Anthropic endpoint must use HTTPS without credentials or fragments".into(),
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
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&self.api_key)
                .map_err(|_| RuntimeError::InvalidInput("invalid provider credential".into()))?,
        );
        Ok(headers)
    }

    fn body(&self, request: &TranslationRequest) -> Value {
        let (system, user) = neutral_prompt(request);
        let model = if request.model_id.trim().is_empty() {
            self.model_id.as_str()
        } else {
            request.model_id.trim()
        };
        json!({
            "model": model,
            "max_tokens": 4096,
            "system": system,
            "messages": [{"role": "user", "content": user}],
            "stream": true
        })
    }
}

impl TranslationBackend for AnthropicBackend {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Anthropic
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
        let mut models = Vec::new();
        let mut next_after = None;
        loop {
            let url = match &next_after {
                Some(after) => format!("{}/v1/models?limit=100&after_id={after}", self.base_url),
                None => format!("{}/v1/models?limit=100", self.base_url),
            };
            let response = self.transport.get_with_headers(&url, self.headers()?)?;
            let value: Value = response.json().map_err(|_| {
                RuntimeError::MalformedResponse(
                    "Anthropic models response is not valid JSON".into(),
                )
            })?;
            models.extend(parse_models(&value)?);
            if value.get("has_more").and_then(Value::as_bool) != Some(true) {
                break;
            }
            next_after = value
                .get("last_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if next_after.is_none() {
                return Err(RuntimeError::MalformedResponse(
                    "Anthropic pagination has_more without last_id".into(),
                ));
            }
        }
        Ok(models)
    }

    fn translate(
        &self,
        request: &TranslationRequest,
        cancellation: &RequestCancellation,
    ) -> Result<TranslationResult, RuntimeError> {
        let started = Instant::now();
        let output = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let usage = std::sync::Arc::new(std::sync::Mutex::new((None, None)));
        let output_ref = output.clone();
        let usage_ref = usage.clone();
        let mut buffer = String::new();
        self.transport.stream_post_json(
            &format!("{}/v1/messages", self.base_url),
            self.headers()?,
            &self.body(request),
            cancellation.clone(),
            move |chunk| {
                buffer.push_str(&String::from_utf8_lossy(chunk));
                consume_sse(&mut buffer, |event, value| {
                    match event {
                        "message_start" => {
                            if let Some(tokens) = value
                                .pointer("/message/usage/input_tokens")
                                .and_then(Value::as_u64)
                            {
                                usage_ref
                                    .lock()
                                    .map_err(|_| {
                                        RuntimeError::Connection(
                                            "translation usage lock is poisoned".into(),
                                        )
                                    })?
                                    .0 = Some(tokens);
                            }
                        }
                        "content_block_delta" => {
                            if let Some(text) = value.pointer("/delta/text").and_then(Value::as_str)
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
                        }
                        "message_delta" => {
                            if let Some(tokens) = value
                                .pointer("/usage/output_tokens")
                                .and_then(Value::as_u64)
                            {
                                usage_ref
                                    .lock()
                                    .map_err(|_| {
                                        RuntimeError::Connection(
                                            "translation usage lock is poisoned".into(),
                                        )
                                    })?
                                    .1 = Some(tokens);
                            }
                        }
                        _ => {}
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
                "Anthropic response contained no translation".into(),
            ));
        }
        let (input, output) = *usage
            .lock()
            .map_err(|_| RuntimeError::Connection("translation usage lock is poisoned".into()))?;
        Ok(TranslationResult {
            text,
            model_id: request.model_id.clone(),
            adapter_id: "anthropic-messages".into(),
            latency_ms: started.elapsed().as_millis(),
            prompt_tokens: input,
            completion_tokens: output,
            total_tokens: input.zip(output).map(|(input, output)| input + output),
            provider_id: Some(ProviderId::Anthropic),
            billed_characters: None,
        })
    }
}

fn parse_models(value: &Value) -> Result<Vec<LocalModel>, RuntimeError> {
    value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            RuntimeError::MalformedResponse("Anthropic models response has no data array".into())
        })
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("id").and_then(Value::as_str).map(|id| LocalModel {
                        id: id.into(),
                        owned_by: Some("anthropic".into()),
                        quantization: None,
                    })
                })
                .collect()
        })
}

fn consume_sse(
    buffer: &mut String,
    mut on_event: impl FnMut(&str, &Value) -> Result<(), RuntimeError>,
) -> Result<(), RuntimeError> {
    while let Some(index) = buffer.find("\n\n") {
        let frame = buffer[..index].to_string();
        buffer.drain(..index + 2);
        let event = frame
            .lines()
            .find_map(|line| line.strip_prefix("event:"))
            .map(str::trim)
            .unwrap_or_default();
        let data = frame
            .lines()
            .find_map(|line| line.strip_prefix("data:"))
            .map(str::trim)
            .unwrap_or_default();
        if !data.is_empty() {
            let value: Value = serde_json::from_str(data).map_err(|_| {
                RuntimeError::MalformedResponse("Anthropic stream event is not valid JSON".into())
            })?;
            on_event(event, &value)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_native_headers_and_messages_shape() {
        let backend = AnthropicBackend::with_base_url(
            "https://api.anthropic.com",
            "claude-3-5-haiku-latest".into(),
            "secret".into(),
        )
        .unwrap();
        let headers = backend.headers().unwrap();
        assert_eq!(headers["anthropic-version"], "2023-06-01");
        assert_eq!(headers["x-api-key"], "secret");
        let request = TranslationRequest {
            model_id: "claude-3-5-haiku-latest".into(),
            adapter_id: String::new(),
            source_language: "en".into(),
            target_language: "ru".into(),
            text: "Hello".into(),
            translation_style: Default::default(),
        };
        assert_eq!(backend.body(&request)["messages"][0]["role"], "user");
        assert!(backend.body(&request).get("api_key").is_none());
    }

    #[test]
    fn parses_paginated_models_and_stream_usage() {
        let first =
            parse_models(&json!({"data":[{"id":"a"}],"has_more":true,"last_id":"a"})).unwrap();
        let second = parse_models(&json!({"data":[{"id":"b"}],"has_more":false})).unwrap();
        assert_eq!(first[0].id, "a");
        assert_eq!(second[0].id, "b");
        let mut buffer = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":4}}}\n\n".into();
        let mut input = None;
        consume_sse(&mut buffer, |event, value| {
            if event == "message_start" {
                input = value
                    .pointer("/message/usage/input_tokens")
                    .and_then(Value::as_u64);
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(input, Some(4));
    }

    #[test]
    fn list_models_request_includes_api_key_header() {
        let backend = AnthropicBackend::with_base_url(
            "https://api.anthropic.com",
            "claude-3-5-haiku-latest".into(),
            "secret".into(),
        )
        .unwrap();
        let headers = backend.headers().unwrap();
        assert_eq!(headers["x-api-key"], "secret");
        assert_eq!(headers["anthropic-version"], "2023-06-01");
    }
}
