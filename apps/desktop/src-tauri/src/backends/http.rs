use crate::domain::RuntimeError;
use crate::services::request_control::RequestCancellation;
use reqwest::blocking::{Client, Response};
use reqwest::header::HeaderMap;
use std::io::{BufRead, Read};
use std::time::{Duration, SystemTime};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(45);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(900);
/// Longest wait for response headers of a streaming request.
pub const HEADER_TIMEOUT: Duration = Duration::from_secs(30);
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

#[derive(Clone)]
pub struct HttpTransport {
    client: Client,
    async_client: reqwest::Client,
    timeouts: HttpTimeouts,
    provider_messages: bool,
}

impl HttpTransport {
    pub fn new() -> Result<Self, RuntimeError> {
        Self::with_timeouts(HttpTimeouts::default())
    }

    pub fn with_timeouts(timeouts: HttpTimeouts) -> Result<Self, RuntimeError> {
        Self::build(timeouts, "", false)
    }

    /// Rebuilds the clients so every request goes through `proxy` (an `http://` or
    /// `https://` URL). An empty value keeps direct connections.
    pub fn with_proxy(&self, proxy: &str) -> Result<Self, RuntimeError> {
        Self::build(self.timeouts, proxy.trim(), self.provider_messages)
    }

    fn build(
        timeouts: HttpTimeouts,
        proxy: &str,
        provider_messages: bool,
    ) -> Result<Self, RuntimeError> {
        let proxy = if proxy.is_empty() {
            None
        } else {
            let parsed = reqwest::Url::parse(proxy)
                .ok()
                .filter(|url| matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h") && url.host_str().is_some())
                .ok_or_else(|| {
                    RuntimeError::InvalidInput(
                        "proxy must be an http://, https:// or socks5:// URL, e.g. http://192.168.0.12:9102"
                            .into(),
                    )
                })?;
            Some(
                reqwest::Proxy::all(parsed.as_str())
                    .map_err(|_| RuntimeError::InvalidInput("proxy address is invalid".into()))?,
            )
        };
        let client = {
            let builder = Client::builder()
                .connect_timeout(timeouts.connect)
                .timeout(timeouts.request);
            match proxy.clone() {
                Some(proxy) => builder.proxy(proxy),
                None => builder.no_proxy(),
            }
            .build()
            .map_err(|error| RuntimeError::Connection(format!("HTTP client: {error}")))?
        };
        let async_client = {
            let builder = reqwest::Client::builder()
                .connect_timeout(timeouts.connect)
                .timeout(timeouts.request);
            match proxy {
                Some(proxy) => builder.proxy(proxy),
                None => builder.no_proxy(),
            }
            .build()
            .map_err(|error| RuntimeError::Connection(format!("HTTP client: {error}")))?
        };
        Ok(Self {
            client,
            async_client,
            timeouts,
            provider_messages,
        })
    }

    /// Adds the provider's own `error.message` to streaming HTTP errors. Meant for
    /// OpenAI-style gateways whose messages explain model access and quota problems.
    pub fn with_provider_messages(mut self) -> Self {
        self.provider_messages = true;
        self
    }

    pub fn get(&self, url: &str) -> Result<Response, RuntimeError> {
        self.get_with_headers(url, HeaderMap::new())
    }

    pub fn get_with_headers(
        &self,
        url: &str,
        headers: HeaderMap,
    ) -> Result<Response, RuntimeError> {
        let response = self
            .client
            .get(url)
            .headers(headers)
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
        let mut body = Vec::new();
        let _ = response
            .take(self.timeouts.max_error_body as u64)
            .read_to_end(&mut body);
        let message = provider_error_message(&body);
        crate::trace::runtime_event(
            "cloud_http_error",
            &format!(
                "status={} message={}",
                status.as_u16(),
                message.as_deref().unwrap_or("-")
            ),
        );
        let error = safe_http_error(status, &headers, &body);
        Err(match message {
            Some(message) if self.provider_messages => with_message(error, &message),
            _ => error,
        })
    }

    #[allow(dead_code)]
    pub fn stream_get<F>(
        &self,
        url: &str,
        headers: HeaderMap,
        cancellation: RequestCancellation,
        on_chunk: F,
    ) -> Result<(), RuntimeError>
    where
        F: FnMut(&[u8]) -> Result<(), RuntimeError> + Send + 'static,
    {
        let client = self.async_client.clone();
        let url = url.to_string();
        let timeouts = self.timeouts;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| RuntimeError::Connection(format!("HTTP runtime: {error}")))?;
        runtime.block_on(async move {
            tokio::time::timeout(
                timeouts.request,
                stream_get_async(client, url, headers, cancellation, timeouts, on_chunk),
            )
            .await
            .map_err(|_| RuntimeError::Timeout("cloud HTTP request timed out".into()))?
        })
    }

