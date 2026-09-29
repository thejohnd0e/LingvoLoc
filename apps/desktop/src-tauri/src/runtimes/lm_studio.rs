use crate::domain::{
    ChatMessage, CompletionRequest, CompletionResponse, LocalModel, ModelRuntime, RuntimeError,
    RuntimeStatus,
};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct LmStudioRuntime {
    endpoint: String,
    client: Client,
}

impl LmStudioRuntime {
    pub fn new(endpoint: &str) -> Result<Self, RuntimeError> {
        let endpoint = endpoint.trim_end_matches('/').to_string();
        if endpoint.is_empty() {
            return Err(RuntimeError::InvalidInput(
                "endpoint must not be empty".into(),
            ));
        }
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|error| RuntimeError::Connection(error.to_string()))?;
        Ok(Self { endpoint, client })
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
            .client
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
            .client
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
            .client
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
        let response = self
            .client
            .post(format!("{}/chat/completions", self.endpoint))
            .json(&ChatCompletionRequest {
                model: request.model,
                messages: request.messages,
                temperature: request.temperature,
                max_tokens: request.max_tokens,
                stream: false,
            })
            .send()
            .map_err(Self::map_request_error)?;
        if !response.status().is_success() {
            return Err(Self::response_error(response));
        }
        let payload: ChatCompletionResponse = response
            .json()
            .map_err(|error| RuntimeError::MalformedResponse(error.to_string()))?;
        let choice = payload.choices.into_iter().next().ok_or_else(|| {
            RuntimeError::MalformedResponse("response contained no choices".into())
        })?;
        Ok(CompletionResponse {
            model: payload.model,
            content: choice.message.content,
            completion_tokens: payload.usage.and_then(|usage| usage.completion_tokens),
        })
    }
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
