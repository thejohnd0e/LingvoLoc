use crate::domain::RuntimeError;
use reqwest::blocking::{Client, Response};
use reqwest::header::HeaderMap;
use std::io::BufRead;
use std::time::{Duration, SystemTime};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(45);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(900);
pub const MAX_ERROR_BODY: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct HttpTimeouts {
    pub connect: Duration,
    pub idle: Duration,
    pub request: Duration,
    pub max_error_body: usize,
}

impl Default for HttpTimeouts {
    fn default() -> Self {
        Self {
            connect: CONNECT_TIMEOUT,
            idle: IDLE_TIMEOUT,
            request: REQUEST_TIMEOUT,
            max_error_body: MAX_ERROR_BODY,
        }
    }
}

pub struct HttpTransport {
    client: Client,
    timeouts: HttpTimeouts,
}

impl HttpTransport {
    pub fn new() -> Result<Self, RuntimeError> {
        Self::with_timeouts(HttpTimeouts::default())
    }

    pub fn with_timeouts(timeouts: HttpTimeouts) -> Result<Self, RuntimeError> {
        let client = Client::builder()
            .connect_timeout(timeouts.connect)
            .timeout(timeouts.request)
            .build()
            .map_err(|error| RuntimeError::Connection(format!("HTTP client: {error}")))?;
        Ok(Self { client, timeouts })
    }

    pub fn get(&self, url: &str) -> Result<Response, RuntimeError> {
        let response = self.client.get(url).send().map_err(map_request_error)?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let headers = response.headers().clone();
        Err(safe_http_error_with_cap(
            status,
            &headers,
            response,
            self.timeouts.max_error_body,
        ))
    }

    pub fn post_json<T: serde::Serialize>(
        &self,
        url: &str,
        headers: HeaderMap,
        body: &T,
    ) -> Result<Response, RuntimeError> {
        let response = self
            .client
            .post(url)
            .headers(headers)
            .json(body)
            .send()
            .map_err(map_request_error)?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let headers = response.headers().clone();
        Err(safe_http_error_with_cap(
            status,
            &headers,
            response,
            self.timeouts.max_error_body,
        ))
    }
}

pub fn read_sse_lines<R: BufRead, F: FnMut(&str) -> Result<(), RuntimeError>>(
    reader: R,
    mut on_data: F,
) -> Result<(), RuntimeError> {
    for line in reader.lines() {
        let line = line.map_err(|error| RuntimeError::Connection(format!("SSE read: {error}")))?;
        if let Some(data) = line.strip_prefix("data:") {
            let data = data.trim();
            if !data.is_empty() && data != "[DONE]" {
                on_data(data)?;
            }
        }
    }
    Ok(())
}

pub fn safe_http_error(
    status: reqwest::StatusCode,
    headers: &HeaderMap,
    body: &[u8],
) -> RuntimeError {
    safe_http_error_parts(status, headers, body, MAX_ERROR_BODY)
}

fn safe_http_error_with_cap(
    status: reqwest::StatusCode,
    headers: &HeaderMap,
    response: Response,
    cap: usize,
) -> RuntimeError {
    let body = response
        .bytes()
        .map(|body| body.into_iter().take(cap).collect::<Vec<_>>())
        .unwrap_or_default();
    safe_http_error_parts(status, headers, &body, cap)
}

fn safe_http_error_parts(
    status: reqwest::StatusCode,
    headers: &HeaderMap,
    _body: &[u8],
    _cap: usize,
) -> RuntimeError {
    let detail = format!("provider returned HTTP {}", status.as_u16());
    match status {
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            RuntimeError::Authentication(detail)
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => RuntimeError::RateLimited {
            detail,
            retry_after: retry_after(headers),
        },
        reqwest::StatusCode::PAYMENT_REQUIRED => RuntimeError::Quota(detail),
        reqwest::StatusCode::BAD_GATEWAY
        | reqwest::StatusCode::SERVICE_UNAVAILABLE
        | reqwest::StatusCode::GATEWAY_TIMEOUT => RuntimeError::Connection(detail),
        _ => RuntimeError::Http {
            status: status.as_u16(),
            detail,
        },
    }
}

fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get("retry-after")?.to_str().ok()?;
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let date = httpdate::parse_http_date(value).ok()?;
    date.duration_since(SystemTime::now()).ok()
}

fn map_request_error(error: reqwest::Error) -> RuntimeError {
    if error.is_timeout() {
        RuntimeError::Timeout("cloud HTTP request timed out".into())
    } else {
        RuntimeError::Connection("cloud HTTP request failed".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::RuntimeError;
    use std::io::Write;
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn never_returning_headers_hit_the_short_test_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(200));
        });
        let transport = HttpTransport::with_timeouts(HttpTimeouts {
            connect: Duration::from_millis(50),
            idle: Duration::from_millis(50),
            request: Duration::from_millis(100),
            max_error_body: 1024,
        })
        .unwrap();
        let error = transport.get(&format!("http://{address}/")).unwrap_err();
        assert!(matches!(error, RuntimeError::Timeout(_)));
    }

    #[test]
    fn safe_http_error_redacts_headers_and_truncates_body() {
        let headers = reqwest::header::HeaderMap::from_iter([(
            reqwest::header::AUTHORIZATION,
            "Bearer super-secret".parse().unwrap(),
        )]);
        let error = safe_http_error(
            reqwest::StatusCode::UNAUTHORIZED,
            &headers,
            b"super-secret source sentence",
        );
        let rendered = error.to_string();
        assert!(!rendered.contains("super-secret"));
        assert!(matches!(error, RuntimeError::Authentication(_)));
    }

    #[test]
    fn retry_after_seconds_are_preserved_without_provider_body() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("retry-after", "7".parse().unwrap());
        let error = safe_http_error(reqwest::StatusCode::TOO_MANY_REQUESTS, &headers, b"busy");
        assert!(matches!(
            error,
            RuntimeError::RateLimited {
                retry_after: Some(duration),
                ..
            } if duration == Duration::from_secs(7)
        ));
    }

    #[test]
    fn retry_after_http_date_is_converted_to_a_bounded_duration() {
        let mut headers = reqwest::header::HeaderMap::new();
        let date = httpdate::fmt_http_date(std::time::SystemTime::now() + Duration::from_secs(7));
        headers.insert("retry-after", date.parse().unwrap());
        let error = safe_http_error(reqwest::StatusCode::TOO_MANY_REQUESTS, &headers, b"busy");
        assert!(matches!(
            error,
            RuntimeError::RateLimited {
                retry_after: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn common_provider_failures_have_distinguishable_errors() {
        let headers = reqwest::header::HeaderMap::new();
        assert!(matches!(
            safe_http_error(reqwest::StatusCode::PAYMENT_REQUIRED, &headers, b"quota"),
            RuntimeError::Quota(_)
        ));
        assert!(matches!(
            safe_http_error(reqwest::StatusCode::BAD_GATEWAY, &headers, b"upstream"),
            RuntimeError::Connection(_)
        ));
        assert!(matches!(
            safe_http_error(
                reqwest::StatusCode::UNPROCESSABLE_ENTITY,
                &headers,
                b"blocked"
            ),
            RuntimeError::Http { status: 422, .. }
        ));
    }

    #[test]
    fn sse_reader_passes_only_data_lines_and_skips_done_marker() {
        let input = "event: message\ndata: {\"text\":\"one\"}\n\ndata: [DONE]\n";
        let mut events = Vec::new();
        read_sse_lines(input.as_bytes(), |event| {
            events.push(event.to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!(events, vec![r#"{"text":"one"}"#]);
    }

    #[allow(dead_code)]
    fn _write_headers(stream: &mut TcpStream) {
        stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
    }
}