    pub fn stream_post_json<T, F>(
        &self,
        url: &str,
        headers: HeaderMap,
        body: &T,
        cancellation: RequestCancellation,
        on_chunk: F,
    ) -> Result<(), RuntimeError>
    where
        T: serde::Serialize,
        F: FnMut(&[u8]) -> Result<(), RuntimeError> + Send + 'static,
    {
        let client = self.async_client.clone();
        let url = url.to_string();
        let body = serde_json::to_vec(body)
            .map_err(|error| RuntimeError::InvalidInput(format!("request JSON: {error}")))?;
        let timeouts = self.timeouts;
        let provider_messages = self.provider_messages;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| RuntimeError::Connection(format!("HTTP runtime: {error}")))?;
        runtime.block_on(async move {
            tokio::time::timeout(
                timeouts.request,
                stream_post_json_async(
                    client,
                    url,
                    headers,
                    body,
                    cancellation,
                    timeouts,
                    provider_messages,
                    on_chunk,
                ),
            )
            .await
            .map_err(|_| RuntimeError::Timeout("cloud HTTP request timed out".into()))?
        })
    }
}

#[allow(dead_code)]
async fn stream_get_async<F>(
    client: reqwest::Client,
    url: String,
    headers: HeaderMap,
    cancellation: RequestCancellation,
    timeouts: HttpTimeouts,
    mut on_chunk: F,
) -> Result<(), RuntimeError>
where
    F: FnMut(&[u8]) -> Result<(), RuntimeError> + Send + 'static,
{
    let response = client
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(map_request_error)?;
    if !response.status().is_success() {
        let status = response.status();
        let headers = response.headers().clone();
        let body = read_bounded_body(response, cancellation.clone(), timeouts).await?;
        return Err(safe_http_error(status, &headers, &body));
    }

    let mut response = response;
    loop {
        if cancellation.is_cancelled() {
            return Err(RuntimeError::Cancelled);
        }
        let chunk = tokio::select! {
            _ = cancellation.cancelled() => return Err(RuntimeError::Cancelled),
            result = tokio::time::timeout(timeouts.idle, response.chunk()) => result
                .map_err(|_| RuntimeError::Timeout("cloud stream idle timeout".into()))?
                .map_err(map_request_error)?,
        };
        let Some(chunk) = chunk else {
            return Ok(());
        };
        on_chunk(&chunk)?;
    }
}

#[allow(clippy::too_many_arguments)]
async fn stream_post_json_async<F>(
    client: reqwest::Client,
    url: String,
    headers: HeaderMap,
    body: Vec<u8>,
    cancellation: RequestCancellation,
    timeouts: HttpTimeouts,
    provider_messages: bool,
    mut on_chunk: F,
) -> Result<(), RuntimeError>
where
    F: FnMut(&[u8]) -> Result<(), RuntimeError> + Send + 'static,
{
    let response = tokio::select! {
        _ = cancellation.cancelled() => return Err(RuntimeError::Cancelled),
        result = tokio::time::timeout(
            HEADER_TIMEOUT.min(timeouts.request),
            client.post(url).headers(headers).body(body).send(),
        ) => result
            .map_err(|_| RuntimeError::Timeout("provider did not respond in time".into()))?
            .map_err(map_request_error)?,
    };
    if !response.status().is_success() {
        let status = response.status();
        let headers = response.headers().clone();
        let body = read_bounded_body(response, cancellation.clone(), timeouts).await?;
        let message = provider_error_message(&body);
        crate::trace::runtime_event(
            "cloud_http_error",
            &format!(
                "status={} message={}",
                status.as_u16(),
                message.as_deref().unwrap_or("-")
            ),
        );
        let error = safe_http_error(status, &headers, &body);
        return Err(match message {
            Some(message) if provider_messages => with_message(error, &message),
            _ => error,
        });
    }
    let mut response = response;
    loop {
        let chunk = tokio::select! {
            _ = cancellation.cancelled() => return Err(RuntimeError::Cancelled),
            result = tokio::time::timeout(timeouts.idle, response.chunk()) => result
                .map_err(|_| RuntimeError::Timeout("cloud stream idle timeout".into()))?
                .map_err(map_request_error)?,
        };
        let Some(chunk) = chunk else {
            return Ok(());
        };
        on_chunk(&chunk)?;
    }
}

