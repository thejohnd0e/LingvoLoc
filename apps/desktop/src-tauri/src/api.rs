use crate::domain::{RuntimeError, Settings, TranslationRequest};
use crate::{services::translation, AppState};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use tauri::{AppHandle, Manager};

const API_ADDRESS: &str = "127.0.0.1:47831";
const MAX_BODY_BYTES: usize = 128 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranslateBody {
    text: String,
    source_language: String,
    target_language: String,
}

fn build_translation_request(
    settings: &Settings,
    body: TranslateBody,
    source_language: String,
) -> TranslationRequest {
    TranslationRequest {
        model_id: settings.active_model_id().to_string(),
        adapter_id: settings.adapter_id.clone(),
        source_language,
        target_language: body.target_language,
        text: body.text,
        translation_style: settings.translation_style,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LanguagePair {
    primary_language: String,
    secondary_language: String,
}

pub fn generate_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn start(app: AppHandle) {
    thread::Builder::new()
        .name("lingvoloc-api".into())
        .spawn(move || {
            let listener = match TcpListener::bind(API_ADDRESS) {
                Ok(listener) => listener,
                Err(error) => {
                    eprintln!("LingvoLoc API unavailable on {API_ADDRESS}: {error}");
                    return;
                }
            };
            for stream in listener.incoming().flatten() {
                let app = app.clone();
                let _ = thread::Builder::new()
                    .name("lingvoloc-api-request".into())
                    .spawn(move || handle_connection(stream, &app));
            }
        })
        .expect("LingvoLoc API thread should start");
}

fn handle_connection(mut stream: TcpStream, app: &AppHandle) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let request = match read_request(&mut stream) {
        Ok(request) => request,
        Err(error) => {
            write_response(&mut stream, 400, json_error(&error.to_string()), None);
            return;
        }
    };
    let origin = request.header("origin");
    if !allowed_origin(origin) {
        write_response(&mut stream, 403, json_error("origin is not allowed"), None);
        return;
    }
    if request.method == "OPTIONS" {
        write_response(&mut stream, 204, String::new(), origin);
        return;
    }
    let expected_token = app.state::<AppState>().api_token.clone();
    let expected = format!("Bearer {expected_token}");
    let authorized = request
        .header("authorization")
        .is_some_and(|value| value.trim() == expected);
    if !authorized {
        write_response(&mut stream, 401, json_error("authorization required"), None);
        return;
    }

    let response = route(&request, app);
    write_response(&mut stream, response.0, response.1, origin);
}

fn allowed_origin(origin: Option<&str>) -> bool {
    origin
        .map(|value| {
            value
                .strip_prefix("chrome-extension://")
                .is_some_and(|id| !id.is_empty())
        })
        .unwrap_or(true)
}

fn route(request: &HttpRequest, app: &AppHandle) -> (u16, String) {
    let state = app.state::<AppState>();
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/api/v1/status") => match settings(&state) {
            // Pairing only proves the desktop token works. Provider outages must not
            // look like a bad pairing token to the extension.
            Ok(settings) => {
                match translation::status_with_credentials(&settings, &state.credentials) {
                    Ok(status) => json_result(Ok(status)),
                    Err(error) => json_result(Ok(crate::domain::RuntimeStatus {
                        available: false,
                        endpoint: String::new(),
                        detail: error.to_string(),
                    })),
                }
            }
            Err(error) => error_response(error),
        },
        ("GET", "/api/v1/models") => match settings(&state) {
            Ok(settings) => json_result(translation::list_models_with_credentials(
                &settings,
                &state.credentials,
            )),
            Err(error) => error_response(error),
        },
        ("GET", "/api/v1/settings/language-pair") => match settings(&state) {
            Ok(settings) => json_result(Ok(LanguagePair {
                primary_language: settings.primary_language,
                secondary_language: settings.secondary_language,
            })),
            Err(error) => json_result::<LanguagePair>(Err(error)),
        },
        ("POST", "/api/v1/translate") => translate(request, &state),
        _ => (404, json_error("endpoint not found")),
    }
}

