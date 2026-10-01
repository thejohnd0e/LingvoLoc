use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::documents::pdf as format;
use crate::documents::{DocumentBlock, DocumentJob, DocumentJobStore, JobState};
use crate::domain::{RuntimeError, TranslationStyle};
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
        != Some("pdf".into())
    {
        return Err(RuntimeError::InvalidInput("select a .pdf file".into()));
    }
    Ok(())
}

fn validate_parser_version(job: &DocumentJob) -> Result<(), RuntimeError> {
    if job.parser_version != format::PARSER_VERSION {
        return Err(RuntimeError::InvalidInput(
            "PDF layout analysis changed; remove this job and add the PDF again".into(),
        ));
    }
    Ok(())
}

fn validate_export(
    job: &DocumentJob,
    source: &[u8],
    blocks: &[DocumentBlock],
) -> Result<(), RuntimeError> {
    if job.format != "pdf" {
        return Err(RuntimeError::InvalidInput("document job is not PDF".into()));
    }
    validate_parser_version(job)?;
    if !DocumentJobStore::source_is_current(job, source) {
        return Err(RuntimeError::InvalidInput(
            "document source changed; analyze it again".into(),
        ));
    }
    if job.state != JobState::Translating {
        return Err(RuntimeError::InvalidInput(
            "PDF job is not ready for export".into(),
        ));
    }
    if blocks.is_empty() || blocks.iter().any(|block| block.translated_text.is_none()) {
        return Err(RuntimeError::InvalidInput(
            "PDF job has untranslated blocks".into(),
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

/// `book.translated.ru.pdf` -> `book.translated.ru.p5-12.pdf` so that partial
/// review outputs never collide with the full translation.
fn with_page_suffix(target: &Path, pages: &str) -> PathBuf {
    let stem = target
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    target.with_file_name(format!("{stem}.p{}.pdf", pages.replace(',', "_")))
}

fn export_state(diagnostics: &[String]) -> JobState {
    if diagnostics.is_empty() {
        JobState::Completed
    } else {
        JobState::CompletedWithWarnings
    }
}

#[tauri::command(async)]
pub fn analyze_pdf(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
    source_path: String,
    source_language: String,
    target_language: String,
    pages: Option<String>,
    translation_style: TranslationStyle,
) -> Result<DocumentJobView, RuntimeError> {
    let selection = format::PageSelection::parse(pages.as_deref().unwrap_or_default())?;
    let source = Path::new(source_path.trim());
    validate_source_path(source)?;
    let bytes = fs::read(source)
        .map_err(|error| RuntimeError::Connection(format!("read PDF source: {error}")))?;
    let analysis = format::analyze(&bytes, selection.as_ref())?;
    if analysis.blocks.is_empty() {
        return Err(RuntimeError::InvalidInput(
            "PDF source has no translatable blocks".into(),
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
        format: "pdf".into(),
        parser_version: format::PARSER_VERSION.into(),
        source_language,
        target_language,
        translation_style,
        runtime_snapshot: snapshot(&current, &current.model_id),
        configuration_version: format::configuration_version(selection.as_ref()),
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
pub fn start_pdf_job(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
    job_id: String,
) -> Result<DocumentJobView, RuntimeError> {
    let mut store = open_store(&app)?;
    let job = store
        .get(&job_id)?
        .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
    if job.format != "pdf" {
        return Err(RuntimeError::InvalidInput("document job is not PDF".into()));
    }
    validate_parser_version(&job)?;
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
pub fn resume_pdf_job(
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
    let job = store
        .get(&job_id)?
        .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
    if job.format != "pdf" {
        return Err(RuntimeError::InvalidInput("document job is not PDF".into()));
    }
    validate_parser_version(&job)?;
    let configuration = job.configuration_version;
    store.resume(&job_id, &current_snapshot, &configuration)?;
    run_job(&mut store, &state, &job_id).map(|(job, _)| job)
}

#[tauri::command(async)]
pub fn export_pdf_job(
    app: AppHandle,
    job_id: String,
    output_path: Option<String>,
) -> Result<DocumentExport, RuntimeError> {
    let mut store = open_store(&app)?;
    let job = store
        .get(&job_id)?
        .ok_or_else(|| RuntimeError::InvalidInput("document job not found".into()))?;
    let source_path = Path::new(&job.source_path);
    let source = fs::read(source_path)
        .map_err(|error| RuntimeError::Connection(format!("read PDF source: {error}")))?;
    let blocks = store.blocks(&job_id)?;
    validate_export(&job, &source, &blocks)?;
    let explicit = output_path
        .as_ref()
        .is_some_and(|path| !path.trim().is_empty());
    let target = output_path
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let target = default_output_path(source_path, &job.target_language, "pdf");
            match format::selection_from_configuration(&job.configuration_version) {
                Ok(Some(selection)) => with_page_suffix(&target, &selection.canonical()),
                _ => target,
            }
        });
    validate_output_target(source_path, &target, explicit)?;
    let selection = format::selection_from_configuration(&job.configuration_version)?;
    let exported = format::export(&source, &blocks, selection.as_ref())?;
    let output = exported.bytes;
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| RuntimeError::Connection(format!("create output folder: {error}")))?;
    store.transition(&job_id, JobState::Exporting, None)?;
    let mut temporary = None;
    let result = (|| {
        let (path, mut file) = create_temporary_output(parent)?;
        temporary = Some(path.clone());
        file.write_all(&output)
            .map_err(|error| RuntimeError::Connection(format!("write PDF output: {error}")))?;
        fs::rename(&path, &target)
            .map_err(|error| RuntimeError::Connection(format!("finish PDF output: {error}")))
    })();
    if let Err(error) = result {
        if let Some(path) = temporary {
            let _ = fs::remove_file(path);
        }
        let _ = store.transition(&job_id, JobState::Failed, Some(&error.to_string()));
        return Err(error);
    }
    let mut diagnostics = store.diagnostics(&job_id)?;
    diagnostics.extend(exported.diagnostics);
    store.replace_diagnostics(&job_id, &diagnostics)?;
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
    fn validates_pdf_extension_case_insensitively() {
        assert!(validate_source_path(Path::new("book.PDF")).is_ok());
        assert!(validate_source_path(Path::new("book.pdf.zip")).is_err());
    }

    #[test]
    fn validates_current_translated_pdf_job_and_safe_output() {
        let job = pdf_job(JobState::Translating);
        let translated = [pdf_block(Some("translation"))];

        assert!(validate_export(&job, b"changed", &translated).is_err());
        assert!(validate_export(&job, b"source", &[pdf_block(None)]).is_err());
        assert!(validate_export(&pdf_job(JobState::Ready), b"source", &translated).is_err());
        assert!(
            validate_output_target(Path::new("book.pdf"), Path::new("book.pdf"), false).is_err()
        );
        assert!(validate_output_target(Path::new("book.pdf"), Path::new("out.pdf"), false).is_ok());
    }

    #[test]
    fn rejects_jobs_from_an_older_pdf_layout_version() {
        let mut job = pdf_job(JobState::Translating);
        job.parser_version = "pdf-v1".into();

        let error = validate_parser_version(&job).expect_err("stale PDF job must be rejected");

        assert!(error
            .to_string()
            .contains("PDF layout analysis changed; remove this job and add the PDF again"));
        assert!(validate_parser_version(&pdf_job(JobState::Translating)).is_ok());
    }

    #[test]
    fn partial_output_name_carries_the_pages() {
        assert_eq!(
            with_page_suffix(Path::new("d/book.translated.ru.pdf"), "5-12,20"),
            Path::new("d/book.translated.ru.p5-12_20.pdf")
        );
    }

    #[test]
    fn completed_state_reflects_parser_diagnostics() {
        assert_eq!(export_state(&[]), JobState::Completed);
        assert_eq!(
            export_state(&["unsupported poem".into()]),
            JobState::CompletedWithWarnings
        );
    }

    fn pdf_job(state: JobState) -> DocumentJob {
        DocumentJob {
            id: "pdf-job".into(),
            source_path: "book.pdf".into(),
            source_hash: content_hash(b"source"),
            format: "pdf".into(),
            parser_version: format::PARSER_VERSION.into(),
            source_language: "en".into(),
            target_language: "ru".into(),
            runtime_snapshot: "snapshot".into(),
            configuration_version: format::PARSER_VERSION.into(),
            translation_style: TranslationStyle::Neutral,
            state,
            error: None,
        }
    }

    fn pdf_block(translated_text: Option<&str>) -> DocumentBlock {
        DocumentBlock {
            id: "pdf#0".into(),
            ordinal: 0,
            block_type: crate::documents::BlockType::Paragraph,
            source_text: "source".into(),
            translated_text: translated_text.map(str::to_string),
        }
    }
}
