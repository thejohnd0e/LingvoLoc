use super::docx;
use super::epub;
use super::store::content_hash;
use super::txt::{self, PARSER_VERSION};
use super::{
    translate_job, DocumentBlock, DocumentJob, DocumentJobStore, JobState, SegmentationLimits,
    WorkerReport,
};
use crate::domain::{RuntimeError, TranslationStyle};
use crate::services::inference_coordinator::snapshot;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

pub mod fb2;
pub mod pdf;

const CONFIGURATION_VERSION: &str = "txt-v1";

#[derive(Debug, Clone, serde::Serialize)]
pub struct DocumentJobView {
    pub job: DocumentJob,
    pub blocks: Vec<DocumentBlock>,
    pub diagnostics: Vec<String>,
    pub translated_blocks: usize,
    pub total_blocks: usize,
    pub paused: bool,
    pub cancelled: bool,
}

/// Lightweight row for the recent-jobs list (no blocks or diagnostics).
#[derive(Debug, Clone, serde::Serialize)]
pub struct DocumentJobSummary {
    pub job: DocumentJob,
    pub total_blocks: usize,
    pub translated_blocks: usize,
}

#[tauri::command(async)]
pub fn list_document_jobs(app: AppHandle) -> Result<Vec<DocumentJobSummary>, RuntimeError> {
    let store = open_store(&app)?;
    Ok(store
        .summaries(30)?
        .into_iter()
        .map(
            |(job, total_blocks, translated_blocks)| DocumentJobSummary {
                job,
                total_blocks,
                translated_blocks,
            },
        )
        .collect())
}

/// Progress without blocks or diagnostics; cheap enough to poll on large books.
#[tauri::command(async)]
pub fn get_document_progress(
    app: AppHandle,
    job_id: String,
) -> Result<DocumentJobSummary, RuntimeError> {
    let store = open_store(&app)?;
    let (job, total_blocks, translated_blocks) = store
        .summary(&job_id)?
        .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
    Ok(DocumentJobSummary {
        job,
        total_blocks,
        translated_blocks,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DocumentExport {
    pub output_path: String,
    pub job: DocumentJobView,
}

fn store_path(app: &AppHandle) -> Result<PathBuf, RuntimeError> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| RuntimeError::Connection(format!("app data folder: {error}")))?;
    fs::create_dir_all(&directory).map_err(|error| {
        RuntimeError::Connection(format!("create document data folder: {error}"))
    })?;
    Ok(directory.join("document-jobs.sqlite"))
}

pub(super) fn open_store(app: &AppHandle) -> Result<DocumentJobStore, RuntimeError> {
    DocumentJobStore::open(&store_path(app)?)
}

pub(super) fn new_job_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("txt-{}-{timestamp}", std::process::id())
}

static TEMP_NONCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn create_temporary_output(parent: &Path) -> Result<(PathBuf, fs::File), RuntimeError> {
    for _ in 0..32 {
        let nonce = TEMP_NONCE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".lingvoloc-{}-{}-{}.tmp",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            nonce
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(RuntimeError::Connection(format!(
                    "create temporary output: {error}"
                )))
            }
        }
    }
    Err(RuntimeError::Connection(
        "could not create a unique temporary output".into(),
    ))
}

fn sanitize_filename_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches('_');
    if sanitized.is_empty() {
        "target".into()
    } else {
        sanitized.to_string()
    }
}

pub(super) fn default_output_path(
    source: &Path,
    target_language: &str,
    extension: &str,
) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("translated");
    let language = sanitize_filename_component(target_language);
    source.with_file_name(format!("{stem}.translated.{language}.{extension}"))
}

pub(super) fn view(
    store: &DocumentJobStore,
    job_id: &str,
) -> Result<DocumentJobView, RuntimeError> {
    let job = store
        .get(job_id)?
        .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
    let blocks = store.blocks(job_id)?;
    let diagnostics = store.diagnostics(job_id)?;
    let translated_blocks = blocks
        .iter()
        .filter(|block| block.translated_text.is_some())
        .count();
    Ok(DocumentJobView {
        paused: matches!(job.state, JobState::Paused),
        cancelled: matches!(job.state, JobState::Cancelled),
        total_blocks: blocks.len(),
        translated_blocks,
        job,
        blocks,
        diagnostics,
    })
}

