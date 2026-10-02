use crate::domain::ProviderId;
use std::fmt;
use std::sync::{Arc, Mutex};

const KEYRING_USERNAME: &str = "api-key";

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    pub configured: bool,
    pub hint: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum CredentialError {
    InvalidInput,
    Missing,
    Backend(String),
}

impl fmt::Debug for CredentialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput => formatter.write_str("InvalidInput"),
            Self::Missing => formatter.write_str("Missing"),
            Self::Backend(_) => formatter.write_str("Backend(redacted)"),
        }
    }
}

impl fmt::Display for CredentialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput => formatter.write_str("credential must not be empty"),
            Self::Missing => formatter.write_str("credential is not configured"),
            Self::Backend(_) => formatter.write_str("credential store operation failed"),
        }
    }
}

impl std::error::Error for CredentialError {}

pub trait CredentialBackend: Send + Sync {
    fn set(&self, provider: ProviderId, secret: &str) -> Result<(), CredentialError>;
    fn get(&self, provider: ProviderId) -> Result<String, CredentialError>;
    fn delete(&self, provider: ProviderId) -> Result<(), CredentialError>;
}

pub struct CredentialStore {
    backend: Mutex<Arc<dyn CredentialBackend>>,
}

impl CredentialStore {
    pub(crate) fn windows() -> Self {
        Self::with_backend(Arc::new(WindowsCredentialBackend))
    }

    fn with_backend(backend: Arc<dyn CredentialBackend>) -> Self {
        Self {
            backend: Mutex::new(backend),
        }
    }

    pub fn status(&self, provider: ProviderId) -> Result<CredentialStatus, CredentialError> {
        let backend = self.backend.lock().map_err(|_| lock_error())?;
        match backend.get(provider) {
            Ok(secret) => Ok(CredentialStatus {
                configured: true,
                hint: Some(mask_hint(&secret)),
            }),
            Err(CredentialError::Missing) => Ok(CredentialStatus {
                configured: false,
                hint: None,
            }),
            Err(error) => Err(error),
        }
    }

    pub fn save(
        &self,
        provider: ProviderId,
        secret: &str,
    ) -> Result<CredentialStatus, CredentialError> {
        if secret.trim().is_empty() {
            return Err(CredentialError::InvalidInput);
        }
        let backend = self.backend.lock().map_err(|_| lock_error())?;
        backend.set(provider, secret)?;
        Ok(CredentialStatus {
            configured: true,
            hint: Some(mask_hint(secret)),
        })
    }

    pub(crate) fn get(&self, provider: ProviderId) -> Result<String, CredentialError> {
        let backend = self.backend.lock().map_err(|_| lock_error())?;
        backend.get(provider)
    }

    pub fn delete(&self, provider: ProviderId) -> Result<(), CredentialError> {
        let backend = self.backend.lock().map_err(|_| lock_error())?;
        match backend.delete(provider) {
            Ok(()) | Err(CredentialError::Missing) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

pub fn map_error(error: CredentialError) -> crate::domain::RuntimeError {
    match error {
        CredentialError::InvalidInput => crate::domain::RuntimeError::InvalidInput(
            "provider credential must not be empty".into(),
        ),
        CredentialError::Missing => crate::domain::RuntimeError::Authentication(
            "provider credential is not configured".into(),
        ),
        CredentialError::Backend(_) => crate::domain::RuntimeError::Connection(
            "Windows Credential Manager is unavailable".into(),
        ),
    }
}

fn mask_hint(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    if chars.len() <= 8 {
        return "****".into();
    }
    let first: String = chars.iter().take(4).collect();
    let last: String = chars.iter().rev().take(4).rev().collect();
    format!("{first}…{last}")
}

fn lock_error() -> CredentialError {
    CredentialError::Backend("credential store lock poisoned".into())
}

struct WindowsCredentialBackend;

impl CredentialBackend for WindowsCredentialBackend {
    fn set(&self, provider: ProviderId, secret: &str) -> Result<(), CredentialError> {
        keyring::Entry::new(provider.service_name(), KEYRING_USERNAME)
            .map_err(|_| CredentialError::Backend("entry creation failed".into()))?
            .set_password(secret)
            .map_err(|_| CredentialError::Backend("credential write failed".into()))
    }

    fn get(&self, provider: ProviderId) -> Result<String, CredentialError> {
        match keyring::Entry::new(provider.service_name(), KEYRING_USERNAME)
            .map_err(|_| CredentialError::Backend("entry creation failed".into()))?
            .get_password()
        {
            Ok(secret) => Ok(secret),
            Err(keyring::Error::NoEntry) => Err(CredentialError::Missing),
            Err(_) => Err(CredentialError::Backend("credential read failed".into())),
        }
    }

    fn delete(&self, provider: ProviderId) -> Result<(), CredentialError> {
        match keyring::Entry::new(provider.service_name(), KEYRING_USERNAME)
            .map_err(|_| CredentialError::Backend("entry creation failed".into()))?
            .delete_credential()
        {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(CredentialError::Backend("credential delete failed".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ProviderId;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeStore {
        values: Mutex<HashMap<String, String>>,
    }

    impl CredentialBackend for FakeStore {
        fn set(&self, provider: ProviderId, secret: &str) -> Result<(), CredentialError> {
            self.values
                .lock()
                .unwrap()
                .insert(provider.service_name().into(), secret.into());
            Ok(())
        }

        fn get(&self, provider: ProviderId) -> Result<String, CredentialError> {
            self.values
                .lock()
                .unwrap()
                .get(provider.service_name())
                .cloned()
                .ok_or(CredentialError::Missing)
        }

        fn delete(&self, provider: ProviderId) -> Result<(), CredentialError> {
            self.values.lock().unwrap().remove(provider.service_name());
            Ok(())
        }
    }

    fn store() -> CredentialStore {
        CredentialStore::with_backend(Arc::new(FakeStore::default()))
    }

    #[test]
    fn missing_key_reports_unconfigured_without_a_hint() {
        let result = store().status(ProviderId::OpenAi).unwrap();
        assert_eq!(
            result,
            CredentialStatus {
                configured: false,
                hint: None
            }
        );
    }

    #[test]
    fn saved_key_reports_only_a_masked_hint() {
        let store = store();
        let result = store
            .save(ProviderId::OpenAi, "sk-1234567890-secret")
            .unwrap();
        assert_eq!(result.hint.as_deref(), Some("sk-1…cret"));
        assert!(!format!("{result:?}").contains("1234567890"));
    }

    #[test]
    fn replacement_and_delete_are_serialized() {
        let store = store();
        store.save(ProviderId::Anthropic, "old-secret").unwrap();
        store.save(ProviderId::Anthropic, "new-secret").unwrap();
        assert_eq!(store.get(ProviderId::Anthropic).unwrap(), "new-secret");
        store.delete(ProviderId::Anthropic).unwrap();
        assert_eq!(
            store.status(ProviderId::Anthropic).unwrap().configured,
            false
        );
    }

    #[test]
    fn empty_keys_are_rejected_before_backend_access() {
        let error = store().save(ProviderId::Gemini, "  ").unwrap_err();
        assert_eq!(error, CredentialError::InvalidInput);
    }

    #[test]
    fn errors_never_display_the_secret() {
        let error = CredentialError::Backend("backend rejected super-secret".into());
        assert!(!error.to_string().contains("super-secret"));
        assert!(!format!("{error:?}").contains("super-secret"));
    }
}
