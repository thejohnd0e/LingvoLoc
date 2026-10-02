use crate::domain::{RuntimeError, RuntimeMode, Settings};
use std::sync::{Condvar, Mutex};

#[derive(Debug, Default)]
struct CoordinatorState {
    active: bool,
    interactive_waiters: usize,
    desired_snapshot: Option<String>,
    lifecycle_busy: bool,
    shutting_down: bool,
}

/// Serializes all model requests without holding application settings or history locks.
/// Interactive callers have priority whenever the current request finishes.
#[derive(Debug, Default)]
pub struct InferenceCoordinator {
    state: Mutex<CoordinatorState>,
    wake: Condvar,
}

impl InferenceCoordinator {
    pub fn run_interactive<T, F>(&self, snapshot: &str, operation: F) -> Result<T, RuntimeError>
    where
        F: FnOnce() -> Result<T, RuntimeError>,
    {
        let mut state = self.lock_state()?;
        if state.shutting_down {
            return Err(shutdown_error());
        }
        state.desired_snapshot = Some(snapshot.to_string());
        state.interactive_waiters += 1;
        while (state.active || state.lifecycle_busy) && !state.shutting_down {
            state = self.wait(state)?;
        }
        state.interactive_waiters -= 1;
        if state.shutting_down {
            return Err(shutdown_error());
        }
        state.active = true;
        drop(state);

        let result = operation();
        let mut state = self.lock_state()?;
        state.active = false;
        self.wake.notify_all();
        result
    }

    /// Runs one document block. Callers should invoke this once per block so an
    /// interactive request can run before the next block is scheduled.
    pub fn run_background<T, F>(&self, snapshot: &str, operation: F) -> Result<T, RuntimeError>
    where
        F: FnOnce() -> Result<T, RuntimeError>,
    {
        let mut state = self.lock_state()?;
        while ((state.active || state.lifecycle_busy) && !state.shutting_down)
            || state.interactive_waiters > 0
        {
            if state.shutting_down {
                return Err(shutdown_error());
            }
            state = self.wait(state)?;
        }
        if state.shutting_down {
            return Err(shutdown_error());
        }
        if let Some(desired) = &state.desired_snapshot {
            if desired != snapshot {
                return Err(model_changed_error());
            }
        } else {
            state.desired_snapshot = Some(snapshot.to_string());
        }
        state.active = true;
        drop(state);

        let result = operation();
        let mut state = self.lock_state()?;
        state.active = false;
        self.wake.notify_all();
        result
    }

    /// Quiesces inference for runtime lifecycle work such as replacing
    /// llama.cpp. Unlike shutdown, the coordinator remains usable afterward.
    pub fn run_lifecycle<T, F>(&self, operation: F) -> Result<T, RuntimeError>
    where
        F: FnOnce() -> Result<T, RuntimeError>,
    {
        let mut state = self.lock_state()?;
        while (state.lifecycle_busy || state.interactive_waiters > 0) && !state.shutting_down {
            state = self.wait(state)?;
        }
        if state.shutting_down {
            return Err(shutdown_error());
        }
        state.lifecycle_busy = true;
        self.wake.notify_all();
        while (state.active || state.interactive_waiters > 0) && !state.shutting_down {
            state = self.wait(state)?;
        }
        if state.shutting_down {
            state.lifecycle_busy = false;
            self.wake.notify_all();
            return Err(shutdown_error());
        }
        drop(state);

        let result = operation();
        let mut state = self.lock_state()?;
        state.lifecycle_busy = false;
        self.wake.notify_all();
        result
    }

    pub fn set_desired_snapshot(&self, snapshot: &str) -> Result<(), RuntimeError> {
        let mut state = self.lock_state()?;
        state.desired_snapshot = Some(snapshot.to_string());
        self.wake.notify_all();
        Ok(())
    }

    /// Stops accepting new work without waiting for an active blocking request.
    pub fn begin_shutdown(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.shutting_down = true;
        self.wake.notify_all();
    }

