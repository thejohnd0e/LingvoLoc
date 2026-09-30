use crate::domain::{
    ChatMessage, CompletionRequest, CompletionResponse, LocalModel, ModelRuntime, RuntimeError,
    RuntimeStatus,
};
use crate::trace::runtime_event as trace_runtime;
use reqwest::blocking::Client;
use serde::Deserialize;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A running model must produce headers or a token at least this often.
const IDLE_TIMEOUT: Duration = Duration::from_secs(45);
/// Safety net for a request that keeps trickling data forever.
const MAX_REQUEST_TIME: Duration = Duration::from_secs(900);
const MAX_BODY_BYTES: usize = 1 << 20;

pub struct LmStudioRuntime {
    endpoint: String,
    idle_timeout: Duration,
}

impl LmStudioRuntime {
    pub fn new(endpoint: &str) -> Result<Self, RuntimeError> {
        let endpoint = endpoint.trim_end_matches('/').to_string();
        if endpoint.is_empty() {
            return Err(RuntimeError::InvalidInput(
                "endpoint must not be empty".into(),
            ));
        }
        Ok(Self {
            endpoint,
            idle_timeout: IDLE_TIMEOUT,
        })
    }

    #[cfg(test)]
    fn with_idle_timeout(mut self, idle_timeout: Duration) -> Self {
        self.idle_timeout = idle_timeout;
        self
    }

    /// Client for the short status/model-list calls; built on demand because every
    /// blocking client owns a background thread, which completions do not need.
    fn client(&self) -> Result<Client, RuntimeError> {
        Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|error| RuntimeError::Connection(error.to_string()))
    }

    fn map_request_error(error: reqwest::Error) -> RuntimeError {
        if error.is_timeout() {
            RuntimeError::Timeout(error.to_string())
        } else {
            RuntimeError::Connection(error.to_string())
        }
    }

    fn response_error(response: reqwest::blocking::Response) -> RuntimeError {
        let status = response.status().as_u16();
        let detail = response
            .text()
            .unwrap_or_else(|_| "empty error body".into());
        RuntimeError::Http {
            status,
            detail: summarize(&detail),
        }
    }

    fn fresh_models_url(&self) -> String {
        let cache_bust = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("{}/models?cache_bust={cache_bust}", self.endpoint)
    }

    fn api_models_url(&self) -> String {
        let base = self
            .endpoint
            .strip_suffix("/v1")
            .unwrap_or(&self.endpoint)
            .trim_end_matches('/');
        format!("{base}/api/v1/models")
    }
}

impl ModelRuntime for LmStudioRuntime {
    fn status(&self) -> Result<RuntimeStatus, RuntimeError> {
        match self
            .client()?
            .get(self.fresh_models_url())
            .header("Cache-Control", "no-cache, no-store")
            .header("Pragma", "no-cache")
            .send()
        {
            Ok(response) if response.status().is_success() => Ok(RuntimeStatus {
                available: true,
                endpoint: self.endpoint.clone(),
                detail: "LM Studio is reachable".into(),
            }),
            Ok(response) => Err(Self::response_error(response)),
            Err(error) => Err(Self::map_request_error(error)),
        }
    }

