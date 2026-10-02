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
    pub fn with_proxy(mut self, proxy: &str) -> Result<Self, RuntimeError> {
        self.transport = self.transport.with_proxy(proxy)?;
        Ok(self)
    }

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
        let scheme = url.scheme();
        let loopback = url
            .host_str()
            .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "::1"));
        if !matches!(scheme, "https" | "http")
            || (scheme == "http" && !loopback)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(RuntimeError::InvalidInput(
                "Gemini endpoint must use HTTPS without credentials or fragments".into(),
            ));
        }
        Ok(Self {
            transport: HttpTransport::with_timeouts(super::http::HttpTimeouts {
                connect: std::time::Duration::from_secs(3),
                idle: std::time::Duration::from_secs(45),
                request: std::time::Duration::from_secs(120),
                max_error_body: super::http::MAX_ERROR_BODY,
            })?,
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
        let response = self
            .transport
            .get_with_headers(&format!("{}/models", self.base_url), self.headers()?)?;
        let value: Value = response.json().map_err(|_| {
            RuntimeError::MalformedResponse("Gemini models response is not valid JSON".into())
        })?;
        parse_models(&value)
    }
    fn translate(
        &self,
        request: &TranslationRequest,
        _cancellation: &RequestCancellation,
    ) -> Result<TranslationResult, RuntimeError> {
        let started = Instant::now();
        let model_id = if request.model_id.trim().is_empty() {
            self.model_id.as_str()
        } else {
            request.model_id.trim()
        };
        let response = self.transport.post_json(
            &format!("{}/models/{model_id}:generateContent", self.base_url),
            self.headers()?,
            &self.body(request),
        )?;
        let value: Value = response.json().map_err(|_| {
            RuntimeError::MalformedResponse("Gemini response is not valid JSON".into())
        })?;
        if let Some(reason) = value
            .pointer("/promptFeedback/blockReason")
            .and_then(Value::as_str)
        {
            return Err(RuntimeError::ContentRejected(format!(
                "Gemini safety block: {reason}"
            )));
        }
        if let Some(reason) = value
            .pointer("/candidates/0/finishReason")
            .and_then(Value::as_str)
        {
            if !matches!(reason, "STOP" | "MAX_TOKENS" | "FINISH_REASON_UNSPECIFIED") {
                return Err(RuntimeError::ContentRejected(format!(
                    "Gemini finish reason: {reason}"
                )));
            }
        }
        let text = extract_candidate_text(&value).ok_or_else(|| {
            RuntimeError::MalformedResponse("Gemini response contained no translation".into())
        })?;
        let usage = value.get("usageMetadata").map(parse_usage).transpose()?;
        Ok(TranslationResult {
            text,
            model_id: model_id.to_string(),
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

fn extract_candidate_text(value: &Value) -> Option<String> {
    let parts = value.pointer("/candidates/0/content/parts")?.as_array()?;
    let mut text = String::new();
    for part in parts {
        if let Some(piece) = part.get("text").and_then(Value::as_str) {
            text.push_str(piece);
        }
    }
    (!text.is_empty()).then_some(text)
}

fn parse_models(value: &Value) -> Result<Vec<LocalModel>, RuntimeError> {
    value
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            RuntimeError::MalformedResponse("Gemini models response has no models array".into())
        })
        .map(|items| {
            let mut models: Vec<LocalModel> = items
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
                        .map(|name| name.strip_prefix("models/").unwrap_or(name))
                        .filter(|id| is_text_generation_model(id))
                        .map(|id| LocalModel {
                            id: id.into(),
                            owned_by: Some("google".into()),
                            quantization: None,
                        })
                })
                .collect();
            models.sort_by(|left, right| {
                model_rank(&left.id)
                    .cmp(&model_rank(&right.id))
                    .then_with(|| left.id.cmp(&right.id))
            });
            models
        })
}

fn is_text_generation_model(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    let banned = [
        "embed",
        "embedding",
        "imagen",
        "veo",
        "tts",
        "audio",
        "robot",
        "computer",
        "aqa",
        "gecko",
        "vision",
        "image",
        "native-audio",
        "live",
    ];
    if banned.iter().any(|part| id.contains(part)) {
        return false;
    }
    id.starts_with("gemini-") || id.starts_with("gemma-")
}

fn model_rank(id: &str) -> u8 {
    let id = id.to_ascii_lowercase();
    if id.contains("flash-lite") {
        0
    } else if id.contains("flash") {
        1
    } else if id.contains("pro") {
        2
    } else if id.starts_with("gemini-") {
        3
    } else {
        4
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filters_models_and_parses_usage() {
        let models = parse_models(&json!({
            "models":[
                {"name":"models/gemini-2.0-flash","supportedGenerationMethods":["generateContent"]},
                {"name":"models/gemini-2.5-flash-lite","supportedGenerationMethods":["generateContent"]},
                {"name":"models/gemma-4-31b-it","supportedGenerationMethods":["generateContent"]},
                {"name":"models/embed","supportedGenerationMethods":["embedContent"]},
                {"name":"models/imagen-3","supportedGenerationMethods":["generateContent"]},
                {"name":"models/gemini-robotics-er-1.5-preview","supportedGenerationMethods":["generateContent"]}
            ]
        })).unwrap();
        assert_eq!(
            models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "gemini-2.5-flash-lite",
                "gemini-2.0-flash",
                "gemma-4-31b-it"
            ]
        );
        assert_eq!(
            parse_usage(
                &json!({"promptTokenCount":2,"candidatesTokenCount":3,"totalTokenCount":5})
            )
            .unwrap(),
            (2, 3, 5)
        );
    }

    #[test]
    fn translate_posts_generate_content_and_returns_text() {
        use crate::services::request_control::RequestCancellation;
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 8192];
            let count = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..count]).into_owned();
            let body = br#"{"candidates":[{"content":{"parts":[{"text":"mom"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":2,"candidatesTokenCount":1,"totalTokenCount":3}}"#;
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        String::from_utf8_lossy(body)
                    )
                    .as_bytes(),
                )
                .unwrap();
            request
        });

        let backend = GeminiBackend::with_base_url(
            &format!("http://{address}"),
            "gemini-2.0-flash".into(),
            "secret".into(),
        )
        .unwrap();
        let result = backend
            .translate(
                &TranslationRequest {
                    model_id: "gemini-2.5-flash-lite".into(),
                    adapter_id: String::new(),
                    source_language: "ru".into(),
                    target_language: "en".into(),
                    text: "мама".into(),
                    translation_style: Default::default(),
                },
                &RequestCancellation::default(),
            )
            .unwrap();
        let request = server.join().unwrap().to_ascii_lowercase();
        assert!(request.contains("post /models/gemini-2.5-flash-lite:generatecontent"));
        assert!(request.contains("x-goog-api-key: secret"));
        assert_eq!(result.text, "mom");
        assert_eq!(result.model_id, "gemini-2.5-flash-lite");
        assert_eq!(result.prompt_tokens, Some(2));
    }

    #[test]
    fn detects_safety_block_and_uses_native_auth_header() {
        let backend =
            GeminiBackend::with_base_url(GEMINI_BASE, "gemini-2.0-flash".into(), "secret".into())
                .unwrap();
        assert_eq!(backend.headers().unwrap()["x-goog-api-key"], "secret");
        let blocked = json!({"promptFeedback":{"blockReason":"SAFETY"}});
        assert_eq!(
            blocked
                .pointer("/promptFeedback/blockReason")
                .and_then(Value::as_str),
            Some("SAFETY")
        );
        assert!(extract_candidate_text(&blocked).is_none());
    }
}
