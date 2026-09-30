use super::segmentation::{segment_block, SegmentationLimits};
use super::{DocumentJobStore, JobState};
use crate::domain::{RuntimeError, Settings, TranslationRequest};
use crate::services::inference_coordinator::InferenceCoordinator;
use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

fn format_trace_event(job_id: &str, ordinal: i64, event: &str, detail: &str) -> String {
    format!("job={job_id} block={ordinal} event={event} {detail}")
}

fn trace_document(job_id: &str, ordinal: i64, event: &str, detail: &str) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let line = format!(
        "{timestamp} {}\n",
        format_trace_event(job_id, ordinal, event, detail)
    );
    let path = std::env::temp_dir().join("lingvoloc-document-worker.log");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WorkerReport {
    pub translated_blocks: usize,
    pub skipped_blocks: usize,
    pub paused: bool,
    pub cancelled: bool,
}

/// Translates persisted blocks one at a time. The translator is injected so
/// recovery and failure behavior can be tested without a model or network.
pub fn translate_job_with<F>(
    store: &mut DocumentJobStore,
    coordinator: &InferenceCoordinator,
    job_id: &str,
    settings: &Settings,
    snapshot: &str,
    limits: SegmentationLimits,
    mut translator: F,
) -> Result<WorkerReport, RuntimeError>
where
    F: FnMut(&TranslationRequest) -> Result<String, RuntimeError>,
{
    let job = store
        .get(job_id)?
        .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
    if !matches!(job.state, JobState::Translating) {
        return Err(RuntimeError::InvalidInput(format!(
            "document job must be translating, got {:?}",
            job.state
        )));
    }

    let blocks = store.blocks(job_id)?;
    let mut report = WorkerReport::default();
    trace_document(
        job_id,
        -1,
        "worker_start",
        &format!("blocks={}", blocks.len()),
    );
    for block in blocks {
        let current_state = store
            .get(job_id)?
            .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?
            .state;
        if current_state == JobState::Pausing {
            trace_document(job_id, block.ordinal, "pause_detected", "");
            store.transition(job_id, JobState::Paused, Some("pause requested"))?;
            report.paused = true;
            return Ok(report);
        }
        if current_state == JobState::Cancelled {
            trace_document(job_id, block.ordinal, "cancel_detected", "");
            report.cancelled = true;
            return Ok(report);
        }
        if block.translated_text.is_some() {
            report.skipped_blocks += 1;
            continue;
        }
        trace_document(
            job_id,
            block.ordinal,
            "block_start",
            &format!("chars={}", block.source_text.chars().count()),
        );
        let segments = match segment_block(&block, limits) {
            Ok(segments) => segments,
            Err(error) => {
                record_error(store, job_id, &error);
                return Err(error);
            }
        };
        if segments.len() > 1 {
            store.replace_block(job_id, &block.id, &segments)?;
        }
        for mut segment in segments {
            trace_document(
                job_id,
                segment.ordinal,
                "request_start",
                &format!("chars={}", segment.source_text.chars().count()),
            );
            let request_started = std::time::Instant::now();
            let request = TranslationRequest {
                model_id: settings.model_id.clone(),
                adapter_id: settings.adapter_id.clone(),
                source_language: job.source_language.clone(),
                target_language: job.target_language.clone(),
                text: segment.source_text.clone(),
            };
            // Logs a warning while a single block request keeps running, with the
            // most recent runtime event, so a stall shows where it stopped.
            let (finished, watchdog) = std::sync::mpsc::channel::<()>();
            let watch_job = job_id.to_string();
            let watch_ordinal = segment.ordinal;
            let watcher = std::thread::spawn(move || {
                let started = std::time::Instant::now();
                while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
                    watchdog.recv_timeout(std::time::Duration::from_secs(20))
                {
                    trace_document(
                        &watch_job,
                        watch_ordinal,
                        "request_slow",
                        &format!(
                            "waited_s={} {}",
                            started.elapsed().as_secs(),
                            crate::trace::last_runtime_event()
                        ),
                    );
                }
            });
            let translated_result = coordinator.run_background(snapshot, || {
                trace_document(job_id, segment.ordinal, "coordinator_acquired", "");
                translator(&request)
            });
            drop(finished);
            let _ = watcher.join();
            trace_document(
                job_id,
                segment.ordinal,
                "request_end",
                &format!(
                    "elapsed_ms={} result={}",
                    request_started.elapsed().as_millis(),
                    if translated_result.is_ok() {
                        "ok"
                    } else {
                        "error"
                    }
                ),
            );
            let translated = match translated_result {
                Ok(translated) => translated,
                Err(error) => {
                    match store.get(job_id)?.map(|job| job.state) {
                        Some(JobState::Pausing) => {
                            store.transition(job_id, JobState::Paused, Some("pause requested"))?;
                            report.paused = true;
                            return Ok(report);
                        }
                        Some(JobState::Cancelled) => {
                            report.cancelled = true;
                            return Ok(report);
                        }
                        None => return Err(error),
                        _ => {}
                    }
                    record_error(store, job_id, &error);
                    return Err(error);
                }
            };
            if translated.trim().is_empty() {
                let error =
                    RuntimeError::MalformedResponse("model returned empty document output".into());
                record_error(store, job_id, &error);
                return Err(error);
            }
            segment.translated_text = Some(translated);
            store.save_block(job_id, &segment)?;
            trace_document(job_id, segment.ordinal, "block_saved", "");
            report.translated_blocks += 1;
        }
    }
    Ok(report)
}