    fn list_models(&self) -> Result<Vec<LocalModel>, RuntimeError> {
        let response = self
            .client()?
            .get(self.api_models_url())
            .header("Cache-Control", "no-cache, no-store")
            .header("Pragma", "no-cache")
            .send()
            .map_err(Self::map_request_error)?;
        if response.status().is_success() {
            let payload: ApiModelsResponse = response
                .json()
                .map_err(|error| RuntimeError::MalformedResponse(error.to_string()))?;
            let models = payload
                .models
                .into_iter()
                .filter(|model| model.model_type == "llm")
                .flat_map(|model| {
                    if model.loaded_instances.is_empty() {
                        vec![LocalModel {
                            id: model.key.clone(),
                            owned_by: Some(model.publisher),
                            quantization: model.quantization.map(|value| value.name),
                        }]
                    } else {
                        model
                            .loaded_instances
                            .into_iter()
                            .map(|instance| LocalModel {
                                quantization: quantization_from_id(&instance.id).or_else(|| {
                                    model.quantization.as_ref().map(|value| value.name.clone())
                                }),
                                id: instance.id,
                                owned_by: Some(model.publisher.clone()),
                            })
                            .collect()
                    }
                })
                .collect();
            return Ok(models);
        }

        if response.status() != reqwest::StatusCode::NOT_FOUND {
            return Err(Self::response_error(response));
        }

        let response = self
            .client()?
            .get(self.fresh_models_url())
            .header("Cache-Control", "no-cache, no-store")
            .header("Pragma", "no-cache")
            .send()
            .map_err(Self::map_request_error)?;
        if !response.status().is_success() {
            return Err(Self::response_error(response));
        }
        let payload: ModelsResponse = response
            .json()
            .map_err(|error| RuntimeError::MalformedResponse(error.to_string()))?;
        Ok(payload
            .data
            .into_iter()
            .map(|model| {
                let quantization = quantization_from_id(&model.id);
                LocalModel {
                    id: model.id,
                    owned_by: model.owned_by,
                    quantization,
                }
            })
            .collect())
    }

    fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, RuntimeError> {
        let url = format!("{}/chat/completions", self.endpoint);
        let body = serde_json::to_vec(&ChatCompletionRequest {
            model: request.model,
            messages: request.messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: true,
            stream_options: StreamOptions {
                include_usage: true,
            },
        })
        .map_err(|error| RuntimeError::InvalidInput(error.to_string()))?;
        let idle = self.idle_timeout;
        trace_runtime("http_thread_spawn", "");
        // The stream is read on a dedicated thread with its own small runtime so the
        // call is safe from any context and each read can carry an idle deadline,
        // which the blocking reqwest client cannot express.
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| RuntimeError::Connection(error.to_string()))?
                .block_on(stream_completion(url, body, idle))
        })
        .join()
        .map_err(|_| RuntimeError::Connection("model request thread panicked".into()))?
    }
}

/// Sends a streaming chat request. The request fails with `Timeout` when the model
/// produces no bytes (headers or tokens) for `idle`, so a stalled server is detected
/// long before an overall deadline would fire.
async fn stream_completion(
    url: String,
    body: Vec<u8>,
    idle: Duration,
) -> Result<CompletionResponse, RuntimeError> {
    let started = Instant::now();
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(MAX_REQUEST_TIME)
        .build()
        .map_err(|error| RuntimeError::Connection(error.to_string()))?;
    trace_runtime("http_send", &format!("url={url} bytes={}", body.len()));
    let stalled = |stage: &str, chunks: usize| {
        trace_runtime(
            "http_stall",
            &format!(
                "stage={stage} chunks={chunks} idle_ms={} total_ms={}",
                idle.as_millis(),
                started.elapsed().as_millis()
            ),
        );
        RuntimeError::Timeout(format!(
            "no data from the model for {} s ({stage})",
            idle.as_secs()
        ))
    };
    let request = client
        .post(&url)
        // Each document block is independent; do not keep connections alive.
        .header("Connection", "close")
        .header("Content-Type", "application/json")
        .body(body);
    let mut response = match tokio::time::timeout(idle, request.send()).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => return Err(LmStudioRuntime::map_request_error(error)),
        Err(_) => return Err(stalled("waiting for response headers", 0)),
    };
    let status = response.status();
    trace_runtime(
        "http_headers",
        &format!(
            "status={} ms={}",
            status.as_u16(),
            started.elapsed().as_millis()
        ),
    );
    let is_stream = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/event-stream"));

    let mut parser = StreamParser::default();
    let mut raw = Vec::new();
    let mut chunks = 0_usize;
    loop {
        let chunk = match tokio::time::timeout(idle, response.chunk()).await {
            Ok(Ok(chunk)) => chunk,
            Ok(Err(error)) => return Err(LmStudioRuntime::map_request_error(error)),
            Err(_) => return Err(stalled("reading response body", chunks)),
        };
        let Some(chunk) = chunk else { break };
        if chunks == 0 {
            trace_runtime(
                "http_first_chunk",
                &format!("ms={}", started.elapsed().as_millis()),
            );
        }
        chunks += 1;
        if is_stream && status.is_success() {
            parser.push(&chunk)?;
            if parser.done {
                break;
            }
        } else if raw.len() < MAX_BODY_BYTES {
            raw.extend_from_slice(&chunk);
        }
    }
    trace_runtime(
        "http_end",
        &format!("chunks={chunks} total_ms={}", started.elapsed().as_millis()),
    );

    if !status.is_success() {
        return Err(RuntimeError::Http {
            status: status.as_u16(),
            detail: summarize(&String::from_utf8_lossy(&raw)),
        });
    }
    if is_stream {
        return parser.finish();
    }
    // A server that ignores `stream` answers with one ordinary JSON document.
    let payload: ChatCompletionResponse = serde_json::from_slice(&raw)
        .map_err(|error| RuntimeError::MalformedResponse(error.to_string()))?;
    let choice =
        payload.choices.into_iter().next().ok_or_else(|| {
            RuntimeError::MalformedResponse("response contained no choices".into())
        })?;
    Ok(CompletionResponse {
        model: payload.model,
        content: choice.message.content,
        completion_tokens: payload.usage.and_then(|usage| usage.completion_tokens),
    })
}

