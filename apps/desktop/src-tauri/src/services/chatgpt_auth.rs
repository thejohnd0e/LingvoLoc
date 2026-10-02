//! "Sign in with ChatGPT" for open-source, locally run apps: lets a ChatGPT Plus/Pro user
//! spend their plan allowance on eligible Responses API requests instead of an API key.
//!
//! OAuth 2.0 authorization code flow with PKCE against `auth.openai.com`, a loopback
//! redirect on `127.0.0.1`, and a dynamically issued client id. Only the small session
//! record (client id, refresh token, email, host id) is stored in Windows Credential
//! Manager; access tokens stay in memory and are refreshed on demand.

use crate::backends::http::HttpTransport;
use crate::domain::{ProviderId, RuntimeError};
use crate::services::credentials::{map_error, CredentialStatus, CredentialStore};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ISSUER: &str = "https://auth.openai.com";
pub const REDIRECT_PORT: u16 = 47835;
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const PLAN_SCOPE: &str = "chatgpt.tokens.use.direct";
const RESOURCE: &str = "https://api.openai.com/v1";
const PLACEHOLDER_CLIENT_ID: &str = "dynamic_agent_client";
const APP_NAME: &str = "LingvoLoc";
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);
const REFRESH_MARGIN: Duration = Duration::from_secs(60);

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Session {
    client_id: String,
    refresh_token: String,
    email: String,
    host_id: String,
}

#[derive(Clone)]
struct Endpoints {
    authorization: String,
    token: String,
    revocation: Option<String>,
}

struct Access {
    token: String,
    expires_at: Instant,
}

static ACCESS: Mutex<Option<Access>> = Mutex::new(None);
static ENDPOINTS: Mutex<Option<Endpoints>> = Mutex::new(None);
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
            RuntimeError::MalformedResponse("OpenID configuration is not valid JSON".into())
        })?;
    let field = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                RuntimeError::MalformedResponse(format!("OpenID configuration lacks {name}"))
            })
    };
    let found = Endpoints {
        authorization: field("authorization_endpoint")?,
        token: field("token_endpoint")?,
        revocation: field("revocation_endpoint").ok(),
    };
    if let Ok(mut guard) = ENDPOINTS.lock() {
        *guard = Some(found.clone());
    }
    Ok(found)
}

fn load_session(credentials: &CredentialStore) -> Result<Option<Session>, RuntimeError> {
    match credentials.get(ProviderId::ChatGpt) {
        Ok(raw) => Ok(serde_json::from_str(&raw).ok()),
        Err(crate::services::credentials::CredentialError::Missing) => Ok(None),
        Err(error) => Err(map_error(error)),
    }
}

fn save_session(credentials: &CredentialStore, session: &Session) -> Result<(), RuntimeError> {
    let raw = serde_json::to_string(session)
        .map_err(|_| RuntimeError::InvalidInput("ChatGPT session could not be saved".into()))?;
    credentials
        .save(ProviderId::ChatGpt, &raw)
        .map(|_| ())
        .map_err(map_error)
}

/// Whether a ChatGPT account is connected; the hint is the account's e-mail address.
pub fn status(credentials: &CredentialStore) -> Result<CredentialStatus, RuntimeError> {
    Ok(match load_session(credentials)? {
        Some(session) => CredentialStatus {
            configured: true,
            hint: Some(session.email),
        },
        None => CredentialStatus {
            configured: false,
            hint: None,
        },
    })
}

