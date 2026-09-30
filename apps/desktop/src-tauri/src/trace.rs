use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Name and time of the most recent runtime event, for stall diagnostics.
static LAST_EVENT: Mutex<Option<(String, u128)>> = Mutex::new(None);

/// Describes the most recent runtime event and how long ago it happened.
pub fn last_runtime_event() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    match LAST_EVENT.lock().ok().and_then(|guard| guard.clone()) {
        Some((event, at)) => format!("last_event={event} age_ms={}", now.saturating_sub(at)),
        None => "last_event=none".into(),
    }
}

/// Appends one diagnostic line to the shared worker log so document-worker events
/// and runtime/HTTP events can be correlated by timestamp.
pub fn runtime_event(event: &str, detail: &str) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    if let Ok(mut last) = LAST_EVENT.lock() {
        *last = Some((event.to_string(), timestamp));
    }
    let line = format!("{timestamp} job=runtime block=-1 event={event} {detail}\n");
    let path = std::env::temp_dir().join("lingvoloc-document-worker.log");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}