/// Incremental parser for OpenAI-style `text/event-stream` chat completions.
#[derive(Default)]
struct StreamParser {
    pending: Vec<u8>,
    model: String,
    content: String,
    completion_tokens: Option<u32>,
    deltas: u32,
    finished: bool,
    done: bool,
}

impl StreamParser {
    fn push(&mut self, bytes: &[u8]) -> Result<(), RuntimeError> {
        self.pending.extend_from_slice(bytes);
        while let Some(position) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=position).collect();
            self.line(String::from_utf8_lossy(&line).trim())?;
            if self.done {
                break;
            }
        }
        Ok(())
    }

    fn line(&mut self, line: &str) -> Result<(), RuntimeError> {
        let Some(data) = line.strip_prefix("data:") else {
            return Ok(());
        };
        let data = data.trim();
        if data == "[DONE]" {
            self.done = true;
            return Ok(());
        }
        let event: StreamEvent = serde_json::from_str(data)
            .map_err(|error| RuntimeError::MalformedResponse(error.to_string()))?;
        if let Some(error) = event.error {
            return Err(RuntimeError::MalformedResponse(summarize(
                &error.to_string(),
            )));
        }
        if let Some(model) = event.model {
            self.model = model;
        }
        if let Some(tokens) = event.usage.and_then(|usage| usage.completion_tokens) {
            self.completion_tokens = Some(tokens);
        }
        for choice in event.choices {
            if let Some(text) = choice.delta.and_then(|delta| delta.content) {
                if !text.is_empty() {
                    self.deltas += 1;
                    self.content.push_str(&text);
                }
            }
            if choice.finish_reason.is_some() {
                self.finished = true;
            }
        }
        Ok(())
    }

    fn finish(mut self) -> Result<CompletionResponse, RuntimeError> {
        if !self.pending.is_empty() {
            let rest = std::mem::take(&mut self.pending);
            self.line(String::from_utf8_lossy(&rest).trim())?;
        }
        if !self.done && !self.finished {
            return Err(RuntimeError::Connection(
                "model response stream ended before completion".into(),
            ));
        }
        Ok(CompletionResponse {
            model: self.model,
            content: self.content,
            // One streamed delta is one token for llama-server; used only when the
            // server does not report usage.
            completion_tokens: self.completion_tokens.or(Some(self.deltas)),
        })
    }
}