/// Returns a valid access token, refreshing it with the stored refresh token when needed.
pub fn access_token(credentials: &CredentialStore, proxy: &str) -> Result<String, RuntimeError> {
    if let Some(token) = cached_token() {
        return Ok(token);
    }
    let _guard = REFRESH_LOCK
        .lock()
        .map_err(|_| RuntimeError::Connection("ChatGPT refresh lock is poisoned".into()))?;
    if let Some(token) = cached_token() {
        return Ok(token);
    }
    let mut session = load_session(credentials)?.ok_or_else(|| {
        RuntimeError::Authentication("sign in with ChatGPT in Settings first".into())
    })?;
    let transport = transport(proxy)?;
    let endpoints = endpoints(&transport)?;
    let reply = transport
        .post_form(
            &endpoints.token,
            &[
                ("grant_type", "refresh_token"),
                ("client_id", &session.client_id),
                ("refresh_token", &session.refresh_token),
                ("resource", RESOURCE),
            ],
        )
        .map_err(|error| match error {
            RuntimeError::Authentication(detail) => RuntimeError::Authentication(format!(
                "ChatGPT sign-in expired or was revoked, sign in again ({detail})"
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
            RuntimeError::MalformedResponse("token response lacks an access token".into())
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

/// Stops a sign-in that is waiting for the browser.
pub fn cancel_sign_in() {
    CANCEL.store(true, Ordering::Release);
}

/// Runs the browser sign-in. `open_browser` receives the authorization URL.
pub fn sign_in(
    credentials: &CredentialStore,
    proxy: &str,
    open_browser: impl FnOnce(&str) -> Result<(), String>,
) -> Result<CredentialStatus, RuntimeError> {
    CANCEL.store(false, Ordering::Release);
    let transport = transport(proxy)?;
    let endpoints = endpoints(&transport)?;
    let previous = load_session(credentials)?;
    let host_id = previous
        .as_ref()
        .map(|session| session.host_id.clone())
        .unwrap_or_else(new_host_id);

    let listener = TcpListener::bind(("127.0.0.1", REDIRECT_PORT)).map_err(|_| {
        RuntimeError::Connection(format!(
            "port {REDIRECT_PORT} is busy; close the other sign-in and try again"
        ))
    })?;
    let redirect_uri = format!("http://127.0.0.1:{REDIRECT_PORT}/auth/callback");
    let verifier = random_token();
    let state = random_token();
    let nonce = random_token();
    let mut params: Vec<(&str, String)> = vec![
        (
            "client_id",
            previous
                .as_ref()
                .map(|session| session.client_id.clone())
                .unwrap_or_else(|| PLACEHOLDER_CLIENT_ID.into()),
        ),
        ("response_type", "code".into()),
        ("redirect_uri", redirect_uri.clone()),
        ("scope", SCOPES.into()),
        ("resource", RESOURCE.into()),
        ("state", state.clone()),
        ("nonce", nonce.clone()),
        ("code_challenge_method", "S256".into()),
        ("code_challenge", challenge(&verifier)),
        ("ext_agent_host_id", host_id.clone()),
    ];
    match &previous {
        Some(session) if !session.email.is_empty() => {
            params.push(("login_hint", session.email.clone()))
        }
        Some(_) => {}
        None => params.push(("agent_name_hint", APP_NAME.into())),
    }
    let url = reqwest::Url::parse_with_params(&endpoints.authorization, &params)
        .map_err(|_| RuntimeError::InvalidInput("authorization URL is invalid".into()))?;
    open_browser(url.as_str()).map_err(|error| {
        RuntimeError::Connection(format!("could not open the browser: {error}"))
    })?;

    let callback = wait_for_callback(&listener)?;
    if let Some(error) = callback.error {
        return Err(RuntimeError::Authentication(format!(
            "ChatGPT sign-in was not completed ({error})"
        )));
    }
    if !equal(callback.state.as_bytes(), state.as_bytes()) {
        return Err(RuntimeError::Authentication(
            "sign-in response did not match the request".into(),
        ));
    }
    let code = callback
        .code
        .ok_or_else(|| RuntimeError::Authentication("sign-in response had no code".into()))?;
    let client_id = resolve_client_id(previous.as_ref(), callback.client_id.as_deref())?;

    let reply = transport.post_form(
        &endpoints.token,
        &[
            ("grant_type", "authorization_code"),
            ("client_id", &client_id),
            ("code", &code),
            ("code_verifier", &verifier),
            ("redirect_uri", &redirect_uri),
            ("resource", RESOURCE),
        ],
    )?;
    if let Some(scope) = reply.get("scope").and_then(Value::as_str) {
        if !scope.split_whitespace().any(|item| item == PLAN_SCOPE) {
            return Err(RuntimeError::Authentication(
                "ChatGPT plan usage was not granted".into(),
            ));
        }
    }
    let refresh_token = reply
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            RuntimeError::Authentication("ChatGPT did not return a refresh token".into())
        })?
        .to_string();
    let email = match reply.get("id_token").and_then(Value::as_str) {
        Some(token) => validate_id_token(token, &client_id, &nonce)?,
        None => String::new(),
    };
    store_access(&reply)?;
    let session = Session {
        client_id,
        refresh_token,
        email: if email.is_empty() {
            previous.map(|session| session.email).unwrap_or_default()
        } else {
            email
        },
        host_id,
    };
    save_session(credentials, &session)?;
    status(credentials)
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
                        ("client_id", &session.client_id),
                    ],
                );
            }
        }
    }
    if let Ok(mut guard) = ACCESS.lock() {
        *guard = None;
    }
    credentials.delete(ProviderId::ChatGpt).map_err(map_error)
}

