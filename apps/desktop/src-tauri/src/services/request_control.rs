use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct RequestCancellation {
    cancelled: Arc<AtomicBool>,
}

impl Default for RequestCancellation {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl RequestCancellation {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

#[derive(Debug, Default)]
pub struct RequestRegistry {
    active: Mutex<HashMap<String, RequestCancellation>>,
}

impl RequestRegistry {
    pub fn start(&self, request_id: &str) -> RequestCancellation {
        let cancellation = RequestCancellation::default();
        if let Ok(mut active) = self.active.lock() {
            active.insert(request_id.into(), cancellation.clone());
        }
        cancellation
    }

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
}
