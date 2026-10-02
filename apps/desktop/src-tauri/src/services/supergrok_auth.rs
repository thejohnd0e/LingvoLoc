//! EXPERIMENTAL: SuperGrok / X Premium+ subscription sign-in for xAI.
//!
//! xAI does not document subscription OAuth for third-party apps. This follows what other
//! open-source agents (OpenCode, Hermes, OpenClaw) do: OAuth 2.0 device code flow
//! (RFC 8628) against `auth.x.ai` as a public client, using xAI's shared Grok client id,
//! then the access token as a bearer token for `api.x.ai/v1`. xAI decides which accounts
//! receive tokens, so a subscriber can still be refused (HTTP 403).
//!
//! Only `{refresh_token, email}` is stored in Windows Credential Manager; access tokens
//! live in memory and are refreshed on demand.

use crate::backends::http::{oauth_error_code, HttpTransport};
use crate::domain::{ProviderId, RuntimeError};
use crate::services::credentials::{map_error, CredentialError, CredentialStatus, CredentialStore};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const ISSUER: &str = "https://auth.x.ai";
const CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const SCOPES: &str = "openid profile email offline_access grok-cli:access api:access";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const REFRESH_MARGIN: Duration = Duration::from_secs(120);
const MIN_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone, Serialize, Deserialize)]
struct Session {
    refresh_token: String,
    email: String,
}

#[derive(Clone)]
struct Endpoints {
    device: String,
    token: String,
    revocation: Option<String>,
}

struct Access {
    token: String,
    expires_at: Instant,
}

struct Pending {
    device_code: String,
    interval: Duration,
    expires_at: Instant,
}

/// What the user must do in the browser to approve the sign-in.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_url: String,
    pub expires_in: u64,
}

static ACCESS: Mutex<Option<Access>> = Mutex::new(None);
static ENDPOINTS: Mutex<Option<Endpoints>> = Mutex::new(None);
static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
static REFRESH_LOCK: Mutex<()> = Mutex::new(());
static CANCEL: AtomicBool = AtomicBool::new(false);

fn transport(proxy: &str) -> Result<HttpTransport, RuntimeError> {
    HttpTransport::new()?.with_proxy(proxy)
}

fn endpoints(transport: &HttpTransport) -> Result<Endpoints, RuntimeError> {
    if let Some(cached) = ENDPOINTS.lock().ok().and_then(|guard| guard.clone()) {
        return Ok(cached);
    }
    let value: Value = transport
        .get(&format!("{ISSUER}/.well-known/openid-configuration"))?
        .json()
        .map_err(|_| {
            RuntimeError::MalformedResponse("xAI OpenID configuration is not valid JSON".into())
        })?;
    let field = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                RuntimeError::MalformedResponse(format!("xAI OpenID configuration lacks {name}"))
            })
    };
    let found = Endpoints {
        device: field("device_authorization_endpoint")?,
        token: field("token_endpoint")?,
        revocation: field("revocation_endpoint").ok(),
    };
    if let Ok(mut guard) = ENDPOINTS.lock() {
        *guard = Some(found.clone());
    }
    Ok(found)
}

fn load_session(credentials: &CredentialStore) -> Result<Option<Session>, RuntimeError> {
    match credentials.get(ProviderId::SuperGrok) {
        Ok(raw) => Ok(serde_json::from_str(&raw).ok()),
        Err(CredentialError::Missing) => Ok(None),
        Err(error) => Err(map_error(error)),
    }
}

fn save_session(credentials: &CredentialStore, session: &Session) -> Result<(), RuntimeError> {
    let raw = serde_json::to_string(session)
        .map_err(|_| RuntimeError::InvalidInput("SuperGrok session could not be saved".into()))?;
    credentials
        .save(ProviderId::SuperGrok, &raw)
        .map(|_| ())
        .map_err(map_error)
}

/// Whether an xAI account is connected; the hint is the account's e-mail address.
pub fn status(credentials: &CredentialStore) -> Result<CredentialStatus, RuntimeError> {
    Ok(match load_session(credentials)? {
        Some(session) => CredentialStatus {
            configured: true,
            hint: Some(session.email).filter(|email| !email.is_empty()),
        },
        None => CredentialStatus {
            configured: false,
            hint: None,
        },
    })
}

