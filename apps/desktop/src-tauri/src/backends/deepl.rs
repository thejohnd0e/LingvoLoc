use super::http::HttpTransport;
use super::TranslationBackend;
use crate::domain::{
    BackendCapabilities, LocalModel, ProviderId, RuntimeError, RuntimeStatus, TranslationRequest,
    TranslationResult, TranslationStyle,
};
use crate::services::request_control::RequestCancellation;
use reqwest::blocking::Response;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use std::time::Instant;

const FREE_BASE: &str = "https://api-free.deepl.com";
const PRO_BASE: &str = "https://api.deepl.com";
const MAX_TEXT_BYTES: usize = 128 * 1024;

pub struct DeepLBackend {
    transport: HttpTransport,
    base_url: String,
    api_key: String,
}

impl DeepLBackend {
    pub fn new(plan: &str, api_key: String) -> Result<Self, RuntimeError> {
        Self::with_base_url(plan, api_key, HttpTransport::new()?)
    }
    fn with_base_url(
        plan: &str,
        api_key: String,
        transport: HttpTransport,
    ) -> Result<Self, RuntimeError> {
        let base_url = match plan {
            "pro" => PRO_BASE,
            "free" | "" => FREE_BASE,
            _ => {
                return Err(RuntimeError::InvalidInput(
                    "DeepL plan must be free or pro".into(),
                ))
            }
        };
        Ok(Self {
            transport,
            base_url: base_url.into(),
            api_key,
        })
    }
    fn headers(&self) -> Result<HeaderMap, RuntimeError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("DeepL-Auth-Key {}", self.api_key))
                .map_err(|_| RuntimeError::InvalidInput("invalid provider credential".into()))?,
        );
        Ok(headers)
    }
    fn body(&self, request: &TranslationRequest) -> Result<Value, RuntimeError> {
        if request.translation_style != TranslationStyle::Neutral {
            return Err(RuntimeError::InvalidInput(
                "DeepL supports only the neutral translation style".into(),
            ));
        }
        if request.text.len() > MAX_TEXT_BYTES {
            return Err(RuntimeError::InvalidInput(
                "DeepL text must be at most 128 KiB".into(),
            ));
        }
        let mut body = json!({"text":[request.text],"target_lang":request.target_language.to_uppercase(),"show_billed_characters":true});
        if request.source_language != "auto" {
            body["source_lang"] = Value::String(request.source_language.to_uppercase());
        }
        Ok(body)
    }

    pub fn list_languages(&self, language_type: &str) -> Result<Vec<String>, RuntimeError> {
        if !matches!(language_type, "source" | "target") {
            return Err(RuntimeError::InvalidInput(
                "DeepL language type must be source or target".into(),
            ));
        }
        parse_languages(self.transport.get(&format!(
            "{}/v2/languages?type={language_type}",
            self.base_url
        ))?)
    }
}

impl TranslationBackend for DeepLBackend {
    fn provider_id(&self) -> ProviderId {
        ProviderId::DeepL
    }
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            model_list: false,
            custom_model_id: false,
            token_usage: false,
            billed_characters: true,
            translation_styles: false,
        }
    }
    fn status(&self) -> Result<RuntimeStatus, RuntimeError> {
        Ok(RuntimeStatus {
            available: true,
            endpoint: self.base_url.clone(),
            detail: "DeepL endpoint configured".into(),
        })
    }
    fn list_models(&self) -> Result<Vec<LocalModel>, RuntimeError> {
        Ok(Vec::new())
    }
    fn translate(
        &self,
        request: &TranslationRequest,
        cancellation: &RequestCancellation,
    ) -> Result<TranslationResult, RuntimeError> {
        if cancellation.is_cancelled() {
            return Err(RuntimeError::Cancelled);
        }
        let started = Instant::now();
        let response = self.transport.post_json(
            &format!("{}/v2/translate", self.base_url),
            self.headers()?,
            &self.body(request)?,
        )?;
        let value: Value = response.json().map_err(|_| {
            RuntimeError::MalformedResponse("DeepL response is not valid JSON".into())
        })?;
        let translation = value
            .pointer("/translations/0/text")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                RuntimeError::MalformedResponse("DeepL response has no translation".into())
            })?;
        let billed = value.get("billed_characters").and_then(Value::as_u64);
        Ok(TranslationResult {
            text: translation.into(),
            model_id: request.model_id.clone(),
            adapter_id: "deepl-translate".into(),
            latency_ms: started.elapsed().as_millis(),
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
            provider_id: Some(ProviderId::DeepL),
            billed_characters: billed,
        })
    }
}

pub(crate) fn parse_languages(response: Response) -> Result<Vec<String>, RuntimeError> {
    let values: Vec<Value> = response.json().map_err(|_| {
        RuntimeError::MalformedResponse("DeepL languages response is not valid JSON".into())
    })?;
    Ok(values
        .into_iter()
        .filter_map(|value| {
            value
                .get("language")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selects_free_and_pro_endpoints_and_neutral_only() {
        let free =
            DeepLBackend::with_base_url("free", "secret".into(), HttpTransport::new().unwrap())
                .unwrap();
        let pro =
            DeepLBackend::with_base_url("pro", "secret".into(), HttpTransport::new().unwrap())
                .unwrap();
        assert_eq!(free.base_url, FREE_BASE);
        assert_eq!(pro.base_url, PRO_BASE);
        let request = TranslationRequest {
            model_id: String::new(),
            adapter_id: String::new(),
            source_language: "auto".into(),
            target_language: "ru".into(),
            text: "Hello".into(),
            translation_style: TranslationStyle::Literary,
        };
        assert!(matches!(
            free.body(&request),
            Err(RuntimeError::InvalidInput(_))
        ));
    }
    #[test]
    fn omits_auto_source_and_maps_target_and_size_guard() {
        let backend =
            DeepLBackend::with_base_url("free", "secret".into(), HttpTransport::new().unwrap())
                .unwrap();
        let request = TranslationRequest {
            model_id: String::new(),
            adapter_id: String::new(),
            source_language: "auto".into(),
            target_language: "ru".into(),
            text: "Hello".into(),
            translation_style: TranslationStyle::Neutral,
        };
        let body = backend.body(&request).unwrap();
        assert!(body.get("source_lang").is_none());
        assert_eq!(body["target_lang"], "RU");
        let mut oversized = request;
        oversized.text = "x".repeat(MAX_TEXT_BYTES + 1);
        assert!(matches!(
            backend.body(&oversized),
            Err(RuntimeError::InvalidInput(_))
        ));
    }
}