#[derive(Deserialize)]
struct StreamEvent {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<Usage>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}
#[derive(Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: Option<StreamDelta>,
    #[serde(default)]
    finish_reason: Option<String>,
}
#[derive(Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ModelResponse>,
}
#[derive(Deserialize)]
struct ApiModelsResponse {
    models: Vec<ApiModelResponse>,
}
#[derive(Deserialize)]
struct ApiModelResponse {
    #[serde(rename = "type")]
    model_type: String,
    publisher: String,
    key: String,
    #[serde(default)]
    quantization: Option<ApiQuantization>,
    #[serde(default)]
    loaded_instances: Vec<LoadedModelResponse>,
}
#[derive(Deserialize)]
struct LoadedModelResponse {
    id: String,
}
#[derive(Deserialize)]
struct ApiQuantization {
    name: String,
}
#[derive(Deserialize)]
struct ModelResponse {
    id: String,
    owned_by: Option<String>,
}
#[derive(serde::Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
    stream: bool,
    stream_options: StreamOptions,
}
#[derive(serde::Serialize)]
struct StreamOptions {
    include_usage: bool,
}
#[derive(Deserialize)]
struct ChatCompletionResponse {
    model: String,
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}
#[derive(Deserialize)]
struct Usage {
    completion_tokens: Option<u32>,
}
#[derive(Deserialize)]
struct Choice {
    message: ChatMessage,
}

fn summarize(value: &str) -> String {
    value.chars().take(500).collect()
}

