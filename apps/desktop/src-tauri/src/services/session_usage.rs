use crate::domain::{ProviderId, TranslationResult};
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionUsageEntry {
    pub provider_id: ProviderId,
    pub model_id: String,
    pub requests: u64,
    pub failed_requests: u64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub billed_characters: Option<u64>,
}

#[derive(Debug, Default)]
pub struct SessionUsageTracker {
    entries: Mutex<HashMap<(ProviderId, String), SessionUsageEntry>>,
}

impl SessionUsageTracker {
    pub fn record_success(&self, result: &TranslationResult) {
        let provider_id = result.provider_id.unwrap_or(ProviderId::LlamaCpp);
        let key = (provider_id, result.model_id.clone());
        if let Ok(mut entries) = self.entries.lock() {
            let entry = entries.entry(key).or_insert_with(|| SessionUsageEntry {
                provider_id,
                model_id: result.model_id.clone(),
                requests: 0,
                failed_requests: 0,
                input_tokens: None,
                output_tokens: None,
                total_tokens: None,
                billed_characters: None,
            });
            entry.requests += 1;
            add_optional(&mut entry.input_tokens, result.prompt_tokens);
            add_optional(&mut entry.output_tokens, result.completion_tokens);
            add_optional(&mut entry.total_tokens, result.total_tokens);
            add_optional(&mut entry.billed_characters, result.billed_characters);
        }
    }

    #[allow(dead_code)]
    pub fn record_failure(&self, provider_id: ProviderId, model_id: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            let entry = entries
                .entry((provider_id, model_id.into()))
                .or_insert_with(|| SessionUsageEntry {
                    provider_id,
                    model_id: model_id.into(),
                    requests: 0,
                    failed_requests: 0,
                    input_tokens: None,
                    output_tokens: None,
                    total_tokens: None,
                    billed_characters: None,
                });
            entry.failed_requests += 1;
        }
    }

    pub fn list(&self) -> Vec<SessionUsageEntry> {
        let Ok(entries) = self.entries.lock() else {
            return Vec::new();
        };
        let mut values: Vec<_> = entries.values().cloned().collect();
        values.sort_by(|left, right| {
            left.provider_id
                .service_name()
                .cmp(right.provider_id.service_name())
                .then(left.model_id.cmp(&right.model_id))
        });
        values
    }
}

fn add_optional(total: &mut Option<u64>, value: Option<u64>) {
    if let Some(value) = value {
        *total = Some(total.unwrap_or(0) + value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_tokens_and_deepl_characters_without_persisting_state() {
        let tracker = SessionUsageTracker::default();
        tracker.record_success(&TranslationResult {
            text: "x".into(),
            model_id: "gpt".into(),
            adapter_id: String::new(),
            latency_ms: 1,
            prompt_tokens: Some(2),
            completion_tokens: Some(3),
            total_tokens: Some(5),
            provider_id: Some(ProviderId::OpenAi),
            billed_characters: None,
        });
        tracker.record_success(&TranslationResult {
            text: "x".into(),
            model_id: "gpt".into(),
            adapter_id: String::new(),
            latency_ms: 1,
            prompt_tokens: Some(4),
            completion_tokens: Some(5),
            total_tokens: Some(9),
            provider_id: Some(ProviderId::OpenAi),
            billed_characters: None,
        });
        tracker.record_success(&TranslationResult {
            text: "x".into(),
            model_id: "n/a".into(),
            adapter_id: String::new(),
            latency_ms: 1,
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
            provider_id: Some(ProviderId::DeepL),
            billed_characters: Some(12),
        });
        tracker.record_failure(ProviderId::OpenAi, "gpt");
        let rows = tracker.list();
        let openai = rows
            .iter()
            .find(|row| row.provider_id == ProviderId::OpenAi)
            .unwrap();
        assert_eq!(
            (openai.requests, openai.failed_requests, openai.total_tokens),
            (2, 1, Some(14))
        );
        assert_eq!(
            rows.iter()
                .find(|row| row.provider_id == ProviderId::DeepL)
                .unwrap()
                .billed_characters,
            Some(12)
        );
    }
}