fn translate(request: &HttpRequest, state: &AppState) -> (u16, String) {
    let body: TranslateBody = match serde_json::from_slice(&request.body) {
        Ok(body) => body,
        Err(error) => return (400, json_error(&format!("invalid JSON: {error}"))),
    };
    if body.text.trim().is_empty() || body.text.len() > MAX_BODY_BYTES {
        return (
            400,
            json_error("text must be non-empty and at most 128 KiB"),
        );
    }
    let settings = match settings_from_state(state) {
        Ok(settings) => settings,
        Err(error) => return error_response(error),
    };
    if settings.active_model_id().trim().is_empty() {
        return error_response(RuntimeError::InvalidInput(
            "a model must be selected in LingvoLoc".into(),
        ));
    }
    let source_language = if body.source_language == "auto" {
        match crate::services::detection::detect_supported_language(&body.text) {
            Ok(language) => language.code,
            Err(error) => return error_response(error),
        }
    } else {
        body.source_language.clone()
    };
    let request = build_translation_request(&settings, body, source_language);
    let snapshot = crate::services::inference_coordinator::snapshot(&settings, &request.model_id);
    let (request_id, cancellation) = state.request_registry.start_generated("api");
    let result = state.inference.run_interactive(&snapshot, || {
        translation::translate_with_cancellation_and_credentials(
            &settings,
            request.clone(),
            &cancellation,
            &state.credentials,
        )
    });
    state.request_registry.finish(&request_id);
    match result {
        Ok(result) => {
            state.session_usage.record_success(&result);
            let history_result = state
                .history
                .lock()
                .map_err(|_| RuntimeError::Connection("history lock is poisoned".into()))
                .and_then(|history| history.add(&request.text, &request, &result));
            match history_result {
                Ok(()) => json_result(Ok(result)),
                Err(error) => error_response(error),
            }
        }
        Err(error) => error_response(error),
    }
}

fn settings(state: &tauri::State<'_, AppState>) -> SettingsResult {
    state
        .settings
        .lock()
        .map(|settings| settings.clone())
        .map_err(|_| RuntimeError::Connection("settings lock is poisoned".into()))
}

fn settings_from_state(state: &AppState) -> SettingsResult {
    state
        .settings
        .lock()
        .map(|settings| settings.clone())
        .map_err(|_| RuntimeError::Connection("settings lock is poisoned".into()))
}

type SettingsResult = Result<Settings, RuntimeError>;

fn error_response(error: RuntimeError) -> (u16, String) {
    let status = match error {
        RuntimeError::InvalidInput(_) => 400,
        RuntimeError::Connection(_) | RuntimeError::Timeout(_) => 503,
        RuntimeError::Http { .. } | RuntimeError::MalformedResponse(_) => 502,
        RuntimeError::UnsupportedAdapter(_) => 422,
        RuntimeError::Authentication(_) => 401,
        RuntimeError::Quota(_) => 402,
        RuntimeError::RateLimited { .. } => 429,
        RuntimeError::ContentRejected(_) => 422,
        RuntimeError::Cancelled => 499,
    };
    (status, json_error(&error.to_string()))
}

fn json_result<T: Serialize>(result: Result<T, RuntimeError>) -> (u16, String) {
    match result {
        Ok(value) => (
            200,
            serde_json::to_string(&value).unwrap_or_else(|_| json_error("serialization failed")),
        ),
        Err(error) => error_response(error),
    }
}

fn json_error(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}

struct HttpRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

fn read_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut bytes = Vec::with_capacity(4096);
    let mut buffer = [0_u8; 4096];
    let header_end;
    loop {
        let count = stream
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            return Err("request ended before headers".into());
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() > MAX_BODY_BYTES + 16 * 1024 {
            return Err("request is too large".into());
        }
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            header_end = index + 4;
            break;
        }
    }
    let header_text = std::str::from_utf8(&bytes[..header_end]).map_err(|_| "invalid headers")?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().ok_or("missing request line")?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().ok_or("missing method")?.to_string();
    let path = request_parts.next().ok_or("missing path")?.to_string();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect::<Vec<_>>();
    let content_length = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.parse::<usize>().map_err(|_| "invalid content length"))
        .transpose()?
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return Err("request body is too large".into());
    }
    while bytes.len() - header_end < content_length {
        let count = stream
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            return Err("request ended before body".into());
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    Ok(HttpRequest {
        method,
        path,
        headers,
        body: bytes[header_end..header_end + content_length].to_vec(),
    })
}

