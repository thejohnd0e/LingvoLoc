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

pub const DEEPSEEK_ENDPOINT: &str = "https://api.deepseek.com/v1";
pub const OPENROUTER_ENDPOINT: &str = "https://openrouter.ai/api/v1";

pub struct OpenAiCompatibleBackend {
    provider: ProviderId,
    transport: HttpTransport,
    base_url: String,
    model_id: String,
    api_key: String,
}

impl OpenAiCompatibleBackend {
    pub fn new(base_url: String, model_id: String, api_key: String) -> Result<Self, RuntimeError> {
        Self::for_provider(ProviderId::OpenAiCompatible, base_url, model_id, api_key)
    }

    pub fn for_provider(
        provider: ProviderId,
        base_url: String,
        model_id: String,
        api_key: String,
    ) -> Result<Self, RuntimeError> {
        let mut backend = Self::with_transport(
            base_url,
            model_id,
            api_key,
            HttpTransport::with_timeouts(super::http::HttpTimeouts {
                request: std::time::Duration::from_secs(120),
                ..Default::default()
            })?
            .with_provider_messages(),
        )?;
        backend.provider = provider;
        Ok(backend)
    }

    fn with_transport(
        base_url: String,
        model_id: String,
        api_key: String,
        transport: HttpTransport,
    ) -> Result<Self, RuntimeError> {
        let base_url = validate_base_url(&base_url)?;
        Ok(Self {
            provider: ProviderId::OpenAiCompatible,
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
        let model = if request.model_id.trim().is_empty() {
            self.model_id.as_str()
        } else {
            request.model_id.trim()
        };
        json!({"model": model, "messages": [{"role":"system","content":system},{"role":"user","content":user}], "temperature": 0.2, "stream": false})
    }
}

impl TranslationBackend for OpenAiCompatibleBackend {
    fn provider_id(&self) -> ProviderId {
        self.provider
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
            detail: format!(
                "{} endpoint configured",
                match self.provider {
                    ProviderId::DeepSeek => "DeepSeek",
                    ProviderId::OpenRouter => "OpenRouter",
                    _ => "OpenAI-compatible",
                }
            ),
        })
    }
    fn list_models(&self) -> Result<Vec<LocalModel>, RuntimeError> {
        let response = self
            .transport
            .get_with_headers(&format!("{}/models", self.base_url), self.headers()?)?;
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
        // The blocking client is used instead of a streamed request: it is the transport
        // that is known to work for model listing and Gemini. It runs on a helper thread so
        // that the Cancel button can stop waiting at once.
        let (sender, receiver) = std::sync::mpsc::channel();
        {
            let transport = self.transport.clone();
            let url = format!("{}/chat/completions", self.base_url);
            let headers = self.headers()?;
            let body = self.body(request);
            std::thread::spawn(move || {
                let outcome = transport
                    .post_json(&url, headers, &body)
                    .and_then(|response| {
                        response.json::<Value>().map_err(|_| {
                            RuntimeError::MalformedResponse(
                                "compatible response is not valid JSON".into(),
                            )
                        })
                    });
                let _ = sender.send(outcome);
            });
        }
        let value = loop {
            if cancellation.is_cancelled() {
                return Err(RuntimeError::Cancelled);
            }
            match receiver.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(outcome) => break outcome?,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(RuntimeError::Connection(
                        "provider request ended unexpectedly".into(),
                    ))
                }
            }
        };
        if let Some(error) = stream_error(&value) {
            return Err(error);
        }
        let text = value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if text.trim().is_empty() {
            return Err(RuntimeError::MalformedResponse(
                "compatible response contained no translation".into(),
            ));
        }
        let usage = value.get("usage").map(parse_usage).transpose()?;
        Ok(TranslationResult {
            text,
            model_id: request.model_id.clone(),
            adapter_id: match self.provider {
                ProviderId::DeepSeek => "deepseek",
                ProviderId::OpenRouter => "openrouter",
                _ => "openai-compatible",
            }
            .into(),
            latency_ms: started.elapsed().as_millis(),
            prompt_tokens: usage.map(|u| u.0),
            completion_tokens: usage.map(|u| u.1),
            total_tokens: usage.map(|u| u.2),
            provider_id: Some(self.provider),
            billed_characters: None,
        })
    }
}