    pub fn wait_for_idle(&self) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        while state.active || state.lifecycle_busy {
            state = match self.wake.wait(state) {
                Ok(state) => state,
                Err(_) => return,
            };
        }
    }

    /// Stops accepting new work and waits for the active blocking request to finish.
    pub fn shutdown(&self) {
        self.begin_shutdown();
        self.wait_for_idle();
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, CoordinatorState>, RuntimeError> {
        self.state
            .lock()
            .map_err(|_| RuntimeError::Connection("inference coordinator lock is poisoned".into()))
    }

    fn wait<'a>(
        &self,
        state: std::sync::MutexGuard<'a, CoordinatorState>,
    ) -> Result<std::sync::MutexGuard<'a, CoordinatorState>, RuntimeError> {
        self.wake
            .wait(state)
            .map_err(|_| RuntimeError::Connection("inference coordinator lock is poisoned".into()))
    }
}

pub fn snapshot(settings: &Settings, model_id: &str) -> String {
    let mode = match settings.runtime_mode {
        RuntimeMode::LmStudio => "lmStudio",
        RuntimeMode::Standalone => "standalone",
        RuntimeMode::OpenAi => "openAi",
        RuntimeMode::Anthropic => "anthropic",
        RuntimeMode::Gemini => "gemini",
        RuntimeMode::DeepL => "deepL",
        RuntimeMode::OpenAiCompatible => "openAiCompatible",
        RuntimeMode::DeepSeek => "deepSeek",
        RuntimeMode::OpenRouter => "openRouter",
    };
    if !matches!(
        settings.runtime_mode,
        RuntimeMode::LmStudio | RuntimeMode::Standalone
    ) {
        let endpoint_category = match settings.runtime_mode {
            RuntimeMode::OpenAi => "openai",
            RuntimeMode::Anthropic => "anthropic",
            RuntimeMode::Gemini => "gemini",
            RuntimeMode::DeepL => {
                if settings.cloud.deep_l.plan == "pro" {
                    "deepl-pro"
                } else {
                    "deepl-free"
                }
            }
            RuntimeMode::OpenAiCompatible => "openai-compatible",
            RuntimeMode::DeepSeek => "deepseek",
            RuntimeMode::OpenRouter => "openrouter",
            RuntimeMode::LmStudio | RuntimeMode::Standalone => unreachable!(),
        };
        return format!("{mode}|{endpoint_category}|{model_id}");
    }
    format!(
        "{mode}|{model_id}|{}|{}|{}",
        settings.endpoint, settings.models_directory, settings.llama_server_path
    )
}

fn shutdown_error() -> RuntimeError {
    RuntimeError::Connection("inference coordinator is shutting down".into())
}

