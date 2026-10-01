use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::documents::fb2 as format;
use crate::documents::{DocumentBlock, DocumentJob, DocumentJobStore, JobState};
use crate::domain::RuntimeError;
use crate::services::inference_coordinator::snapshot;
use tauri::AppHandle;

use super::{
    create_temporary_output, default_output_path, new_job_id, open_store,
    resolve_document_languages, run_job, view, DocumentExport, DocumentJobView,
};

fn validate_source_path(source: &Path) -> Result<(), RuntimeError> {
    if source
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        != Some("fb2".into())
    {
        return Err(RuntimeError::InvalidInput("select an .fb2 file".into()));
    }
    Ok(())
}

fn validate_export(
    job: &DocumentJob,
    source: &[u8],
    blocks: &[DocumentBlock],
) -> Result<(), RuntimeError> {
    if job.format != "fb2" {
        return Err(RuntimeError::InvalidInput("document job is not FB2".into()));
    }
    if !DocumentJobStore::source_is_current(job, source) {
        return Err(RuntimeError::InvalidInput(
            "document source changed; analyze it again".into(),
        ));
    }
    if job.state != JobState::Translating {
        return Err(RuntimeError::InvalidInput(
            "FB2 job is not ready for export".into(),
        ));
    }
    if blocks.is_empty() || blocks.iter().any(|block| block.translated_text.is_none()) {
        return Err(RuntimeError::InvalidInput(
            "FB2 job has untranslated blocks".into(),
        ));
    }
    Ok(())
}

/// `replace` is true for a path confirmed in the save dialog.
fn validate_output_target(source: &Path, target: &Path, replace: bool) -> Result<(), RuntimeError> {
    if super::same_path(source, target) {
        return Err(RuntimeError::InvalidInput(
            "output must be different from the source".into(),
        ));
    }
    if !replace && target.exists() {
        return Err(RuntimeError::InvalidInput(
            "output already exists; choose another path".into(),
        ));
    }
    Ok(())
}

fn export_state(diagnostics: &[String]) -> JobState {
    if diagnostics.is_empty() {
        JobState::Completed
    } else {
        JobState::CompletedWithWarnings
    }
}

#[tauri::command(async)]
pub fn analyze_fb2(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
    source_path: String,
    source_language: String,
    target_language: String,
) -> Result<DocumentJobView, RuntimeError> {
    let source = Path::new(source_path.trim());
    validate_source_path(source)?;
    let bytes = fs::read(source)
        .map_err(|error| RuntimeError::Connection(format!("read FB2 source: {error}")))?;
    let analysis = format::analyze(&bytes)?;
    if analysis.blocks.is_empty() {
        return Err(RuntimeError::InvalidInput(
            "FB2 source has no translatable blocks".into(),
        ));
    }
    let current = state
        .settings
        .lock()
        .map_err(|_| RuntimeError::Connection("settings lock is poisoned".into()))?
        .clone();
    if current.model_id.trim().is_empty() {
        return Err(RuntimeError::InvalidInput(
            "select a model before analyzing a document".into(),
        ));
    }
    let sample = analysis
        .blocks
        .iter()
        .take(8)
        .max_by_key(|block| block.source_text.chars().count())
        .map(|block| block.source_text.as_str())
        .unwrap_or_default();
    let (source_language, target_language) =
        resolve_document_languages(&source_language, &target_language, sample, &current)?;
    let job_id = new_job_id();
    let job = DocumentJob {
        id: job_id.clone(),
        source_path: source.display().to_string(),
        source_hash: crate::documents::store::content_hash(&bytes),
        format: "fb2".into(),
        parser_version: format::PARSER_VERSION.into(),
        source_language,
        target_language,
        runtime_snapshot: snapshot(&current, &current.model_id),
        configuration_version: format::PARSER_VERSION.into(),
        state: JobState::Queued,
        error: None,
    };
    let mut store = open_store(&app)?;
    store.create(&job)?;
    store.transition(&job_id, JobState::Analyzing, None)?;
    for block in analysis.blocks {
        store.save_block(&job_id, &block)?;
    }
    store.replace_diagnostics(&job_id, &analysis.diagnostics)?;
    store.transition(&job_id, JobState::Ready, None)?;
    view(&store, &job_id)
}

#[tauri::command(async)]
pub fn start_fb2_job(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
    job_id: String,
) -> Result<DocumentJobView, RuntimeError> {
    let mut store = open_store(&app)?;
    let job = store
        .get(&job_id)?
        .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
    if job.format != "fb2" {
        return Err(RuntimeError::InvalidInput("document job is not FB2".into()));
    }
    if job.state != JobState::Ready {
        return Err(RuntimeError::InvalidInput(format!(
            "document job is {:?}, not ready",
            job.state
        )));
    }
    store.transition(&job_id, JobState::Translating, None)?;
    run_job(&mut store, &state, &job_id).map(|(job, _)| job)
}