fn cached_token() -> Option<String> {
    let guard = ACCESS.lock().ok()?;
    let access = guard.as_ref()?;
    (access.expires_at > Instant::now() + REFRESH_MARGIN).then(|| access.token.clone())
}

fn store_access(reply: &Value) -> Result<String, RuntimeError> {
    let token = reply
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            RuntimeError::MalformedResponse("xAI token response lacks an access token".into())
        })?
        .to_string();
    let lifetime = reply
        .get("expires_in")
        .and_then(Value::as_u64)
        .unwrap_or(3600);
    if let Ok(mut guard) = ACCESS.lock() {
        *guard = Some(Access {
            token: token.clone(),
            expires_at: Instant::now() + Duration::from_secs(lifetime),
        });
    }
    Ok(token)
}

/// Returns a valid access token, refreshing it with the stored refresh token when needed.
pub fn access_token(credentials: &CredentialStore, proxy: &str) -> Result<String, RuntimeError> {
    if let Some(token) = cached_token() {
        return Ok(token);
    }
    let _guard = REFRESH_LOCK
        .lock()
        .map_err(|_| RuntimeError::Connection("SuperGrok refresh lock is poisoned".into()))?;
    if let Some(token) = cached_token() {
        return Ok(token);
    }
    let mut session = load_session(credentials)?.ok_or_else(|| {
        RuntimeError::Authentication("sign in with SuperGrok in Settings first".into())
    })?;
    let transport = transport(proxy)?;
    let endpoints = endpoints(&transport)?;
    let reply = transport
        .post_form(
            &endpoints.token,
            &[
                ("grant_type", "refresh_token"),
                ("client_id", CLIENT_ID),
                ("refresh_token", &session.refresh_token),
            ],
        )
        .map_err(|error| match error {
            RuntimeError::Authentication(detail) => RuntimeError::Authentication(format!(
                "SuperGrok sign-in expired or was revoked, sign in again ({detail})"
            )),
            other => other,
        })?;
    let token = store_access(&reply)?;
    if let Some(rotated) = reply.get("refresh_token").and_then(Value::as_str) {
        if rotated != session.refresh_token {
            session.refresh_token = rotated.to_string();
            save_session(credentials, &session)?;
        }
    }
    Ok(token)
}

/// Requests a device code. The caller opens `verification_url` and shows `user_code`,
/// then calls [`finish_sign_in`].
pub fn begin_sign_in(proxy: &str) -> Result<DeviceCode, RuntimeError> {
    CANCEL.store(false, Ordering::Release);
    let transport = transport(proxy)?;
    let endpoints = endpoints(&transport)?;
    let (status, reply) = transport.post_form_raw(
        &endpoints.device,
        &[("client_id", CLIENT_ID), ("scope", SCOPES)],
    )?;
    if !(200..300).contains(&status) {
        return Err(RuntimeError::Authentication(format!(
            "xAI refused the sign-in request (HTTP {status}, {})",
            oauth_error_code(&reply)
        )));
    }
    let text = |name: &str| {
        reply
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                RuntimeError::MalformedResponse(format!("xAI device response lacks {name}"))
            })
    };
    let device_code = text("device_code")?;
    let user_code = text("user_code")?;
    let verification_url =
        text("verification_uri_complete").or_else(|_| text("verification_uri"))?;
    let expires_in = reply
        .get("expires_in")
        .and_then(Value::as_u64)
        .unwrap_or(600);
    let interval = Duration::from_secs(reply.get("interval").and_then(Value::as_u64).unwrap_or(5))
        .max(MIN_INTERVAL);
    if let Ok(mut guard) = PENDING.lock() {
        *guard = Some(Pending {
            device_code,
            interval,
            expires_at: Instant::now() + Duration::from_secs(expires_in),
        });
    }
    Ok(DeviceCode {
        user_code,
        verification_url,
        expires_in,
    })
}

