use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalModel {
    pub id: String,
    pub owned_by: Option<String>,
    pub quantization: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeStatus {
    pub available: bool,
    pub endpoint: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranslationRequest {
    pub model_id: String,
    pub adapter_id: String,
    pub source_language: String,
    pub target_language: String,
    pub text: String,
    #[serde(default)]
    pub translation_style: TranslationStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranslationResult {
    pub text: String,
    pub model_id: String,
    pub adapter_id: String,
    pub latency_ms: u128,
    #[serde(default)]
    pub prompt_tokens: Option<u64>,
    #[serde(default)]
    pub completion_tokens: Option<u64>,
    #[serde(default)]
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DetectedLanguage {
    pub code: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeMode {
    LmStudio,
    #[default]
    Standalone,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TranslationStyle {
    #[default]
    Neutral,
    Literary,
    Technical,
    Conversational,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub runtime_mode: RuntimeMode,
    #[serde(default)]
    pub models_directory: String,
    #[serde(default)]
    pub llama_server_path: String,
    pub endpoint: String,
    pub model_id: String,
    pub adapter_id: String,
    pub source_language: String,
    pub target_language: String,
    pub primary_language: String,
    pub secondary_language: String,
    #[serde(default)]
    pub translation_style: TranslationStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuntimeError {
    InvalidInput(String),
    Connection(String),
    Timeout(String),
    Http { status: u16, detail: String },
    MalformedResponse(String),
    UnsupportedAdapter(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(detail) => write!(formatter, "invalid input: {detail}"),
            Self::Connection(detail) => write!(formatter, "runtime connection failed: {detail}"),
            Self::Timeout(detail) => write!(formatter, "runtime request timed out: {detail}"),
            Self::Http { status, detail } => {
                write!(formatter, "runtime returned HTTP {status}: {detail}")
            }
            Self::MalformedResponse(detail) => {
                write!(formatter, "malformed runtime response: {detail}")
            }
            Self::UnsupportedAdapter(adapter) => {
                write!(formatter, "unsupported adapter: {adapter}")
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

pub trait ModelRuntime {
    fn status(&self) -> Result<RuntimeStatus, RuntimeError>;
    fn list_models(&self) -> Result<Vec<LocalModel>, RuntimeError>;
    fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, RuntimeError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompletionResponse {
    pub model: String,
    pub content: String,
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

pub trait TranslationModelAdapter {
    fn id(&self) -> &'static str;
    fn build_request(
        &self,
        request: &TranslationRequest,
    ) -> Result<CompletionRequest, RuntimeError>;
    fn parse_response(&self, response: CompletionResponse) -> Result<String, RuntimeError>;
}

#[cfg(test)]
mod tests {
    use super::{RuntimeMode, Settings, TranslationStyle};

    #[test]
    fn settings_use_frontend_camel_case_contract() {
        let settings: Settings = serde_json::from_str(
            r#"{
                "endpoint": "http://127.0.0.1:1234/v1",
                "modelId": "translategemma-4b-it@q8_0",
                "adapterId": "translategemma",
                "sourceLanguage": "auto",
                "targetLanguage": "ru",
                "primaryLanguage": "en",
                "secondaryLanguage": "ru"
            }"#,
        )
        .expect("frontend settings should deserialize");
        assert_eq!(settings.model_id, "translategemma-4b-it@q8_0");
        assert_eq!(settings.secondary_language, "ru");
        assert_eq!(settings.runtime_mode, RuntimeMode::Standalone);
        assert_eq!(settings.translation_style, TranslationStyle::Neutral);
    }

    #[test]
    fn settings_accept_standalone_mode() {
        let settings: Settings = serde_json::from_str(
            r#"{
                "runtimeMode": "standalone",
                "modelsDirectory": "D:/models",
                "llamaServerPath": "D:/llama/llama-server.exe",
                "endpoint": "http://127.0.0.1:1234/v1",
                "modelId": "a.gguf",
                "adapterId": "translategemma",
                "sourceLanguage": "auto",
                "targetLanguage": "ru",
                "primaryLanguage": "en",
                "secondaryLanguage": "ru"
            }"#,
        )
        .expect("standalone settings should deserialize");
        assert_eq!(settings.runtime_mode, RuntimeMode::Standalone);
    }

    #[test]
    fn translation_styles_round_trip_in_camel_case() {
        for (json, expected) in [
            ("neutral", TranslationStyle::Neutral),
            ("literary", TranslationStyle::Literary),
            ("technical", TranslationStyle::Technical),
            ("conversational", TranslationStyle::Conversational),
        ] {
            let value = serde_json::to_string(&expected).expect("style should serialize");
            assert_eq!(value, format!("\"{json}\""));
            assert_eq!(
                serde_json::from_str::<TranslationStyle>(&value).unwrap(),
                expected
            );
        }
    }
}