fn record_error(store: &mut DocumentJobStore, job_id: &str, error: &RuntimeError) {
    let detail = error.to_string();
    if matches!(error, RuntimeError::InvalidInput(message) if message.starts_with("document paused:"))
    {
        if store
            .transition(job_id, JobState::Pausing, Some(&detail))
            .is_ok()
        {
            let _ = store.transition(job_id, JobState::Paused, Some(&detail));
        }
    } else {
        let _ = store.transition(job_id, JobState::Failed, Some(&detail));
    }
}

pub fn translate_job(
    store: &mut DocumentJobStore,
    coordinator: &InferenceCoordinator,
    job_id: &str,
    settings: &Settings,
    snapshot: &str,
    limits: SegmentationLimits,
) -> Result<WorkerReport, RuntimeError> {
    translate_job_with(
        store,
        coordinator,
        job_id,
        settings,
        snapshot,
        limits,
        |request| {
            crate::services::translation::translate(settings, request.clone())
                .map(|result| result.text)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::store::content_hash;
    use crate::documents::{BlockType, DocumentBlock, DocumentJob};
    use std::path::PathBuf;

    fn settings() -> Settings {
        Settings {
            runtime_mode: Default::default(),
            models_directory: String::new(),
            llama_server_path: String::new(),
            endpoint: "http://127.0.0.1:1234/v1".into(),
            model_id: "model-a".into(),
            adapter_id: "adapter".into(),
            source_language: "en".into(),
            target_language: "ru".into(),
            primary_language: "en".into(),
            secondary_language: "ru".into(),
        }
    }

    fn store() -> DocumentJobStore {
        let store = DocumentJobStore::in_memory().unwrap();
        let job = DocumentJob {
            id: "job-1".into(),
            source_path: "book.txt".into(),
            source_hash: content_hash(b"source"),
            format: "txt".into(),
            parser_version: "1".into(),
            source_language: "en".into(),
            target_language: "ru".into(),
            runtime_snapshot: "model-a".into(),
            configuration_version: "1".into(),
            state: JobState::Queued,
            error: None,
        };
        store.create(&job).unwrap();
        store
            .transition("job-1", JobState::Analyzing, None)
            .unwrap();
        store.transition("job-1", JobState::Ready, None).unwrap();
        store
            .transition("job-1", JobState::Translating, None)
            .unwrap();
        store
    }

    fn persisted_store(label: &str) -> (DocumentJobStore, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "lingvoloc-worker-{label}-{}-{}.sqlite",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = DocumentJobStore::open(&path).unwrap();
        let job = DocumentJob {
            id: "job-1".into(),
            source_path: "book.txt".into(),
            source_hash: content_hash(b"source"),
            format: "txt".into(),
            parser_version: "1".into(),
            source_language: "en".into(),
            target_language: "ru".into(),
            runtime_snapshot: "model-a".into(),
            configuration_version: "1".into(),
            state: JobState::Queued,
            error: None,
        };
        store.create(&job).unwrap();
        store
            .transition("job-1", JobState::Analyzing, None)
            .unwrap();
        store.transition("job-1", JobState::Ready, None).unwrap();
        store
            .transition("job-1", JobState::Translating, None)
            .unwrap();
        (store, path)
    }

    #[test]
    fn formats_trace_events_without_source_text() {
        assert_eq!(
            format_trace_event("job-1", 54, "request_start", "chars=24"),
            "job=job-1 block=54 event=request_start chars=24"
        );
    }

    #[test]
    fn saves_each_success_and_keeps_previous_results_when_a_later_block_fails() {
        let mut store = store();
        store
            .save_block(
                "job-1",
                &DocumentBlock {
                    id: "one".into(),
                    ordinal: 1,
                    block_type: BlockType::Paragraph,
                    source_text: "first".into(),
                    translated_text: None,
                },
            )
            .unwrap();
        store
            .save_block(
                "job-1",
                &DocumentBlock {
                    id: "two".into(),
                    ordinal: 2,
                    block_type: BlockType::Paragraph,
                    source_text: "second".into(),
                    translated_text: None,
                },
            )
            .unwrap();
        let mut calls = 0;
        let error = translate_job_with(
            &mut store,
            &InferenceCoordinator::default(),
            "job-1",
            &settings(),
            "model-a",
            SegmentationLimits::default(),
            |_| {
                calls += 1;
                if calls == 2 {
                    Err(RuntimeError::Timeout("mock timeout".into()))
                } else {
                    Ok("translated".into())
                }
            },
        )
        .unwrap_err();
        assert_eq!(error, RuntimeError::Timeout("mock timeout".into()));
        let blocks = store.blocks("job-1").unwrap();
        assert_eq!(
            blocks
                .iter()
                .filter(|block| block.translated_text.is_some())
                .count(),
            1
        );
        assert_eq!(store.get("job-1").unwrap().unwrap().state, JobState::Failed);
    }

    #[test]
    fn skips_already_translated_blocks() {
        let mut store = store();
        store
            .save_block(
                "job-1",
                &DocumentBlock {
                    id: "one".into(),
                    ordinal: 1,
                    block_type: BlockType::Paragraph,
                    source_text: "first".into(),
                    translated_text: Some("ready".into()),
                },
            )
            .unwrap();
        let report = translate_job_with(
            &mut store,
            &InferenceCoordinator::default(),
            "job-1",
            &settings(),
            "model-a",
            SegmentationLimits::default(),
            |_| panic!("translated blocks must not be sent again"),
        )
        .unwrap();
        assert_eq!(
            report,
            WorkerReport {
                translated_blocks: 0,
                skipped_blocks: 1,
                ..WorkerReport::default()
            }
        );
    }

    #[test]
    fn request_error_after_pause_finishes_the_job_as_paused() {
        let (mut store, path) = persisted_store("pause");
        store
            .save_block(
                "job-1",
                &DocumentBlock {
                    id: "one".into(),
                    ordinal: 1,
                    block_type: BlockType::Paragraph,
                    source_text: "first".into(),
                    translated_text: None,
                },
            )
            .unwrap();

        let report = translate_job_with(
            &mut store,
            &InferenceCoordinator::default(),
            "job-1",
            &settings(),
            "model-a",
            SegmentationLimits::default(),
            |_| {
                DocumentJobStore::open(&path)
                    .unwrap()
                    .transition("job-1", JobState::Pausing, Some("pause requested"))
                    .unwrap();
                Err(RuntimeError::Connection("request interrupted".into()))
            },
        )
        .unwrap();

        assert!(report.paused);
        assert_eq!(store.get("job-1").unwrap().unwrap().state, JobState::Paused);
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn request_error_after_cancel_keeps_the_job_cancelled() {
        let (mut store, path) = persisted_store("cancel");
        store
            .save_block(
                "job-1",
                &DocumentBlock {
                    id: "one".into(),
                    ordinal: 1,
                    block_type: BlockType::Paragraph,
                    source_text: "first".into(),
                    translated_text: None,
                },
            )
            .unwrap();

        let report = translate_job_with(
            &mut store,
            &InferenceCoordinator::default(),
            "job-1",
            &settings(),
            "model-a",
            SegmentationLimits::default(),
            |_| {
                DocumentJobStore::open(&path)
                    .unwrap()
                    .transition("job-1", JobState::Cancelled, Some("cancelled by user"))
                    .unwrap();
                Err(RuntimeError::Connection("request interrupted".into()))
            },
        )
        .unwrap();

        assert!(report.cancelled);
        assert_eq!(
            store.get("job-1").unwrap().unwrap().state,
            JobState::Cancelled
        );
        drop(store);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn persists_subdivided_blocks_in_reading_order() {
        let mut store = store();
        store
            .save_block(
                "job-1",
                &DocumentBlock {
                    id: "one".into(),
                    ordinal: 1,
                    block_type: BlockType::Paragraph,
                    source_text: "First sentence. Second sentence. Third sentence.".into(),
                    translated_text: None,
                },
            )
            .unwrap();
        let report = translate_job_with(
            &mut store,
            &InferenceCoordinator::default(),
            "job-1",
            &settings(),
            "model-a",
            SegmentationLimits {
                context_characters: 24,
                prompt_reserve: 4,
                output_reserve: 4,
            },
            |request| Ok(format!("translated: {}", request.text)),
        )
        .unwrap();
        let blocks = store.blocks("job-1").unwrap();
        assert_eq!(report.translated_blocks, 3);
        assert_eq!(blocks.len(), 3);
        assert!(blocks
            .windows(2)
            .all(|pair| pair[0].ordinal < pair[1].ordinal));
        assert!(blocks.iter().all(|block| block.translated_text.is_some()));
    }
}
