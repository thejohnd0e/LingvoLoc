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
    #[serde(default)]
    pub provider_id: Option<ProviderId>,
    #[serde(default)]
    pub billed_characters: Option<u64>,
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
    OpenAi,
    Anthropic,
    Gemini,
    DeepL,
    OpenAiCompatible,
    DeepSeek,
    OpenRouter,
    Xai,
    ChatGpt,
    SuperGrok,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum ProviderId {
    LlamaCpp,
    LmStudio,
    OpenAi,
    Anthropic,
    Gemini,
    DeepL,
    OpenAiCompatible,
    DeepSeek,
    OpenRouter,
    Xai,
    ChatGpt,
    SuperGrok,
}

impl ProviderId {
    pub fn service_name(self) -> &'static str {
        match self {
            Self::LlamaCpp => "com.lingoloc.desktop.ai.llamacpp",
            Self::LmStudio => "com.lingoloc.desktop.ai.lmstudio",
            Self::OpenAi => "com.lingoloc.desktop.ai.openai",
            Self::Anthropic => "com.lingoloc.desktop.ai.anthropic",
            Self::Gemini => "com.lingoloc.desktop.ai.gemini",
            Self::DeepL => "com.lingoloc.desktop.ai.deepl",
            Self::OpenAiCompatible => "com.lingoloc.desktop.ai.openai-compatible",
            Self::DeepSeek => "com.lingoloc.desktop.ai.deepseek",
            Self::OpenRouter => "com.lingoloc.desktop.ai.openrouter",
            Self::Xai => "com.lingoloc.desktop.ai.xai",
            Self::ChatGpt => "com.lingoloc.desktop.ai.chatgpt",
            Self::SuperGrok => "com.lingoloc.desktop.ai.supergrok",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendCapabilities {
    pub model_list: bool,
    pub custom_model_id: bool,
    pub token_usage: bool,
    pub billed_characters: bool,
    pub translation_styles: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CloudModelConfig {
    pub model_id: String,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub available_models: Vec<LocalModel>,
    #[serde(default)]
    pub models_refreshed_at: Option<u64>,
    /// Optional `http://`, `https://` or `socks5://` proxy for this provider.
    #[serde(default)]
    pub proxy_url: String,
}

impl CloudModelConfig {
    fn new(model_id: &str) -> Self {
        Self {
            model_id: model_id.into(),
            endpoint: String::new(),
            available_models: Vec::new(),
            models_refreshed_at: None,
            proxy_url: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeepLSettings {
    #[serde(default)]
    pub plan: String,
    #[serde(default)]
    pub available_languages: Vec<String>,
    #[serde(default)]
    pub languages_refreshed_at: Option<u64>,
    #[serde(default)]
    pub proxy_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CloudSettings {
    #[serde(default)]
    pub consent_accepted: bool,
    pub open_ai: CloudModelConfig,
    pub anthropic: CloudModelConfig,
    pub gemini: CloudModelConfig,
    pub deep_l: DeepLSettings,
    pub open_ai_compatible: CloudModelConfig,
    #[serde(default = "default_deep_seek")]
    pub deep_seek: CloudModelConfig,
    #[serde(default = "default_open_router")]
    pub open_router: CloudModelConfig,
    #[serde(default = "default_xai")]
    pub xai: CloudModelConfig,
    #[serde(default = "default_chat_gpt")]
    pub chat_gpt: CloudModelConfig,
    #[serde(default = "default_super_grok")]
    pub super_grok: CloudModelConfig,
}

fn default_deep_seek() -> CloudModelConfig {
    CloudModelConfig::new("deepseek-chat")
}

fn default_open_router() -> CloudModelConfig {
    CloudModelConfig::new("")
}

fn default_xai() -> CloudModelConfig {
    CloudModelConfig::new("")
}

fn default_chat_gpt() -> CloudModelConfig {
    CloudModelConfig::new("")
}

fn default_super_grok() -> CloudModelConfig {
    CloudModelConfig::new("")
}

impl Default for CloudSettings {
    fn default() -> Self {
        Self {
            consent_accepted: false,
            open_ai: CloudModelConfig::new("gpt-4o-mini"),
            anthropic: CloudModelConfig::new("claude-3-5-haiku-latest"),
            gemini: CloudModelConfig::new("gemini-2.0-flash"),
            deep_l: DeepLSettings {
                plan: "free".into(),
                available_languages: Vec::new(),
                languages_refreshed_at: None,
                proxy_url: String::new(),
            },
            open_ai_compatible: CloudModelConfig::new(""),
            deep_seek: default_deep_seek(),
            open_router: default_open_router(),
            xai: default_xai(),
            chat_gpt: default_chat_gpt(),
            super_grok: default_super_grok(),
        }
    }
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
    #[serde(default)]
    pub cloud: CloudSettings,
}

impl Settings {
    pub fn active_model_id(&self) -> &str {
        match self.runtime_mode {
            RuntimeMode::OpenAi => self.cloud.open_ai.model_id.as_str(),
            RuntimeMode::Anthropic => self.cloud.anthropic.model_id.as_str(),
            RuntimeMode::Gemini => self.cloud.gemini.model_id.as_str(),
            RuntimeMode::DeepL => "deepL",
            RuntimeMode::OpenAiCompatible => self.cloud.open_ai_compatible.model_id.as_str(),
            RuntimeMode::DeepSeek => self.cloud.deep_seek.model_id.as_str(),
            RuntimeMode::OpenRouter => self.cloud.open_router.model_id.as_str(),
            RuntimeMode::Xai => self.cloud.xai.model_id.as_str(),
            RuntimeMode::ChatGpt => self.cloud.chat_gpt.model_id.as_str(),
            RuntimeMode::SuperGrok => self.cloud.super_grok.model_id.as_str(),
            RuntimeMode::LmStudio | RuntimeMode::Standalone => self.model_id.as_str(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuntimeError {
    InvalidInput(String),
    Connection(String),
    Timeout(String),
    Http {
        status: u16,
        detail: String,
    },
    MalformedResponse(String),
    UnsupportedAdapter(String),
    Authentication(String),
    Quota(String),
    RateLimited {
        detail: String,
        retry_after: Option<std::time::Duration>,
    },
    ContentRejected(String),
    Cancelled,
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
            Self::Authentication(detail) => {
                write!(formatter, "provider authentication failed: {detail}")
            }
            Self::Quota(detail) => write!(formatter, "provider quota exceeded: {detail}"),
            Self::RateLimited { detail, .. } => {
                write!(formatter, "provider rate limited the request: {detail}")
            }
            Self::ContentRejected(detail) => {
                write!(formatter, "provider rejected the content: {detail}")
            }
            Self::Cancelled => write!(formatter, "translation request cancelled"),
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
    use super::{ProviderId, RuntimeMode, Settings, TranslationStyle};

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
    fn old_settings_get_default_cloud_configuration() {
        let settings: Settings = serde_json::from_str(
            r#"{
                "endpoint": "http://127.0.0.1:1234/v1",
                "modelId": "a.gguf",
                "adapterId": "translategemma",
                "sourceLanguage": "auto",
                "targetLanguage": "ru",
                "primaryLanguage": "en",
                "secondaryLanguage": "ru"
            }"#,
        )
        .expect("old settings should deserialize with cloud defaults");

        assert_eq!(settings.runtime_mode, RuntimeMode::Standalone);
        assert!(!settings.cloud.consent_accepted);
        assert_eq!(settings.cloud.open_ai.model_id, "gpt-4o-mini");
    }

    #[test]
    fn provider_ids_round_trip_in_camel_case() {
        for (json, expected) in [
            ("llamaCpp", ProviderId::LlamaCpp),
            ("lmStudio", ProviderId::LmStudio),
            ("openAi", ProviderId::OpenAi),
            ("anthropic", ProviderId::Anthropic),
            ("gemini", ProviderId::Gemini),
            ("deepL", ProviderId::DeepL),
            ("openAiCompatible", ProviderId::OpenAiCompatible),
            ("deepSeek", ProviderId::DeepSeek),
            ("openRouter", ProviderId::OpenRouter),
            ("xai", ProviderId::Xai),
            ("chatGpt", ProviderId::ChatGpt),
            ("superGrok", ProviderId::SuperGrok),
        ] {
            assert_eq!(
                serde_json::from_str::<ProviderId>(&format!("\"{json}\"")).unwrap(),
                expected
            );
        }
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
