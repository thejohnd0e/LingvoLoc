//! Translation through a ChatGPT Plus/Pro plan ("Sign in with ChatGPT" plan usage).
//! Requests go to the public Responses API with the user's OAuth access token.

use super::http::{HttpTimeouts, HttpTransport};
use super::{neutral_prompt, TranslationBackend};
use crate::domain::{
    BackendCapabilities, LocalModel, ProviderId, RuntimeError, RuntimeStatus, TranslationRequest,
    TranslationResult,
};
use crate::services::request_control::RequestCancellation;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use std::io::BufRead;
use std::time::{Duration, Instant};

type Usage = (u64, u64, u64);

const API_BASE: &str = "https://api.openai.com/v1";
const XAI_API_BASE: &str = "https://api.x.ai/v1";

pub struct ChatGptBackend {
    provider: ProviderId,
    transport: HttpTransport,
    base_url: String,
    model_id: String,
    access_token: String,
}

impl ChatGptBackend {
    pub fn new(model_id: String, access_token: String) -> Result<Self, RuntimeError> {
        Self::with_base_url(API_BASE.into(), model_id, access_token)
    }

    /// The same Responses API flow against xAI, for a SuperGrok subscription token.
    pub fn for_super_grok(model_id: String, access_token: String) -> Result<Self, RuntimeError> {
        let mut backend = Self::with_base_url(XAI_API_BASE.into(), model_id, access_token)?;
        backend.provider = ProviderId::SuperGrok;
        Ok(backend)
    }

    fn with_base_url(
        base_url: String,
        model_id: String,
        access_token: String,
    ) -> Result<Self, RuntimeError> {
        Ok(Self {
            provider: ProviderId::ChatGpt,
            transport: HttpTransport::with_timeouts(HttpTimeouts {
                request: Duration::from_secs(180),
                ..Default::default()
            })?
            .with_provider_messages(),
            base_url: base_url.trim_end_matches('/').into(),
            model_id,
            access_token,
        })
    }

    pub fn with_proxy(mut self, proxy: &str) -> Result<Self, RuntimeError> {
        self.transport = self.transport.with_proxy(proxy)?;
        Ok(self)
    }

    fn headers(&self, accept: &'static str) -> Result<HeaderMap, RuntimeError> {
        if self.access_token.is_empty() {
            return Err(RuntimeError::Authentication(
                "sign in to this provider in Settings first".into(),
            ));
        }
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(ACCEPT, HeaderValue::from_static(accept));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.access_token))
                .map_err(|_| RuntimeError::InvalidInput("invalid ChatGPT access token".into()))?,
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
        // The plan-usage preview rejects temperature, max_output_tokens and system messages,
        // and requires `store: false` with `stream: true`.
        json!({
            "model": model,
            "instructions": system,
            "input": [{"role": "user", "content": user}],
            "store": false,
            "stream": true,
        })
    }
}