fn quantization_from_id(id: &str) -> Option<String> {
    id.rsplit_once('@')
        .map(|(_, suffix)| suffix.trim())
        .filter(|suffix| !suffix.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn server(response: &'static str, status: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test server should bind");
        let address = format!("http://{}", listener.local_addr().unwrap());
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request should arrive");
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request);
            let body = response.as_bytes();
            let header = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        });
        address
    }

    #[test]
    fn lists_models_from_openai_compatible_response() {
        let endpoint = server(
            r#"{"models":[{"type":"llm","publisher":"local","key":"translategemma-4b-it","quantization":{"name":"Q8_0"},"loaded_instances":[{"id":"translategemma-4b-it@q8_0"}]}]}"#,
            "200 OK",
        );
        let models = LmStudioRuntime::new(&endpoint)
            .unwrap()
            .list_models()
            .unwrap();
        assert_eq!(models[0].id, "translategemma-4b-it@q8_0");
        assert_eq!(models[0].quantization.as_deref(), Some("q8_0"));
    }

    /// Manual repro: LINGVOLOC_REPRO_ENDPOINT=http://127.0.0.1:PORT/v1 cargo test repro -- --ignored --nocapture
    #[test]
    #[ignore]
    fn repro_sequential_completions() {
        let endpoint = std::env::var("LINGVOLOC_REPRO_ENDPOINT").expect("endpoint env var");
        for n in 1..=300 {
            let started = std::time::Instant::now();
            let result = LmStudioRuntime::new(&endpoint).unwrap().complete(CompletionRequest {
                model: "m".into(),
                messages: vec![ChatMessage {
                    role: "user".into(),
                    content: "Translate the following text from Russian (ru) to English (en). Return only the translation in the target language.\nСовершенно обескуражен".into(),
                }],
                temperature: 0.0,
                max_tokens: 2048,
            });
            let elapsed = started.elapsed();
            if result.is_err() || elapsed.as_secs() > 5 {
                println!("request {n}: {elapsed:?} {result:?}");
            }
            assert!(result.is_ok(), "request {n} failed");
        }
    }

    fn request() -> CompletionRequest {
        CompletionRequest {
            model: "m".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "hi".into(),
            }],
            temperature: 0.0,
            max_tokens: 16,
        }
    }

    /// Serves one request: writes `head`, then each part with `pause` between them,
    /// then keeps the connection open for `hold` before closing.
    fn scripted_server(head: &'static str, parts: Vec<&'static str>, hold: Duration) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test server should bind");
        let address = format!("http://{}/v1", listener.local_addr().unwrap());
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request should arrive");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer);
            if !head.is_empty() {
                stream.write_all(head.as_bytes()).unwrap();
            }
            for part in parts {
                stream.write_all(part.as_bytes()).unwrap();
                stream.flush().unwrap();
            }
            thread::sleep(hold);
        });
        address
    }

    const SSE_HEAD: &str =
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";

    #[test]
    fn assembles_streamed_completion_with_usage() {
        let endpoint = scripted_server(
            SSE_HEAD,
            vec![
                "data: {\"model\":\"m\",\"choices\":[{\"delta\":{\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":null}]}\n\n",
                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                "data: {\"choices\":[],\"usage\":{\"completion_tokens\":2}}\n\n",
                "data: [DONE]\n\n",
            ],
            Duration::from_millis(0),
        );
        let response = LmStudioRuntime::new(&endpoint)
            .unwrap()
            .complete(request())
            .unwrap();
        assert_eq!(response.content, "Hello");
        assert_eq!(response.model, "m");
        assert_eq!(response.completion_tokens, Some(2));
    }

    #[test]
    fn accepts_plain_json_from_servers_that_ignore_streaming() {
        let body = r#"{"model":"m","choices":[{"message":{"role":"assistant","content":"ok"}}],"usage":{"completion_tokens":1}}"#;
        let head: &'static str = Box::leak(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .into_boxed_str(),
        );
        let endpoint = scripted_server(head, vec![], Duration::from_millis(0));
        let response = LmStudioRuntime::new(&endpoint)
            .unwrap()
            .complete(request())
            .unwrap();
        assert_eq!(response.content, "ok");
    }

    #[test]
    fn stalled_stream_times_out_after_idle_period() {
        let endpoint = scripted_server(
            SSE_HEAD,
            vec!["data: {\"choices\":[{\"delta\":{\"content\":\"He\"}}]}\n\n"],
            Duration::from_secs(10),
        );
        let started = Instant::now();
        let error = LmStudioRuntime::new(&endpoint)
            .unwrap()
            .with_idle_timeout(Duration::from_millis(400))
            .complete(request())
            .unwrap_err();
        assert!(matches!(error, RuntimeError::Timeout(_)), "{error:?}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn server_that_accepts_but_never_answers_times_out() {
        let endpoint = scripted_server("", vec![], Duration::from_secs(10));
        let started = Instant::now();
        let error = LmStudioRuntime::new(&endpoint)
            .unwrap()
            .with_idle_timeout(Duration::from_millis(400))
            .complete(request())
            .unwrap_err();
        assert!(matches!(error, RuntimeError::Timeout(_)), "{error:?}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn stream_cut_before_completion_is_a_connection_error() {
        let endpoint = scripted_server(
            SSE_HEAD,
            vec!["data: {\"choices\":[{\"delta\":{\"content\":\"He\"}}]}\n\n"],
            Duration::from_millis(0),
        );
        let error = LmStudioRuntime::new(&endpoint)
            .unwrap()
            .complete(request())
            .unwrap_err();
        assert!(matches!(error, RuntimeError::Connection(_)), "{error:?}");
    }

    #[test]
    fn leaves_quantization_unknown_when_model_id_has_no_suffix() {
        assert_eq!(quantization_from_id("translategemma-12b-it"), None);
    }

    #[test]
    fn maps_http_errors_without_user_text() {
        let endpoint = server(
            r#"{"error":"model unavailable"}"#,
            "503 Service Unavailable",
        );
        let error = LmStudioRuntime::new(&endpoint)
            .unwrap()
            .list_models()
            .unwrap_err();
        assert_eq!(
            error,
            RuntimeError::Http {
                status: 503,
                detail: r#"{"error":"model unavailable"}"#.into()
            }
        );
    }
}