async fn read_bounded_body(
    mut response: reqwest::Response,
    cancellation: RequestCancellation,
    timeouts: HttpTimeouts,
) -> Result<Vec<u8>, RuntimeError> {
    let mut body = Vec::new();
    while body.len() < timeouts.max_error_body {
        let chunk = tokio::select! {
            _ = cancellation.cancelled() => return Err(RuntimeError::Cancelled),
            result = tokio::time::timeout(timeouts.idle, response.chunk()) => result
                .map_err(|_| RuntimeError::Timeout("cloud HTTP error body timed out".into()))?
                .map_err(map_request_error)?,
        };
        let Some(chunk) = chunk else { break };
        let remaining = timeouts.max_error_body - body.len();
        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }
    Ok(body)
}

#[allow(dead_code)]
pub fn read_sse_lines<R: BufRead, F: FnMut(&str) -> Result<(), RuntimeError>>(
    reader: R,
    cancellation: &RequestCancellation,
    mut on_data: F,
) -> Result<(), RuntimeError> {
    for line in reader.lines() {
        if cancellation.is_cancelled() {
            return Err(RuntimeError::Cancelled);
        }
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

/// Extracts a short, single-line `error.message` from a JSON error body.
pub fn provider_error_message(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let message = value
        .pointer("/error/message")
        .or_else(|| value.get("error"))
        .or_else(|| value.get("message"))?
        .as_str()?;
    let cleaned: String = message
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(200)
        .collect();
    let cleaned = cleaned.trim();
    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

fn with_message(error: RuntimeError, message: &str) -> RuntimeError {
    match error {
        RuntimeError::Authentication(detail) => {
            RuntimeError::Authentication(format!("{detail}: {message}"))
        }
        RuntimeError::Quota(detail) => RuntimeError::Quota(format!("{detail}: {message}")),
        RuntimeError::Connection(detail) => {
            RuntimeError::Connection(format!("{detail}: {message}"))
        }
        RuntimeError::RateLimited {
            detail,
            retry_after,
        } => RuntimeError::RateLimited {
            detail: format!("{detail}: {message}"),
            retry_after,
        },
        RuntimeError::Http { status, detail } => RuntimeError::Http {
            status,
            detail: format!("{detail}: {message}"),
        },
        other => other,
    }
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
    let mut body = Vec::new();
    let _ = response.take(cap as u64).read_to_end(&mut body);
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
    use std::io::{Read, Write};
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
        assert!(matches!(error, RuntimeError::Timeout(_)), "{error:?}");
    }

    #[test]
    fn get_with_headers_sends_provider_authentication() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 4096];
            let count = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..count]).into_owned();
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\n{\"models\":[]}",
                )
                .unwrap();
            request
        });

        let transport = HttpTransport::new().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-goog-api-key", "secret".parse().unwrap());
        let response = transport
            .get_with_headers(&format!("http://{address}/models"), headers)
            .unwrap();
        let _ = response.text();

        let request = server.join().unwrap();
        assert!(request.contains("x-goog-api-key: secret"));
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
    fn provider_error_details_are_bounded() {
        let headers = reqwest::header::HeaderMap::new();
        let body = format!("provider detail {}", "x".repeat(2_000));
        let error = safe_http_error(reqwest::StatusCode::BAD_REQUEST, &headers, body.as_bytes());
        let rendered = error.to_string();
        assert_eq!(
            rendered,
            "runtime returned HTTP 400: provider returned HTTP 400"
        );
        assert!(!rendered.contains('x'));
    }

    #[test]
    fn non_auth_provider_bodies_never_reach_user_errors() {
        let headers = reqwest::header::HeaderMap::new();
        for status in [
            reqwest::StatusCode::BAD_REQUEST,
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        ] {
            let error = safe_http_error(status, &headers, b"api_key=super-secret source text");
            assert!(!error.to_string().contains("super-secret"));
            assert!(!error.to_string().contains("source text"));
        }
    }

    #[test]
    fn proxy_must_be_an_http_url() {
        let transport = HttpTransport::new().unwrap();
        assert!(transport.with_proxy("http://192.168.0.12:9102").is_ok());
        assert!(transport.with_proxy("").is_ok());
        assert!(transport.with_proxy("not a url").is_err());
        assert!(transport.with_proxy("socks5://127.0.0.1:1080").is_ok());
        assert!(transport.with_proxy("ftp://host:1").is_err());
    }

    #[test]
    fn provider_error_message_is_short_and_single_line() {
        let body = br#"{"error":{"message":"No endpoints found\nfor this model","code":404}}"#;
        assert_eq!(
            provider_error_message(body).as_deref(),
            Some("No endpoints found for this model")
        );
        let long = format!(r#"{{"error":"{}"}}"#, "x".repeat(500));
        assert_eq!(provider_error_message(long.as_bytes()).unwrap().len(), 200);
        assert!(provider_error_message(b"<html>blocked</html>").is_none());
    }

    #[test]
    fn sse_reader_passes_only_data_lines_and_skips_done_marker() {
        let input = "event: message\ndata: {\"text\":\"one\"}\n\ndata: [DONE]\n";
        let mut events = Vec::new();
        read_sse_lines(input.as_bytes(), &RequestCancellation::default(), |event| {
            events.push(event.to_string());
            Ok(())
        })
        .unwrap();
        assert_eq!(events, vec![r#"{"text":"one"}"#]);
    }

    #[test]
    fn stalled_stream_returns_idle_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: 100\r\n\r\nx",
                )
                .unwrap();
            stream.flush().unwrap();
            thread::sleep(Duration::from_millis(200));
        });
        let transport = HttpTransport::with_timeouts(HttpTimeouts {
            connect: Duration::from_millis(50),
            idle: Duration::from_millis(50),
            request: Duration::from_secs(1),
            max_error_body: 1024,
        })
        .unwrap();
        let error = transport
            .stream_get(
                &format!("http://{address}/"),
                HeaderMap::new(),
                RequestCancellation::default(),
                |_| Ok(()),
            )
            .unwrap_err();
        assert!(matches!(error, RuntimeError::Timeout(_)), "{error:?}");
    }

    #[test]
    fn cancellation_stops_a_stalled_stream() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (ready_sender, ready_receiver) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 100\r\n\r\nx")
                .unwrap();
            stream.flush().unwrap();
            ready_sender.send(()).unwrap();
            thread::sleep(Duration::from_millis(300));
        });
        let transport = std::sync::Arc::new(
            HttpTransport::with_timeouts(HttpTimeouts {
                connect: Duration::from_millis(50),
                idle: Duration::from_secs(1),
                request: Duration::from_secs(2),
                max_error_body: 1024,
            })
            .unwrap(),
        );
        let cancellation = RequestCancellation::default();
        let worker_cancellation = cancellation.clone();
        let worker_transport = transport.clone();
        let worker = thread::spawn(move || {
            worker_transport.stream_get(
                &format!("http://{address}/"),
                HeaderMap::new(),
                worker_cancellation,
                |_| Ok(()),
            )
        });
        ready_receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        cancellation.cancel();
        let result = worker.join().unwrap();
        assert!(matches!(result, Err(RuntimeError::Cancelled)), "{result:?}");
    }

    #[allow(dead_code)]
    fn _write_headers(stream: &mut TcpStream) {
        stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
    }
}