impl TranslationBackend for ChatGptBackend {
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
            available: !self.access_token.is_empty(),
            endpoint: self.base_url.clone(),
            detail: if self.provider == ProviderId::SuperGrok {
                "SuperGrok"
            } else {
                "ChatGPT Plus/Pro"
            }
            .into(),
        })
    }

    fn list_models(&self) -> Result<Vec<LocalModel>, RuntimeError> {
        let response = self.transport.get_with_headers(
            &format!("{}/models", self.base_url),
            self.headers("application/json")?,
        )?;
        let value: Value = response.json().map_err(|_| {
            RuntimeError::MalformedResponse("ChatGPT model list is not valid JSON".into())
        })?;
        parse_models(&value)
    }

    fn translate(
        &self,
        request: &TranslationRequest,
        cancellation: &RequestCancellation,
    ) -> Result<TranslationResult, RuntimeError> {
        let started = Instant::now();
        let (sender, receiver) = std::sync::mpsc::channel();
        {
            let transport = self.transport.clone();
            let url = format!("{}/responses", self.base_url);
            let headers = self.headers("text/event-stream")?;
            let body = self.body(request);
            std::thread::spawn(move || {
                let outcome = transport
                    .post_json(&url, headers, &body)
                    .and_then(|response| parse_response_stream(std::io::BufReader::new(response)));
                let _ = sender.send(outcome);
            });
        }
        let (text, usage) = loop {
            if cancellation.is_cancelled() {
                return Err(RuntimeError::Cancelled);
            }
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(outcome) => break outcome?,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(RuntimeError::Connection(
                        "ChatGPT request ended unexpectedly".into(),
                    ))
                }
            }
        };
        Ok(TranslationResult {
            text,
            model_id: request.model_id.clone(),
            adapter_id: if self.provider == ProviderId::SuperGrok {
                "supergrok-responses"
            } else {
                "chatgpt-responses"
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

fn parse_models(value: &Value) -> Result<Vec<LocalModel>, RuntimeError> {
    if let Some(data) = value.get("data").and_then(Value::as_array) {
        return Ok(data
            .iter()
            .filter_map(|model| model.get("id").and_then(Value::as_str))
            .filter(|id| !id.is_empty() && id.len() <= 200)
            .map(|id| LocalModel {
                id: id.to_string(),
                owned_by: None,
                quantization: None,
            })
            .collect());
    }
    let models = value
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            RuntimeError::MalformedResponse("ChatGPT model list has an unexpected shape".into())
        })?;
    Ok(models
        .iter()
        .filter(|model| model.get("visibility").and_then(Value::as_str) == Some("list"))
        .filter_map(|model| model.get("slug").and_then(Value::as_str))
        .filter(|slug| !slug.is_empty() && slug.len() <= 200)
        .map(|slug| LocalModel {
            id: slug.to_string(),
            owned_by: None,
            quantization: None,
        })
        .collect())
}

fn event_error(value: &Value, fallback: &str) -> RuntimeError {
    let message = value
        .pointer("/response/error/message")
        .or_else(|| value.pointer("/error/message"))
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or(fallback);
    let cleaned: String = message
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(200)
        .collect();
    RuntimeError::Connection(format!("ChatGPT: {}", cleaned.trim()))
}

/// Reads a Responses API server-sent-event stream and returns the translated text and
/// token usage. The stream must end with `response.completed`.
fn parse_response_stream<R: BufRead>(reader: R) -> Result<(String, Option<Usage>), RuntimeError> {
    const MAX_TEXT: usize = 16 * 1024 * 1024;
    let mut text = String::new();
    let mut usage = None;
    let mut completed = false;
    for line in reader.lines() {
        let line = line.map_err(|error| {
            if error.kind() == std::io::ErrorKind::TimedOut {
                RuntimeError::Timeout("ChatGPT response timed out".into())
            } else {
                RuntimeError::Connection("ChatGPT stream was interrupted".into())
            }
        })?;
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let value: Value = serde_json::from_str(data).map_err(|_| {
            RuntimeError::MalformedResponse("ChatGPT stream event is not valid JSON".into())
        })?;
        match value.get("type").and_then(Value::as_str) {
            Some("response.output_text.delta") => {
                if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                    if text.len() + delta.len() > MAX_TEXT {
                        return Err(RuntimeError::MalformedResponse(
                            "ChatGPT response is too large".into(),
                        ));
                    }
                    text.push_str(delta);
                }
            }
            Some("response.completed") => {
                if let Some(found) = value.pointer("/response/usage") {
                    let input = found.get("input_tokens").and_then(Value::as_u64);
                    let output = found.get("output_tokens").and_then(Value::as_u64);
                    if let (Some(input), Some(output)) = (input, output) {
                        let total = found
                            .get("total_tokens")
                            .and_then(Value::as_u64)
                            .unwrap_or(input + output);
                        usage = Some((input, output, total));
                    }
                }
                completed = true;
                break;
            }
            Some("response.incomplete") => {
                return Err(event_error(&value, "response was incomplete"));
            }
            Some("response.failed") | Some("error") => {
                return Err(event_error(&value, "request failed"));
            }
            _ => {}
        }
    }
    if !completed {
        return Err(RuntimeError::Connection(
            "ChatGPT stream ended before the response was complete".into(),
        ));
    }
    if text.trim().is_empty() {
        return Err(RuntimeError::MalformedResponse(
            "ChatGPT response contained no translation".into(),
        ));
    }
    Ok((text, usage))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_text_and_usage_are_collected() {
        let stream = "event: response.created\ndata: {\"type\":\"response.created\"}\n\n\
data: {\"type\":\"response.output_text.delta\",\"delta\":\"ма\"}\n\n\
data: {\"type\":\"response.output_text.delta\",\"delta\":\"ма\"}\n\n\
data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":5,\"output_tokens\":2}}}\n\n";
        let (text, usage) = parse_response_stream(stream.as_bytes()).unwrap();
        assert_eq!(text, "мама");
        assert_eq!(usage, Some((5, 2, 7)));
    }

    #[test]
    fn truncated_or_failed_streams_are_errors() {
        let truncated = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"x\"}\n\n";
        assert!(parse_response_stream(truncated.as_bytes()).is_err());
        let failed = "data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"message\":\"nope\"}}}\n\n";
        let error = parse_response_stream(failed.as_bytes()).unwrap_err();
        assert!(error.to_string().contains("nope"), "{error}");
    }

    #[test]
    fn only_listed_models_are_offered() {
        let models = parse_models(&json!({"models":[
            {"slug":"gpt-5","display_name":"GPT-5","visibility":"list"},
            {"slug":"hidden","display_name":"Hidden","visibility":"hide"}
        ]}))
        .unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-5");
    }

    #[test]
    fn openai_style_model_lists_are_accepted() {
        let models = parse_models(&json!({"data":[{"id":"grok-4"},{"id":"grok-3-mini"}]})).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "grok-4");
    }

    #[test]
    fn request_body_follows_the_plan_usage_rules() {
        let backend = ChatGptBackend::new("gpt-5".into(), "token".into()).unwrap();
        let body = backend.body(&TranslationRequest {
            model_id: "gpt-5".into(),
            adapter_id: String::new(),
            source_language: "en".into(),
            target_language: "ru".into(),
            text: "mom".into(),
            translation_style: Default::default(),
        });
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert!(body.get("temperature").is_none());
        assert_eq!(body["input"][0]["role"], "user");
        assert!(body["instructions"]
            .as_str()
            .unwrap()
            .contains("translator"));
    }
}