fn resolve_client_id(
    previous: Option<&Session>,
    returned: Option<&str>,
) -> Result<String, RuntimeError> {
    let valid = |id: &str| {
        !id.is_empty()
            && id.len() <= 200
            && id != PLACEHOLDER_CLIENT_ID
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    match (previous, returned) {
        (Some(session), Some(id)) if session.client_id != id => Err(RuntimeError::Authentication(
            "ChatGPT issued a different client id".into(),
        )),
        (Some(session), _) => Ok(session.client_id.clone()),
        (None, Some(id)) if valid(id) => Ok(id.to_string()),
        (None, _) => Err(RuntimeError::Authentication(
            "ChatGPT did not issue a client id".into(),
        )),
    }
}

#[derive(Default)]
struct Callback {
    code: Option<String>,
    state: String,
    client_id: Option<String>,
    error: Option<String>,
}

fn wait_for_callback(listener: &TcpListener) -> Result<Callback, RuntimeError> {
    listener
        .set_nonblocking(true)
        .map_err(|_| RuntimeError::Connection("could not listen for the browser".into()))?;
    let deadline = Instant::now() + SIGN_IN_TIMEOUT;
    loop {
        if CANCEL.load(Ordering::Acquire) {
            return Err(RuntimeError::Cancelled);
        }
        if Instant::now() > deadline {
            return Err(RuntimeError::Timeout(
                "ChatGPT sign-in timed out; try again".into(),
            ));
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(callback) = handle_connection(stream) {
                    return Ok(callback);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return Err(RuntimeError::Connection("browser connection failed".into())),
        }
    }
}

fn handle_connection(mut stream: TcpStream) -> Option<Callback> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buffer = [0_u8; 8192];
    let count = stream.read(&mut buffer).ok()?;
    let request = String::from_utf8_lossy(&buffer[..count]).into_owned();
    let callback = parse_callback_request(&request);
    let (status, body) = match &callback {
        Some(callback) if callback.error.is_some() => (
            "200 OK",
            "Sign-in was not completed. You can close this tab and return to LingvoLoc.",
        ),
        Some(_) => (
            "200 OK",
            "Signed in. You can close this tab and return to LingvoLoc.",
        ),
        None => ("404 Not Found", "Not found."),
    };
    let page = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>LingvoLoc</title><body style=\"font-family:sans-serif;padding:2rem\"><p>{body}</p>"
    );
    let _ = stream.write_all(
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{page}",
            page.len()
        )
        .as_bytes(),
    );
    callback
}

fn parse_callback_request(request: &str) -> Option<Callback> {
    let mut lines = request.lines();
    let first = lines.next()?;
    let mut parts = first.split_whitespace();
    if parts.next()? != "GET" {
        return None;
    }
    let target = parts.next()?;
    let host_ok = lines.any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("host")
                && value.trim() == format!("127.0.0.1:{REDIRECT_PORT}")
        })
    });
    if !host_ok {
        return None;
    }
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
    if url.path() != "/auth/callback" {
        return None;
    }
    let mut callback = Callback::default();
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => callback.code = Some(value.into_owned()),
            "state" => callback.state = value.into_owned(),
            "client_id" => callback.client_id = Some(value.into_owned()),
            "error" => callback.error = Some(value.chars().take(80).collect()),
            _ => {}
        }
    }
    Some(callback)
}