#[tauri::command(async)]
pub fn resume_fb2_job(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
    job_id: String,
) -> Result<DocumentJobView, RuntimeError> {
    let mut store = open_store(&app)?;
    let current = state
        .settings
        .lock()
        .map_err(|_| RuntimeError::Connection("settings lock is poisoned".into()))?
        .clone();
    let current_snapshot = snapshot(&current, &current.model_id);
    store.resume(&job_id, &current_snapshot, format::PARSER_VERSION)?;
    run_job(&mut store, &state, &job_id).map(|(job, _)| job)
}

#[tauri::command(async)]
pub fn export_fb2_job(
    app: AppHandle,
    job_id: String,
    output_path: Option<String>,
) -> Result<DocumentExport, RuntimeError> {
    let store = open_store(&app)?;
    let job = store
        .get(&job_id)?
        .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
    let source_path = Path::new(&job.source_path);
    let source = fs::read(source_path)
        .map_err(|error| RuntimeError::Connection(format!("read FB2 source: {error}")))?;
    let blocks = store.blocks(&job_id)?;
    validate_export(&job, &source, &blocks)?;
    let explicit = output_path
        .as_ref()
        .is_some_and(|path| !path.trim().is_empty());
    let target = output_path
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_output_path(source_path, &job.target_language, "fb2"));
    validate_output_target(source_path, &target, explicit)?;
    let output = format::export(&source, &blocks)?;
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| RuntimeError::Connection(format!("create output folder: {error}")))?;
    store.transition(&job_id, JobState::Exporting, None)?;
    let mut temporary = None;
    let result = (|| {
        let (path, mut file) = create_temporary_output(parent)?;
        temporary = Some(path.clone());
        file.write_all(&output)
            .map_err(|error| RuntimeError::Connection(format!("write FB2 output: {error}")))?;
        fs::rename(&path, &target)
            .map_err(|error| RuntimeError::Connection(format!("finish FB2 output: {error}")))
    })();
    if let Err(error) = result {
        if let Some(path) = temporary {
            let _ = fs::remove_file(path);
        }
        let _ = store.transition(&job_id, JobState::Failed, Some(&error.to_string()));
        return Err(error);
    }
    let diagnostics = store.diagnostics(&job_id)?;
    store.transition(&job_id, export_state(&diagnostics), None)?;
    Ok(DocumentExport {
        output_path: target.display().to_string(),
        job: view(&store, &job_id)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::store::content_hash;

    #[test]
    fn validates_fb2_extension_case_insensitively() {
        assert!(validate_source_path(Path::new("book.FB2")).is_ok());
        assert!(validate_source_path(Path::new("book.fb2.zip")).is_err());
    }

    #[test]
    fn validates_current_translated_fb2_job_and_safe_output() {
        let job = fb2_job(JobState::Translating);
        let translated = [fb2_block(Some("translation"))];

        assert!(validate_export(&job, b"changed", &translated).is_err());
        assert!(validate_export(&job, b"source", &[fb2_block(None)]).is_err());
        assert!(validate_export(&fb2_job(JobState::Ready), b"source", &translated).is_err());
        assert!(
            validate_output_target(Path::new("book.fb2"), Path::new("book.fb2"), false).is_err()
        );
        assert!(validate_output_target(Path::new("book.fb2"), Path::new("out.fb2"), false).is_ok());
    }

    #[test]
    fn completed_state_reflects_parser_diagnostics() {
        assert_eq!(export_state(&[]), JobState::Completed);
        assert_eq!(
            export_state(&["unsupported poem".into()]),
            JobState::CompletedWithWarnings
        );
    }

    fn fb2_job(state: JobState) -> DocumentJob {
        DocumentJob {
            id: "fb2-job".into(),
            source_path: "book.fb2".into(),
            source_hash: content_hash(b"source"),
            format: "fb2".into(),
            parser_version: format::PARSER_VERSION.into(),
            source_language: "en".into(),
            target_language: "ru".into(),
            runtime_snapshot: "snapshot".into(),
            configuration_version: format::PARSER_VERSION.into(),
            state,
            error: None,
        }
    }

    fn fb2_block(translated_text: Option<&str>) -> DocumentBlock {
        DocumentBlock {
            id: "fb2#0".into(),
            ordinal: 0,
            block_type: crate::documents::BlockType::Paragraph,
            source_text: "source".into(),
            translated_text: translated_text.map(str::to_string),
        }
    }
}