fn model_changed_error() -> RuntimeError {
    RuntimeError::InvalidInput(
        "document paused: runtime or model configuration changed; resume explicitly".into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::Duration;

    fn ok(value: &'static str) -> Result<&'static str, RuntimeError> {
        Ok(value)
    }

    #[test]
    fn interactive_work_runs_before_the_next_background_block() {
        let coordinator = Arc::new(InferenceCoordinator::default());
        let first_done = Arc::new(Barrier::new(2));
        let order = Arc::new(Mutex::new(Vec::new()));
        let worker_coordinator = Arc::clone(&coordinator);
        let worker_done = Arc::clone(&first_done);
        let worker_order = Arc::clone(&order);
        let worker = thread::spawn(move || {
            worker_coordinator
                .run_background("model-a", || {
                    worker_order.lock().unwrap().push("block-1");
                    worker_done.wait();
                    ok("block-1")
                })
                .unwrap();
            worker_coordinator
                .run_background("model-a", || {
                    worker_order.lock().unwrap().push("block-2");
                    ok("block-2")
                })
                .unwrap();
        });
        while order.lock().unwrap().is_empty() {
            thread::yield_now();
        }
        let interactive_order = Arc::clone(&order);
        let interactive = thread::spawn(move || {
            coordinator
                .run_interactive("model-a", || {
                    interactive_order.lock().unwrap().push("interactive");
                    ok("interactive")
                })
                .unwrap();
        });
        thread::sleep(Duration::from_millis(10));
        first_done.wait();
        interactive.join().unwrap();
        worker.join().unwrap();
        assert_eq!(
            *order.lock().unwrap(),
            vec!["block-1", "interactive", "block-2"]
        );
    }

    #[test]
    fn background_work_is_rejected_after_a_model_change() {
        let coordinator = InferenceCoordinator::default();
        coordinator
            .run_background("model-a", || ok("block"))
            .unwrap();
        coordinator
            .run_interactive("model-b", || ok("interactive"))
            .unwrap();
        assert!(coordinator
            .run_background("model-a", || ok("stale"))
            .is_err());
    }

    #[test]
    fn runtime_errors_are_returned_and_do_not_poison_the_gate() {
        let coordinator = InferenceCoordinator::default();
        let error = coordinator
            .run_background("model-a", || {
                Err::<(), _>(RuntimeError::Timeout("mock timeout".into()))
            })
            .unwrap_err();
        assert_eq!(error, RuntimeError::Timeout("mock timeout".into()));
        assert_eq!(
            coordinator
                .run_interactive("model-a", || ok("interactive"))
                .unwrap(),
            "interactive"
        );
    }

    #[test]
    fn shutdown_waits_for_active_work_and_rejects_later_work() {
        let coordinator = Arc::new(InferenceCoordinator::default());
        let barrier = Arc::new(Barrier::new(2));
        let worker_coordinator = Arc::clone(&coordinator);
        let worker_barrier = Arc::clone(&barrier);
        let worker = thread::spawn(move || {
            worker_coordinator
                .run_interactive("model-a", || {
                    worker_barrier.wait();
                    thread::sleep(Duration::from_millis(20));
                    ok("done")
                })
                .unwrap();
        });
        barrier.wait();
        coordinator.shutdown();
        worker.join().unwrap();
        assert!(coordinator
            .run_interactive("model-a", || ok("late"))
            .is_err());
    }

    #[test]
    fn begin_shutdown_rejects_new_work_before_active_work_drains() {
        let coordinator = Arc::new(InferenceCoordinator::default());
        let active = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_coordinator = Arc::clone(&coordinator);
        let worker_active = Arc::clone(&active);
        let worker_release = Arc::clone(&release);
        let worker = thread::spawn(move || {
            worker_coordinator
                .run_interactive("model-a", || {
                    worker_active.wait();
                    worker_release.wait();
                    ok("done")
                })
                .unwrap();
        });

        active.wait();
        coordinator.begin_shutdown();
        assert!(coordinator
            .run_interactive("model-a", || ok("late"))
            .is_err());
        release.wait();
        coordinator.wait_for_idle();
        worker.join().unwrap();
    }

    #[test]
    fn lifecycle_work_quiesces_inference_without_disabling_it() {
        let coordinator = InferenceCoordinator::default();
        assert_eq!(
            coordinator.run_lifecycle(|| ok("updated")).unwrap(),
            "updated"
        );
        assert_eq!(
            coordinator
                .run_interactive("model-a", || ok("usable"))
                .unwrap(),
            "usable"
        );
    }

    #[test]
    fn settings_snapshot_change_rejects_the_old_background_snapshot() {
        let coordinator = InferenceCoordinator::default();
        coordinator
            .run_background("model-a", || ok("first"))
            .unwrap();
        coordinator.set_desired_snapshot("model-b").unwrap();
        assert!(coordinator
            .run_background("model-a", || ok("stale"))
            .is_err());
    }

    #[test]
    fn standalone_snapshot_format_remains_unchanged() {
        let settings = Settings {
            runtime_mode: RuntimeMode::Standalone,
            endpoint: "http://127.0.0.1:1234/v1".into(),
            models_directory: "D:/models".into(),
            llama_server_path: "D:/llama/llama-server.exe".into(),
            ..serde_json::from_str(
                r#"{
                    "endpoint": "",
                    "modelId": "",
                    "adapterId": "",
                    "sourceLanguage": "auto",
                    "targetLanguage": "ru",
                    "primaryLanguage": "en",
                    "secondaryLanguage": "ru"
                }"#,
            )
            .unwrap()
        };

        assert_eq!(
            snapshot(&settings, "model.gguf"),
            "standalone|model.gguf|http://127.0.0.1:1234/v1|D:/models|D:/llama/llama-server.exe"
        );
    }

    #[test]
    fn cloud_snapshot_contains_provider_and_model_but_not_credentials() {
        let mut settings: Settings = serde_json::from_str(
            r#"{
                "runtimeMode": "openAi",
                "endpoint": "https://api.openai.com/v1",
                "modelId": "gpt-4o-mini",
                "adapterId": "openai",
                "sourceLanguage": "auto",
                "targetLanguage": "ru",
                "primaryLanguage": "en",
                "secondaryLanguage": "ru"
            }"#,
        )
        .unwrap();
        settings.cloud.open_ai.model_id = "gpt-4o-mini".into();

        let value = snapshot(&settings, "gpt-4o-mini");
        assert!(value.contains("openAi|openai|gpt-4o-mini"));
        assert!(!value.contains("sk-test-secret"));
    }
}