/// Polls until the user approves (or refuses, or the code expires) and stores the session.
pub fn finish_sign_in(
    credentials: &CredentialStore,
    proxy: &str,
) -> Result<CredentialStatus, RuntimeError> {
    let Pending {
        device_code,
        mut interval,
        expires_at,
    } = PENDING
        .lock()
        .ok()
        .and_then(|mut guard| guard.take())
        .ok_or_else(|| RuntimeError::InvalidInput("no SuperGrok sign-in is in progress".into()))?;
    let transport = transport(proxy)?;
    let endpoints = endpoints(&transport)?;
    loop {
        let wake = Instant::now() + interval;
        while Instant::now() < wake {
            if CANCEL.load(Ordering::Acquire) {
                return Err(RuntimeError::Cancelled);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if Instant::now() > expires_at {
            return Err(RuntimeError::Timeout(
                "SuperGrok sign-in timed out; try again".into(),
            ));
        }
        let (status, reply) = transport.post_form_raw(
            &endpoints.token,
            &[
                ("grant_type", DEVICE_GRANT),
                ("device_code", &device_code),
                ("client_id", CLIENT_ID),
            ],
        )?;
        if (200..300).contains(&status) {
            return complete(credentials, &reply);
        }
        match oauth_error_code(&reply).as_str() {
            "authorization_pending" => {}
            "slow_down" => interval += MIN_INTERVAL,
            "access_denied" => {
                return Err(RuntimeError::Authentication(
                    "SuperGrok sign-in was declined".into(),
                ))
            }
            "expired_token" => {
                return Err(RuntimeError::Timeout(
                    "SuperGrok sign-in expired; try again".into(),
                ))
            }
            code => {
                return Err(RuntimeError::Authentication(format!(
                    "SuperGrok sign-in failed (HTTP {status}, {code})"
                )))
            }
        }
    }
}

fn complete(
    credentials: &CredentialStore,
    reply: &Value,
) -> Result<CredentialStatus, RuntimeError> {
    let refresh_token = reply
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| RuntimeError::Authentication("xAI did not return a refresh token".into()))?
        .to_string();
    store_access(reply)?;
    let email = reply
        .get("id_token")
        .and_then(Value::as_str)
        .and_then(email_from_id_token)
        .unwrap_or_default();
    save_session(
        credentials,
        &Session {
            refresh_token,
            email,
        },
    )?;
    status(credentials)
}

/// Reads the e-mail claim for display only; nothing is trusted from it.
fn email_from_id_token(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims
        .get("email")
        .or_else(|| claims.get("name"))
        .and_then(Value::as_str)
        .map(|value| value.chars().take(120).collect())
}

/// Stops a sign-in that is waiting for the browser.
pub fn cancel_sign_in() {
    CANCEL.store(true, Ordering::Release);
}

/// Forgets the account: revokes the refresh token (best effort) and clears local data.
pub fn sign_out(credentials: &CredentialStore, proxy: &str) -> Result<(), RuntimeError> {
    if let Ok(Some(session)) = load_session(credentials) {
        if let Ok(transport) = transport(proxy) {
            if let Some(url) = endpoints(&transport)
                .ok()
                .and_then(|found| found.revocation)
            {
                let _ = transport.post_form(
                    &url,
                    &[
                        ("token", &session.refresh_token),
                        ("token_type_hint", "refresh_token"),
                        ("client_id", CLIENT_ID),
                    ],
                );
            }
        }
    }
    if let Ok(mut guard) = ACCESS.lock() {
        *guard = None;
    }
    credentials.delete(ProviderId::SuperGrok).map_err(map_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_is_read_from_the_id_token_payload() {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"email":"me@example.com"}"#);
        assert_eq!(
            email_from_id_token(&format!("e30.{payload}.sig")).as_deref(),
            Some("me@example.com")
        );
        assert!(email_from_id_token("garbage").is_none());
    }

    #[test]
    fn finishing_without_a_started_sign_in_is_an_error() {
        if let Ok(mut guard) = PENDING.lock() {
            *guard = None;
        }
        let credentials = CredentialStore::windows();
        assert!(matches!(
            finish_sign_in(&credentials, ""),
            Err(RuntimeError::InvalidInput(_))
        ));
    }
}
