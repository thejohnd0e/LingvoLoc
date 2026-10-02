use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

#[derive(Clone, Debug)]
pub struct RequestCancellation {
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl Default for RequestCancellation {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        }
    }
}

impl RequestCancellation {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    #[allow(dead_code)]
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    pub async fn cancelled(&self) {
        let mut notified = std::pin::pin!(self.notify.notified());
        notified.as_mut().enable();
        if self.is_cancelled() {
            return;
        }
        notified.await;
    }
}

#[derive(Debug, Default)]
pub struct RequestRegistry {
    active: Mutex<HashMap<String, RequestCancellation>>,
    next_id: AtomicU64,
}

impl RequestRegistry {
    pub fn start_generated(&self, prefix: &str) -> (String, RequestCancellation) {
        let request_id = format!("{prefix}-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let cancellation = self.start(&request_id);
        (request_id, cancellation)
    }

    #[allow(dead_code)]
    pub fn cancel_all(&self) {
        let Ok(active) = self.active.lock() else {
            return;
        };
        for cancellation in active.values() {
            cancellation.cancel();
        }
    }
}

impl RequestRegistry {
    pub fn start(&self, request_id: &str) -> RequestCancellation {
        let cancellation = RequestCancellation::default();
        if let Ok(mut active) = self.active.lock() {
            active.insert(request_id.into(), cancellation.clone());
        }
        cancellation
    }

    #[allow(dead_code)]
    pub fn cancel(&self, request_id: &str) -> bool {
        let Ok(active) = self.active.lock() else {
            return false;
        };
        let Some(cancellation) = active.get(request_id) else {
            return false;
        };
        cancellation.cancel();
        true
    }

    /// Cancels every active request whose id starts with `prefix`.
    pub fn cancel_prefix(&self, prefix: &str) -> usize {
        let Ok(active) = self.active.lock() else {
            return 0;
        };
        let mut count = 0;
        for (id, cancellation) in active.iter() {
            if id.starts_with(prefix) {
                cancellation.cancel();
                count += 1;
            }
        }
        count
    }

    pub fn finish(&self, request_id: &str) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(request_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_cancels_and_removes_the_active_request() {
        let registry = RequestRegistry::default();
        let request = registry.start("job-1");
        assert!(!request.is_cancelled());
        assert!(registry.cancel("job-1"));
        assert!(request.is_cancelled());
        registry.finish("job-1");
        assert!(!registry.cancel("job-1"));
    }

    #[test]
    fn cancel_prefix_only_cancels_matching_requests() {
        let registry = RequestRegistry::default();
        let (_, interactive) = registry.start_generated("interactive");
        let (_, api) = registry.start_generated("api");
        assert_eq!(registry.cancel_prefix("interactive-"), 1);
        assert!(interactive.is_cancelled());
        assert!(!api.is_cancelled());
    }

    #[test]
    fn generated_request_ids_are_unique() {
        let registry = RequestRegistry::default();
        let mut ids = std::collections::HashSet::new();
        for _ in 0..100 {
            let (id, _) = registry.start_generated("interactive");
            assert!(ids.insert(id));
        }
    }
}