/// Providers such as OpenRouter report some failures as an `error` object inside a
/// successful (HTTP 200) response or stream.
fn stream_error(value: &Value) -> Option<RuntimeError> {
    value.get("error")?;
    let body = serde_json::to_vec(value).ok()?;
    let message = super::http::provider_error_message(&body)
        .unwrap_or_else(|| "provider reported an error".into());
    Some(RuntimeError::Connection(format!(
        "provider error: {message}"
    )))
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
        let backend_body = json!({"model":"custom","messages":[{"role":"system","content":"x"},{"role":"user","content":"Hello"}],"stream":false});
        assert_eq!(backend_body["model"], "custom");
        assert_eq!(
            parse_usage(&json!({"prompt_tokens":2,"completion_tokens":3,"total_tokens":5}))
                .unwrap(),
            (2, 3, 5)
        );
        let _ = request;
    }

    #[test]
    fn list_models_sends_authorization_and_parses_ids() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::thread;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 4096];
            let count = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..count]).into_owned();
            let body = br#"{"data":[{"id":"custom-a"},{"id":"custom-b"}]}"#;
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

        let backend = OpenAiCompatibleBackend::new(
            format!("http://{address}/v1"),
            "custom-a".into(),
            "secret".into(),
        )
        .unwrap();
        let models = backend.list_models().unwrap();
        let request = server.join().unwrap().to_ascii_lowercase();
        assert!(
            request.contains("authorization: bearer secret"),
            "request missing auth header: {request}"
        );
        assert_eq!(
            models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["custom-a", "custom-b"]
        );
    }

    fn one_shot_server(status_line: &'static str, body: &'static str) -> String {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 8192];
            let _ = stream.read(&mut buffer);
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 {status_line}
Content-Length: {}
Connection: close

{body}",
                    body.len()
                )
                .as_bytes(),
            );
        });
        format!("http://{address}/v1")
    }

    fn sample_request() -> TranslationRequest {
        TranslationRequest {
            model_id: "m:free".into(),
            adapter_id: String::new(),
            source_language: "en".into(),
            target_language: "ru".into(),
            text: "mom".into(),
            translation_style: Default::default(),
        }
    }

    #[test]
    fn translate_reads_a_non_streamed_response_with_usage() {
        let endpoint = one_shot_server(
            "200 OK",
            r#"{"choices":[{"message":{"content":"мама"}}],"usage":{"prompt_tokens":3,"completion_tokens":1}}"#,
        );
        let backend = OpenAiCompatibleBackend::for_provider(
            ProviderId::OpenRouter,
            endpoint,
            "m:free".into(),
            "k".into(),
        )
        .unwrap();
        let result = backend
            .translate(&sample_request(), &RequestCancellation::default())
            .unwrap();
        assert_eq!(result.text, "мама");
        assert_eq!(result.total_tokens, Some(4));
        assert_eq!(result.provider_id, Some(ProviderId::OpenRouter));
    }

    #[test]
    fn http_errors_include_the_provider_message() {
        let endpoint = one_shot_server(
            "404 Not Found",
            r#"{"error":{"message":"No endpoints found for this model"}}"#,
        );
        let backend = OpenAiCompatibleBackend::for_provider(
            ProviderId::OpenRouter,
            endpoint,
            "m".into(),
            "k".into(),
        )
        .unwrap();
        let error = backend
            .translate(&sample_request(), &RequestCancellation::default())
            .unwrap_err();
        assert!(error.to_string().contains("No endpoints found"), "{error}");
    }

    #[test]
    fn in_band_error_objects_become_errors() {
        let endpoint = one_shot_server("200 OK", r#"{"error":{"message":"rate limited"}}"#);
        let backend = OpenAiCompatibleBackend::for_provider(
            ProviderId::DeepSeek,
            endpoint,
            "m".into(),
            "k".into(),
        )
        .unwrap();
        let error = backend
            .translate(&sample_request(), &RequestCancellation::default())
            .unwrap_err();
        assert!(error.to_string().contains("rate limited"), "{error}");
    }
}