#[tauri::command(async)]
pub fn analyze_txt(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
    source_path: String,
    source_language: String,
    target_language: String,
    translation_style: TranslationStyle,
) -> Result<DocumentJobView, RuntimeError> {
    let source = Path::new(source_path.trim());
    if source
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        != Some("txt".into())
    {
        return Err(RuntimeError::InvalidInput("select a .txt file".into()));
    }
    let bytes = fs::read(source)
        .map_err(|error| RuntimeError::Connection(format!("read TXT source: {error}")))?;
    let document = txt::decode(&bytes).map(|text| txt::parse(&text))?;
    if document.paragraphs.is_empty() {
        return Err(RuntimeError::InvalidInput(
            "TXT source has no non-empty paragraphs".into(),
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
    let source_language = if source_language == "auto" {
        crate::services::detection::detect_supported_language(&document.paragraphs[0].source_text)?
            .code
    } else {
        source_language
    };
    let target_language = if target_language == "auto" {
        if source_language == current.primary_language {
            current.secondary_language.clone()
        } else {
            current.primary_language.clone()
        }
    } else {
        target_language
    };
    let job_id = new_job_id();
    let mut store = open_store(&app)?;
    let job = DocumentJob {
        id: job_id.clone(),
        source_path: source.display().to_string(),
        source_hash: content_hash(&bytes),
        format: "txt".into(),
        parser_version: PARSER_VERSION.into(),
        source_language,
        target_language,
        translation_style,
        runtime_snapshot: snapshot(&current, &current.model_id),
        configuration_version: CONFIGURATION_VERSION.into(),
        state: JobState::Queued,
        error: None,
    };
    store.create(&job)?;
    store.transition(&job_id, JobState::Analyzing, None)?;
    for block in document.paragraphs {
        store.save_block(&job_id, &block)?;
    }
    store.transition(&job_id, JobState::Ready, None)?;
    view(&store, &job_id)
}

pub(super) fn run_job(
    store: &mut DocumentJobStore,
    state: &tauri::State<'_, crate::AppState>,
    job_id: &str,
) -> Result<(DocumentJobView, WorkerReport), RuntimeError> {
    let result = (|| {
        let current = state
            .settings
            .lock()
            .map_err(|_| RuntimeError::Connection("settings lock is poisoned".into()))?
            .clone();
        let job = store
            .get(job_id)?
            .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
        let source = fs::read(&job.source_path).map_err(|error| {
            let label = match job.format.as_str() {
                "txt" => "TXT",
                "epub" => "EPUB",
                "fb2" => "FB2",
                _ => "DOCX",
            };
            RuntimeError::Connection(format!("read {label} source: {error}"))
        })?;
        if !DocumentJobStore::source_is_current(&job, &source) {
            return Err(RuntimeError::InvalidInput(
                "document source changed; analyze it again".into(),
            ));
        }
        let current_snapshot = snapshot(&current, &current.model_id);
        if current_snapshot != job.runtime_snapshot {
            return Err(RuntimeError::InvalidInput(
                "document settings changed; resume explicitly after re-analysis".into(),
            ));
        }
        let report = translate_job(
            store,
            &state.inference,
            job_id,
            &current,
            &current_snapshot,
            SegmentationLimits::default(),
        )?;
        Ok((view(store, job_id)?, report))
    })();
    if result.is_err()
        && store
            .get(job_id)
            .ok()
            .flatten()
            .is_some_and(|job| job.state == JobState::Translating)
    {
        if let Err(error) = &result {
            mark_preflight_failure(store, job_id, error);
        }
    }
    result
}

pub(super) fn resolve_document_languages(
    source_language: &str,
    target_language: &str,
    sample: &str,
    settings: &crate::domain::Settings,
) -> Result<(String, String), RuntimeError> {
    let source = if source_language == "auto" {
        crate::services::detection::detect_supported_language(sample)?.code
    } else {
        source_language.to_string()
    };
    let target = if target_language == "auto" {
        if source == settings.primary_language {
            settings.secondary_language.clone()
        } else {
            settings.primary_language.clone()
        }
    } else {
        target_language.to_string()
    };
    if source == target {
        return Err(RuntimeError::InvalidInput(format!(
            "source and target language are both '{source}'. Choose a different target language; the file may already be translated."
        )));
    }
    Ok((source, target))
}

fn validate_epub_source_path(source: &Path) -> Result<(), RuntimeError> {
    if source
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        != Some("epub".into())
    {
        return Err(RuntimeError::InvalidInput("select a .epub file".into()));
    }
    Ok(())
}

fn validate_epub_export(
    job: &DocumentJob,
    source: &[u8],
    blocks: &[DocumentBlock],
    output_path: Option<&Path>,
) -> Result<(), RuntimeError> {
    if job.format != "epub" {
        return Err(RuntimeError::InvalidInput(
            "document job is not EPUB".into(),
        ));
    }
    if !DocumentJobStore::source_is_current(job, source) {
        return Err(RuntimeError::InvalidInput(
            "document source changed; analyze it again".into(),
        ));
    }
    if job.state != JobState::Translating {
        return Err(RuntimeError::InvalidInput(
            "EPUB job is not ready for export".into(),
        ));
    }
    if blocks.is_empty() || blocks.iter().any(|block| block.translated_text.is_none()) {
        return Err(RuntimeError::InvalidInput(
            "EPUB job has untranslated blocks".into(),
        ));
    }
    if let Some(target) = output_path {
        let source_path = Path::new(&job.source_path);
        if same_path(source_path, target) {
            return Err(RuntimeError::InvalidInput(
                "output must be different from the source".into(),
            ));
        }
        // A path picked in the save dialog was confirmed for replacement there.
    }
    Ok(())
}

fn epub_export_state(diagnostics: &[String]) -> JobState {
    if diagnostics.is_empty() {
        JobState::Completed
    } else {
        JobState::CompletedWithWarnings
    }
}

fn mark_preflight_failure(store: &mut DocumentJobStore, job_id: &str, error: &RuntimeError) {
    if store
        .get(job_id)
        .ok()
        .flatten()
        .is_some_and(|job| job.state == JobState::Translating)
    {
        let _ = store.transition(job_id, JobState::Failed, Some(&error.to_string()));
    }
}

#[tauri::command(async)]
pub fn analyze_docx(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
    source_path: String,
    source_language: String,
    target_language: String,
    translation_style: TranslationStyle,
) -> Result<DocumentJobView, RuntimeError> {
    let source = Path::new(source_path.trim());
    if source
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        != Some("docx".into())
    {
        return Err(RuntimeError::InvalidInput("select a .docx file".into()));
    }
    let bytes = fs::read(source)
        .map_err(|error| RuntimeError::Connection(format!("read DOCX source: {error}")))?;
    let analysis = docx::analyze(&bytes)?;
    if analysis.blocks.is_empty() {
        return Err(RuntimeError::InvalidInput(
            "DOCX source has no translatable paragraphs".into(),
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
    let job_id = new_job_id();
    let sample = analysis
        .blocks
        .iter()
        .take(8)
        .max_by_key(|block| block.source_text.chars().count())
        .map(|block| block.source_text.as_str())
        .unwrap_or_default();
    let (source_language, target_language) =
        resolve_document_languages(&source_language, &target_language, sample, &current)?;
    let job = DocumentJob {
        id: job_id.clone(),
        source_path: source.display().to_string(),
        source_hash: content_hash(&bytes),
        format: "docx".into(),
        parser_version: docx::PARSER_VERSION.into(),
        source_language,
        target_language,
        translation_style,
        runtime_snapshot: snapshot(&current, &current.model_id),
        configuration_version: docx::PARSER_VERSION.into(),
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
pub fn analyze_epub(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
    source_path: String,
    source_language: String,
    target_language: String,
    translation_style: TranslationStyle,
) -> Result<DocumentJobView, RuntimeError> {
    let source = Path::new(source_path.trim());
    validate_epub_source_path(source)?;
    let bytes = fs::read(source)
        .map_err(|error| RuntimeError::Connection(format!("read EPUB source: {error}")))?;
    let analysis = epub::analyze(&bytes)?;
    if analysis.blocks.is_empty() {
        return Err(RuntimeError::InvalidInput(
            "EPUB source has no translatable blocks".into(),
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
        source_hash: content_hash(&bytes),
        format: "epub".into(),
        parser_version: epub::PARSER_VERSION.into(),
        source_language,
        target_language,
        translation_style,
        runtime_snapshot: snapshot(&current, &current.model_id),
        configuration_version: epub::PARSER_VERSION.into(),
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
pub fn start_docx_job(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
    job_id: String,
) -> Result<DocumentJobView, RuntimeError> {
    let mut store = open_store(&app)?;
    let job = store
        .get(&job_id)?
        .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
    if job.format != "docx" {
        return Err(RuntimeError::InvalidInput(
            "document job is not DOCX".into(),
        ));
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
pub fn resume_docx_job(
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
    store.resume(&job_id, &current_snapshot, docx::PARSER_VERSION)?;
    run_job(&mut store, &state, &job_id).map(|(job, _)| job)
}

#[tauri::command(async)]
pub fn start_epub_job(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
    job_id: String,
) -> Result<DocumentJobView, RuntimeError> {
    let mut store = open_store(&app)?;
    let job = store
        .get(&job_id)?
        .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
    if job.format != "epub" {
        return Err(RuntimeError::InvalidInput(
            "document job is not EPUB".into(),
        ));
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
pub fn resume_epub_job(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
    job_id: String,
) -> Result<DocumentJobView, RuntimeError> {
    let mut store = open_store(&app)?;
    let job = store
        .get(&job_id)?
        .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
    if job.format != "epub" {
        return Err(RuntimeError::InvalidInput(
            "document job is not EPUB".into(),
        ));
    }
    let current = state
        .settings
        .lock()
        .map_err(|_| RuntimeError::Connection("settings lock is poisoned".into()))?
        .clone();
    let current_snapshot = snapshot(&current, &current.model_id);
    store.resume(&job_id, &current_snapshot, epub::PARSER_VERSION)?;
    run_job(&mut store, &state, &job_id).map(|(job, _)| job)
}

#[tauri::command(async)]
pub fn start_txt_job(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
    job_id: String,
) -> Result<DocumentJobView, RuntimeError> {
    let mut store = open_store(&app)?;
    let job = store
        .get(&job_id)?
        .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
    if job.state != JobState::Ready {
        return Err(RuntimeError::InvalidInput(format!(
            "document job is {:?}, not ready",
            job.state
        )));
    }
    store.transition(&job_id, JobState::Translating, None)?;
    run_job(&mut store, &state, &job_id).map(|(view, _)| view)
}

#[tauri::command(async)]
pub fn resume_txt_job(
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
    store.resume(&job_id, &current_snapshot, CONFIGURATION_VERSION)?;
    run_job(&mut store, &state, &job_id).map(|(view, _)| view)
}

#[tauri::command(async)]
pub fn get_document_job(app: AppHandle, job_id: String) -> Result<DocumentJobView, RuntimeError> {
    let store = open_store(&app)?;
    view(&store, &job_id)
}

#[tauri::command(async)]
pub fn pause_document_job(app: AppHandle, job_id: String) -> Result<DocumentJobView, RuntimeError> {
    let store = open_store(&app)?;
    store.transition(&job_id, JobState::Pausing, Some("pause requested"))?;
    crate::runtimes::llama_server::interrupt("pause requested");
    view(&store, &job_id)
}

#[tauri::command(async)]
pub fn cancel_document_job(
    app: AppHandle,
    job_id: String,
) -> Result<DocumentJobView, RuntimeError> {
    let store = open_store(&app)?;
    store.transition(&job_id, JobState::Cancelled, Some("cancelled by user"))?;
    crate::runtimes::llama_server::interrupt("job cancelled");
    view(&store, &job_id)
}

#[tauri::command(async)]
pub fn clear_document_job(app: AppHandle, job_id: String) -> Result<(), RuntimeError> {
    let store = open_store(&app)?;
    let state = store.get(&job_id)?.map(|job| job.state);
    if matches!(
        state,
        Some(JobState::Translating | JobState::Pausing | JobState::Cancelled)
    ) {
        crate::runtimes::llama_server::interrupt("job cleared");
    }
    store.delete(&job_id)
}

#[tauri::command(async)]
pub fn export_txt_job(
    app: AppHandle,
    job_id: String,
    output_path: Option<String>,
) -> Result<DocumentExport, RuntimeError> {
    let store = open_store(&app)?;
    let job = store
        .get(&job_id)?
        .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
    let blocks = store.blocks(&job_id)?;
    let output = txt::export(&blocks)?;
    let source = Path::new(&job.source_path);
    let explicit = output_path
        .as_ref()
        .is_some_and(|path| !path.trim().is_empty());
    let target = output_path
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_output_path(source, &job.target_language, "txt"));
    if same_path(source, &target) {
        return Err(RuntimeError::InvalidInput(
            "output must be different from the source".into(),
        ));
    }
    if !explicit && target.exists() {
        return Err(RuntimeError::InvalidInput(
            "output already exists; choose another path".into(),
        ));
    }
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| RuntimeError::Connection(format!("create output folder: {error}")))?;
    store.transition(&job_id, JobState::Exporting, None)?;
    let mut temporary = None;
    let result = (|| {
        let (path, mut file) = create_temporary_output(parent)?;
        temporary = Some(path.clone());
        file.write_all(output.as_bytes())
            .map_err(|error| RuntimeError::Connection(format!("write TXT output: {error}")))?;
        fs::rename(&path, &target)
            .map_err(|error| RuntimeError::Connection(format!("finish TXT output: {error}")))
    })();
    if let Err(error) = result {
        if let Some(path) = temporary {
            let _ = fs::remove_file(path);
        }
        let _ = store.transition(&job_id, JobState::Failed, Some(&error.to_string()));
        return Err(error);
    }
    store.transition(&job_id, JobState::Completed, None)?;
    Ok(DocumentExport {
        output_path: target.display().to_string(),
        job: view(&store, &job_id)?,
    })
}

#[tauri::command(async)]
pub fn export_docx_job(
    app: AppHandle,
    job_id: String,
    output_path: Option<String>,
) -> Result<DocumentExport, RuntimeError> {
    let store = open_store(&app)?;
    let job = store
        .get(&job_id)?
        .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
    if job.format != "docx" {
        return Err(RuntimeError::InvalidInput(
            "document job is not DOCX".into(),
        ));
    }
    let source_path = Path::new(&job.source_path);
    let source = fs::read(source_path)
        .map_err(|error| RuntimeError::Connection(format!("read DOCX source: {error}")))?;
    if !DocumentJobStore::source_is_current(&job, &source) {
        return Err(RuntimeError::InvalidInput(
            "document source changed; analyze it again".into(),
        ));
    }
    if job.state != JobState::Translating {
        return Err(RuntimeError::InvalidInput(
            "DOCX job is not ready for export".into(),
        ));
    }
    let blocks = store.blocks(&job_id)?;
    if blocks.is_empty() || blocks.iter().any(|block| block.translated_text.is_none()) {
        return Err(RuntimeError::InvalidInput(
            "DOCX job has untranslated blocks".into(),
        ));
    }
    let output = docx::export(&source, &blocks)?;
    let explicit = output_path
        .as_ref()
        .is_some_and(|path| !path.trim().is_empty());
    let target = output_path
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_output_path(source_path, &job.target_language, "docx"));
    if same_path(source_path, &target) {
        return Err(RuntimeError::InvalidInput(
            "output must be different from the source".into(),
        ));
    }
    if !explicit && target.exists() {
        return Err(RuntimeError::InvalidInput(
            "output already exists; choose another path".into(),
        ));
    }
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| RuntimeError::Connection(format!("create output folder: {error}")))?;
    store.transition(&job_id, JobState::Exporting, None)?;
    let mut temporary = None;
    let result = (|| {
        let (path, mut file) = create_temporary_output(parent)?;
        temporary = Some(path.clone());
        file.write_all(&output)
            .map_err(|error| RuntimeError::Connection(format!("write DOCX output: {error}")))?;
        fs::rename(&path, &target)
            .map_err(|error| RuntimeError::Connection(format!("finish DOCX output: {error}")))
    })();
    if let Err(error) = result {
        if let Some(path) = temporary {
            let _ = fs::remove_file(path);
        }
        let _ = store.transition(&job_id, JobState::Failed, Some(&error.to_string()));
        return Err(error);
    }
    let final_state = if store.diagnostics(&job_id)?.is_empty() {
        JobState::Completed
    } else {
        JobState::CompletedWithWarnings
    };
    store.transition(&job_id, final_state, None)?;
    Ok(DocumentExport {
        output_path: target.display().to_string(),
        job: view(&store, &job_id)?,
    })
}

#[tauri::command(async)]
pub fn export_epub_job(
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
        .map_err(|error| RuntimeError::Connection(format!("read EPUB source: {error}")))?;
    let blocks = store.blocks(&job_id)?;
    let explicit = output_path
        .as_ref()
        .is_some_and(|path| !path.trim().is_empty());
    let target = output_path
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_output_path(source_path, &job.target_language, "epub"));
    validate_epub_export(&job, &source, &blocks, Some(&target))?;
    if !explicit && target.exists() {
        return Err(RuntimeError::InvalidInput(
            "output already exists; choose another path".into(),
        ));
    }
    let output = epub::export(&source, &blocks)?;
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| RuntimeError::Connection(format!("create output folder: {error}")))?;
    store.transition(&job_id, JobState::Exporting, None)?;
    let mut temporary = None;
    let result = (|| {
        let (path, mut file) = create_temporary_output(parent)?;
        temporary = Some(path.clone());
        file.write_all(&output)
            .map_err(|error| RuntimeError::Connection(format!("write EPUB output: {error}")))?;
        fs::rename(&path, &target)
            .map_err(|error| RuntimeError::Connection(format!("finish EPUB output: {error}")))
    })();
    if let Err(error) = result {
        if let Some(path) = temporary {
            let _ = fs::remove_file(path);
        }
        let _ = store.transition(&job_id, JobState::Failed, Some(&error.to_string()));
        return Err(error);
    }
    let diagnostics = store.diagnostics(&job_id)?;
    store.transition(&job_id, epub_export_state(&diagnostics), None)?;
    Ok(DocumentExport {
        output_path: target.display().to_string(),
        job: view(&store, &job_id)?,
    })
}

pub(super) fn same_path(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::epub;
    use crate::domain::{RuntimeMode, Settings, TranslationStyle};

    fn settings() -> Settings {
        Settings {
            runtime_mode: RuntimeMode::Standalone,
            models_directory: String::new(),
            llama_server_path: String::new(),
            endpoint: String::new(),
            model_id: "model".into(),
            adapter_id: "adapter".into(),
            source_language: "auto".into(),
            target_language: "en".into(),
            primary_language: "en".into(),
            secondary_language: "ru".into(),
            translation_style: TranslationStyle::Neutral,
        }
    }

    #[test]
    fn resolves_auto_source_and_opposite_auto_target() {
        let resolved =
            resolve_document_languages("auto", "auto", "Привет мир", &settings()).unwrap();
        assert_eq!(resolved, ("ru".into(), "en".into()));
    }

    #[test]
    fn keeps_explicit_document_language_direction() {
        let same = resolve_document_languages("en", "en", "Hello", &settings()).unwrap_err();
        assert!(same.to_string().contains("both 'en'"), "{same}");
        let resolved = resolve_document_languages("de", "fr", "Hallo", &settings()).unwrap();
        assert_eq!(resolved, ("de".into(), "fr".into()));
    }

    #[test]
    fn validates_epub_extension_case_insensitively() {
        assert!(validate_epub_source_path(Path::new("book.EPUB")).is_ok());
        assert!(validate_epub_source_path(Path::new("book.docx")).is_err());
    }

    #[test]
    fn resolves_epub_auto_languages_from_a_representative_sample() {
        let resolved =
            resolve_document_languages("auto", "auto", "Привет из EPUB", &settings()).unwrap();
        assert_eq!(resolved, ("ru".into(), "en".into()));
    }

    #[test]
    fn epub_export_preflight_rejects_mutated_source() {
        let job = epub_job(JobState::Translating);
        let error = validate_epub_export(&job, b"changed", &[], None).unwrap_err();
        assert!(error.to_string().contains("source changed"));
    }

    #[test]
    fn epub_export_preflight_requires_active_job_and_all_translated_blocks() {
        let mut job = epub_job(JobState::Ready);
        let blocks = vec![epub_block(None), epub_block(Some("translated"))];
        assert!(validate_epub_export(&job, b"source", &blocks, None).is_err());

        job.state = JobState::Translating;
        assert!(validate_epub_export(&job, b"source", &blocks, None).is_err());
        assert!(validate_epub_export(
            &job,
            b"source",
            &[epub_block(Some("one")), epub_block(Some("two"))],
            None
        )
        .is_ok());
    }

    #[test]
    fn epub_export_preflight_rejects_source_and_existing_output_collisions() {
        let job = epub_job(JobState::Translating);
        let blocks = [epub_block(Some("translated"))];
        let source_collision =
            validate_epub_export(&job, b"source", &blocks, Some(Path::new("book.epub")))
                .unwrap_err();
        assert!(source_collision
            .to_string()
            .contains("different from the source"));

        let directory =
            std::env::temp_dir().join(format!("lingvoloc-epub-command-{}", new_job_id()));
        fs::create_dir_all(&directory).unwrap();
        let existing = directory.join("existing.epub");
        fs::write(&existing, b"already here").unwrap();
        assert!(validate_epub_export(&job, b"source", &blocks, Some(&existing)).is_ok());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn epub_export_uses_warning_state_when_diagnostics_exist() {
        assert_eq!(epub_export_state(&[]), JobState::Completed);
        assert_eq!(
            epub_export_state(&["unsupported script".into()]),
            JobState::CompletedWithWarnings
        );
    }

    #[test]
    fn default_output_names_keep_target_language_inside_source_directory() {
        let source = Path::new("books/source.epub");
        let target = default_output_path(source, "../../outside:ru", "epub");
        assert_eq!(target.parent(), source.parent());
        assert_eq!(
            target.extension().and_then(|value| value.to_str()),
            Some("epub")
        );
        assert!(!target.to_string_lossy().contains("..\\"));
        assert!(!target.to_string_lossy().contains("../"));
        assert!(!target.to_string_lossy().contains(':'));
    }

    #[test]
    fn temporary_outputs_are_unique_and_exclusively_created() {
        let directory = std::env::temp_dir().join(format!("lingvoloc-epub-temp-{}", new_job_id()));
        fs::create_dir_all(&directory).unwrap();
        let (first_path, mut first) = create_temporary_output(&directory).unwrap();
        let (second_path, mut second) = create_temporary_output(&directory).unwrap();
        first.write_all(b"first").unwrap();
        second.write_all(b"second").unwrap();
        assert_ne!(first_path, second_path);
        assert_eq!(fs::read(&first_path).unwrap(), b"first");
        assert_eq!(fs::read(&second_path).unwrap(), b"second");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn epub_preflight_failure_transitions_translating_job_to_failed() {
        let mut store = DocumentJobStore::in_memory().unwrap();
        let job = epub_job(JobState::Translating);
        store.create(&job).unwrap();
        let error = RuntimeError::InvalidInput("source changed; analyze it again".into());
        mark_preflight_failure(&mut store, &job.id, &error);
        let failed = store.get(&job.id).unwrap().unwrap();
        assert_eq!(failed.state, JobState::Failed);
        assert_eq!(
            failed.error.as_deref(),
            Some("invalid input: source changed; analyze it again")
        );
    }

    fn epub_job(state: JobState) -> DocumentJob {
        DocumentJob {
            id: "epub-job".into(),
            source_path: "book.epub".into(),
            source_hash: content_hash(b"source"),
            format: "epub".into(),
            parser_version: epub::PARSER_VERSION.into(),
            source_language: "en".into(),
            target_language: "ru".into(),
            runtime_snapshot: "snapshot".into(),
            configuration_version: epub::PARSER_VERSION.into(),
            translation_style: TranslationStyle::Neutral,
            state,
            error: None,
        }
    }

    fn epub_block(translated_text: Option<&str>) -> DocumentBlock {
        DocumentBlock {
            id: "OPS/chapter.xhtml#0".into(),
            ordinal: 0,
            block_type: crate::documents::BlockType::Paragraph,
            source_text: "source".into(),
            translated_text: translated_text.map(str::to_string),
        }
    }
}