/// Checks the claims of the ID token (issuer, audience, nonce, expiry) and returns the
/// account e-mail. The token arrives straight from the token endpoint over TLS, which
/// OpenID Connect accepts in place of a signature check.
fn validate_id_token(token: &str, client_id: &str, nonce: &str) -> Result<String, RuntimeError> {
    let invalid = || RuntimeError::Authentication("ChatGPT identity token is invalid".into());
    let payload = token.split('.').nth(1).ok_or_else(invalid)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).map_err(|_| invalid())?;
    let claims: Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if claims.get("iss").and_then(Value::as_str) != Some(ISSUER) {
        return Err(invalid());
    }
    let audience_ok = match claims.get("aud") {
        Some(Value::String(aud)) => aud == client_id,
        Some(Value::Array(list)) => list.iter().any(|aud| aud.as_str() == Some(client_id)),
        _ => false,
    };
    if !audience_ok {
        return Err(invalid());
    }
    if let Some(found) = claims.get("nonce").and_then(Value::as_str) {
        if found != nonce {
            return Err(invalid());
        }
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    match claims.get("exp").and_then(Value::as_u64) {
        Some(exp) if exp + 5 >= now => {}
        _ => return Err(invalid()),
    }
    Ok(claims
        .get("email")
        .or_else(|| claims.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string())
}

fn random_token() -> String {
    URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>())
}

fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn new_host_id() -> String {
    let mut bytes = rand::random::<[u8; 16]>();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "urn:uuid:{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn equal(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id_token(claims: Value) -> String {
        format!(
            "e30.{}.sig",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
        )
    }

    #[test]
    fn pkce_challenge_matches_the_rfc_example() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn host_id_is_a_uuid_urn() {
        let id = new_host_id();
        assert!(id.starts_with("urn:uuid:"));
        assert_eq!(id.len(), "urn:uuid:".len() + 36);
        assert_ne!(id, new_host_id());
    }

    #[test]
    fn callback_requires_the_loopback_host_and_path() {
        let ok = format!(
            "GET /auth/callback?code=abc&state=xyz&client_id=app_1 HTTP/1.1\r\nHost: 127.0.0.1:{REDIRECT_PORT}\r\n\r\n"
        );
        let callback = parse_callback_request(&ok).unwrap();
        assert_eq!(callback.code.as_deref(), Some("abc"));
        assert_eq!(callback.state, "xyz");
        assert_eq!(callback.client_id.as_deref(), Some("app_1"));
        let wrong_host = "GET /auth/callback?code=a&state=b HTTP/1.1\r\nHost: evil.test\r\n\r\n";
        assert!(parse_callback_request(wrong_host).is_none());
        let wrong_path =
            format!("GET /other?code=a HTTP/1.1\r\nHost: 127.0.0.1:{REDIRECT_PORT}\r\n\r\n");
        assert!(parse_callback_request(&wrong_path).is_none());
        let post =
            format!("POST /auth/callback HTTP/1.1\r\nHost: 127.0.0.1:{REDIRECT_PORT}\r\n\r\n");
        assert!(parse_callback_request(&post).is_none());
    }

    #[test]
    fn id_token_claims_are_checked() {
        let exp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 600;
        let good = id_token(serde_json::json!({
            "iss": ISSUER, "aud": "app_1", "nonce": "n1", "exp": exp, "email": "me@example.com"
        }));
        assert_eq!(
            validate_id_token(&good, "app_1", "n1").unwrap(),
            "me@example.com"
        );
        assert!(validate_id_token(&good, "other", "n1").is_err());
        assert!(validate_id_token(&good, "app_1", "n2").is_err());
        let expired = id_token(serde_json::json!({
            "iss": ISSUER, "aud": "app_1", "exp": 1, "email": "me@example.com"
        }));
        assert!(validate_id_token(&expired, "app_1", "n1").is_err());
        let foreign = id_token(serde_json::json!({
            "iss": "https://evil.test", "aud": "app_1", "exp": exp
        }));
        assert!(validate_id_token(&foreign, "app_1", "n1").is_err());
    }

    #[test]
    fn client_id_rules() {
        assert_eq!(
            resolve_client_id(None, Some("oaiapp_abc-1")).unwrap(),
            "oaiapp_abc-1"
        );
        assert!(resolve_client_id(None, Some(PLACEHOLDER_CLIENT_ID)).is_err());
        assert!(resolve_client_id(None, Some("bad id")).is_err());
        assert!(resolve_client_id(None, None).is_err());
        let session = Session {
            client_id: "app_1".into(),
            refresh_token: "r".into(),
            email: String::new(),
            host_id: "h".into(),
        };
        assert_eq!(resolve_client_id(Some(&session), None).unwrap(), "app_1");
        assert!(resolve_client_id(Some(&session), Some("app_2")).is_err());
    }
}