fn write_response(stream: &mut TcpStream, status: u16, body: String, origin: Option<&str>) {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        422 => "Unprocessable Entity",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Error",
    };
    let cors = origin
        .filter(|value| allowed_origin(Some(value)))
        .map(|value| {
            format!(
                "Access-Control-Allow-Origin: {value}\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Authorization, Content-Type\r\n"
            )
        })
        .unwrap_or_default();
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{cors}\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::{allowed_origin, build_translation_request, read_request, TranslateBody};
    use crate::domain::{RuntimeMode, Settings, TranslationStyle};
    use std::io::Write;
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    #[test]
    fn only_extension_origins_are_allowed() {
        assert!(allowed_origin(None));
        assert!(allowed_origin(Some("chrome-extension://abcdefghijklmnop")));
        assert!(!allowed_origin(Some("https://example.com")));
        assert!(!allowed_origin(Some("chrome-extension://")));
    }

    #[test]
    fn bearer_token_comparison_trims_header_value() {
        let expected_token = "abc123";
        let expected = format!("Bearer {expected_token}");
        assert_eq!("Bearer abc123".trim(), expected);
        assert_eq!(" Bearer abc123 ".trim(), expected);
    }

    #[test]
    fn parses_json_request_body() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let address = listener.local_addr().expect("listener should have address");
        let sender = thread::spawn(move || {
            let mut stream = TcpStream::connect(address).expect("client should connect");
            stream
                .write_all(b"POST /api/v1/translate HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}")
                .expect("request should write");
        });
        let (mut stream, _) = listener.accept().expect("server should accept");
        let request = read_request(&mut stream).expect("request should parse");
        sender.join().expect("sender should finish");
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/api/v1/translate");
        assert_eq!(request.body, b"{}");
    }

    #[test]
    fn builds_extension_request_with_desktop_style() {
        let settings = Settings {
            runtime_mode: RuntimeMode::Standalone,
            models_directory: String::new(),
            llama_server_path: String::new(),
            endpoint: "local".into(),
            model_id: "model".into(),
            adapter_id: "adapter".into(),
            source_language: "auto".into(),
            target_language: "en".into(),
            primary_language: "en".into(),
            secondary_language: "ru".into(),
            translation_style: TranslationStyle::Conversational,
            cloud: Default::default(),
        };
        let request = build_translation_request(
            &settings,
            TranslateBody {
                text: "Hello".into(),
                source_language: "en".into(),
                target_language: "ru".into(),
            },
            "en".into(),
        );
        assert_eq!(request.translation_style, TranslationStyle::Conversational);
        assert_eq!(request.text, "Hello");
        assert_eq!(request.model_id, "model");
    }

    #[test]
    fn builds_extension_request_with_cloud_model_id() {
        let mut settings = Settings {
            runtime_mode: RuntimeMode::Gemini,
            models_directory: String::new(),
            llama_server_path: String::new(),
            endpoint: "local".into(),
            model_id: String::new(),
            adapter_id: "adapter".into(),
            source_language: "auto".into(),
            target_language: "en".into(),
            primary_language: "en".into(),
            secondary_language: "ru".into(),
            translation_style: TranslationStyle::Neutral,
            cloud: Default::default(),
        };
        settings.cloud.gemini.model_id = "gemini-2.5-flash-lite".into();
        let request = build_translation_request(
            &settings,
            TranslateBody {
                text: "Hello".into(),
                source_language: "en".into(),
                target_language: "th".into(),
            },
            "en".into(),
        );
        assert_eq!(request.model_id, "gemini-2.5-flash-lite");
        assert_eq!(request.target_language, "th");
    }
}
