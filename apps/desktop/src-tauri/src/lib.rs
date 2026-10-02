mod adapters;
mod api;
mod backends;
pub mod documents;
mod domain;
mod runtimes;
mod services;
mod trace;

use domain::{
    DetectedLanguage, LocalModel, RuntimeError, RuntimeMode, RuntimeStatus, Settings,
    TranslationRequest, TranslationResult, TranslationStyle,
};
use services::{
    credentials::{CredentialStatus, CredentialStore},
    history::{HistoryEntry, HistoryStore},
    request_control::RequestRegistry,
    session_usage::{SessionUsageEntry, SessionUsageTracker},
    translation,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub struct AppState {
    pub(crate) settings: Mutex<Settings>,
    pub(crate) history: Mutex<HistoryStore>,
    pub(crate) inference: services::inference_coordinator::InferenceCoordinator,
    pub(crate) pending_clipboard: Mutex<Option<String>>,
    pub(crate) api_token: String,
    pub(crate) credentials: CredentialStore,
    pub(crate) request_registry: RequestRegistry,
    pub(crate) session_usage: SessionUsageTracker,
    pub(crate) document_usage: Mutex<HashMap<String, documents::RequestUsage>>,
}

impl AppState {
    fn new(history: HistoryStore) -> Self {
        Self {
            settings: Mutex::new(Settings {
                runtime_mode: RuntimeMode::Standalone,
                models_directory: String::new(),
                llama_server_path: String::new(),
                endpoint: "http://127.0.0.1:1234/v1".into(),
                model_id: String::new(),
                adapter_id: "translategemma".into(),
                source_language: "auto".into(),
                target_language: "en".into(),
                primary_language: "en".into(),
                secondary_language: "ru".into(),
                translation_style: TranslationStyle::Neutral,
                cloud: Default::default(),
            }),
            history: Mutex::new(history),
            inference: services::inference_coordinator::InferenceCoordinator::default(),
            pending_clipboard: Mutex::new(None),
            api_token: api::generate_token(),
            credentials: CredentialStore::windows(),
            request_registry: RequestRegistry::default(),
            session_usage: SessionUsageTracker::default(),
            document_usage: Mutex::new(HashMap::new()),
        }
    }
}

fn settings(state: &tauri::State<'_, AppState>) -> Result<Settings, RuntimeError> {
    state
        .settings
        .lock()
        .map(|value| value.clone())
        .map_err(|_| RuntimeError::Connection("settings lock is poisoned".into()))
}

#[tauri::command]
fn get_api_token(state: tauri::State<'_, AppState>) -> String {
    state.api_token.clone()
}

#[tauri::command]
fn write_clipboard(text: String) -> Result<(), RuntimeError> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|error| RuntimeError::Connection(format!("clipboard: {error}")))?;
    clipboard
        .set_text(text)
        .map_err(|error| RuntimeError::Connection(format!("clipboard: {error}")))
}

#[tauri::command(async)]
fn get_runtime_status(state: tauri::State<'_, AppState>) -> Result<RuntimeStatus, RuntimeError> {
    translation::status_with_credentials(&settings(&state)?, &state.credentials)
}

#[tauri::command(async)]
fn list_models(state: tauri::State<'_, AppState>) -> Result<Vec<LocalModel>, RuntimeError> {
    translation::list_models_with_credentials(&settings(&state)?, &state.credentials)
}

fn settings_for_provider(
    current: &Settings,
    provider_id: domain::ProviderId,
) -> Result<Settings, RuntimeError> {
    let mut next = current.clone();
    next.runtime_mode = match provider_id {
        domain::ProviderId::OpenAi => domain::RuntimeMode::OpenAi,
        domain::ProviderId::Anthropic => domain::RuntimeMode::Anthropic,
        domain::ProviderId::Gemini => domain::RuntimeMode::Gemini,
        domain::ProviderId::DeepL => domain::RuntimeMode::DeepL,
        domain::ProviderId::OpenAiCompatible => domain::RuntimeMode::OpenAiCompatible,
        domain::ProviderId::LlamaCpp | domain::ProviderId::LmStudio => {
            return Err(RuntimeError::InvalidInput(
                "provider has no cloud refresh command".into(),
            ))
        }
    };
    Ok(next)
}

#[tauri::command(async)]
fn get_backend_capabilities(
    state: tauri::State<'_, AppState>,
) -> Result<domain::BackendCapabilities, RuntimeError> {
    Ok(
        backends::for_settings_with_credentials(&settings(&state)?, &state.credentials)?
            .capabilities(),
    )
}

#[tauri::command(async)]
fn refresh_provider_models(
    state: tauri::State<'_, AppState>,
    provider_id: domain::ProviderId,
) -> Result<Vec<LocalModel>, RuntimeError> {
    let next = settings_for_provider(&settings(&state)?, provider_id)?;
    translation::list_models_with_credentials(&next, &state.credentials)
}

#[tauri::command(async)]
fn refresh_deepl_languages(state: tauri::State<'_, AppState>) -> Result<Vec<String>, RuntimeError> {
    let current = settings(&state)?;
    let key = state
        .credentials
        .get(domain::ProviderId::DeepL)
        .map_err(services::credentials::map_error)?;
    let backend = backends::deepl::DeepLBackend::new(&current.cloud.deep_l.plan, key)?;
    backend.list_languages("target")
}

#[tauri::command(async)]
fn test_provider_connection(
    state: tauri::State<'_, AppState>,
    provider_id: domain::ProviderId,
) -> Result<RuntimeStatus, RuntimeError> {
    let next = settings_for_provider(&settings(&state)?, provider_id)?;
    let backend = backends::for_settings_with_credentials(&next, &state.credentials)?;
    if provider_id == domain::ProviderId::OpenAiCompatible {
        return Ok(RuntimeStatus {
            available: true,
            endpoint: next.cloud.open_ai_compatible.endpoint,
            detail: "OpenAI-compatible endpoint configured".into(),
        });
    }
    backend.status()
}

#[tauri::command(async)]
fn translate(
    state: tauri::State<'_, AppState>,
    request: TranslationRequest,
) -> Result<TranslationResult, RuntimeError> {
    let current = settings(&state)?;
    if request.model_id.trim().is_empty() {
        return Err(RuntimeError::InvalidInput(
            "a model must be selected".into(),
        ));
    }
    let snapshot = services::inference_coordinator::snapshot(&current, &request.model_id);
    let (request_id, cancellation) = state.request_registry.start_generated("interactive");
    let result = state.inference.run_interactive(&snapshot, || {
        translation::translate_with_cancellation_and_credentials(
            &current,
            request.clone(),
            &cancellation,
            &state.credentials,
        )
    });
    state.request_registry.finish(&request_id);
    let result = result?;
    state.session_usage.record_success(&result);
    state
        .history
        .lock()
        .map_err(|_| RuntimeError::Connection("history lock is poisoned".into()))?
        .add(&request.text, &request, &result)?;
    Ok(result)
}

#[tauri::command]
fn get_session_usage(state: tauri::State<'_, AppState>) -> Vec<SessionUsageEntry> {
    state.session_usage.list()
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> Result<Settings, RuntimeError> {
    settings(&state)
}

#[tauri::command]
fn update_settings(
    state: tauri::State<'_, AppState>,
    next: Settings,
) -> Result<Settings, RuntimeError> {
    if next.endpoint.trim().is_empty() || next.adapter_id.trim().is_empty() {
        return Err(RuntimeError::InvalidInput(
            "endpoint and adapter are required".into(),
        ));
    }
    let result = state.inference.run_lifecycle(|| {
        let mut current = state
            .settings
            .lock()
            .map_err(|_| RuntimeError::Connection("settings lock is poisoned".into()))?;
        *current = next.clone();
        Ok(next)
    })?;
    state
        .inference
        .set_desired_snapshot(&services::inference_coordinator::snapshot(
            &result,
            &result.model_id,
        ))?;
    Ok(result)
}

#[tauri::command(async)]
fn get_provider_credential_status(
    state: tauri::State<'_, AppState>,
    provider_id: domain::ProviderId,
) -> Result<CredentialStatus, RuntimeError> {
    state
        .credentials
        .status(provider_id)
        .map_err(services::credentials::map_error)
}

#[tauri::command(async)]
fn save_provider_credential(
    state: tauri::State<'_, AppState>,
    provider_id: domain::ProviderId,
    secret: String,
) -> Result<CredentialStatus, RuntimeError> {
    state
        .credentials
        .save(provider_id, &secret)
        .map_err(services::credentials::map_error)
}

#[tauri::command(async)]
fn delete_provider_credential(
    state: tauri::State<'_, AppState>,
    provider_id: domain::ProviderId,
) -> Result<(), RuntimeError> {
    state
        .credentials
        .delete(provider_id)
        .map_err(services::credentials::map_error)
}

#[tauri::command(async)]
fn check_llama_server(path: String) -> Result<String, RuntimeError> {
    runtimes::llama_server::check_server(&path)
}

#[tauri::command(async)]
fn locate_llama_server(directory: String) -> Result<String, RuntimeError> {
    runtimes::llama_server::find_in_directory(std::path::Path::new(directory.trim()))
        .map(|path| path.display().to_string())
        .ok_or_else(|| {
            RuntimeError::InvalidInput("llama-server.exe was not found in that folder".into())
        })
}

#[tauri::command(async)]
fn get_llama_devices(path: String) -> Result<runtimes::llama_server::LlamaDevices, RuntimeError> {
    runtimes::llama_server::describe_devices(&path)
}

#[tauri::command(async)]
fn llama_path_status(path: String) -> Result<String, RuntimeError> {
    runtimes::llama_server::user_path(&path, false)
}

#[tauri::command(async)]
fn add_llama_to_path(path: String) -> Result<String, RuntimeError> {
    runtimes::llama_server::user_path(&path, true)
}

#[tauri::command(async)]
fn get_gpu_info() -> runtimes::llama_download::GpuInfo {
    runtimes::llama_download::detect_gpu_info().1
}

#[tauri::command(async)]
fn download_llama_cpp(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<runtimes::llama_download::DownloadedLlama, RuntimeError> {
    static BUSY: AtomicBool = AtomicBool::new(false);
    if BUSY.swap(true, Ordering::SeqCst) {
        return Err(RuntimeError::InvalidInput(
            "llama.cpp is already being downloaded".into(),
        ));
    }
    let result = state.inference.run_lifecycle(|| {
        let base = app
            .path()
            .app_data_dir()
            .map_err(|error| RuntimeError::Connection(format!("app data folder: {error}")))?
            .join("llama.cpp");
        runtimes::llama_download::download_latest(&base, |progress| {
            let _ = app.emit("llama-download-progress", progress);
        })
    });
    BUSY.store(false, Ordering::SeqCst);
    result
}

#[tauri::command(async)]
fn find_llama_server() -> Option<String> {
    runtimes::llama_server::find_on_path().map(|path| path.display().to_string())
}

#[tauri::command(async)]
fn detect_language(text: String) -> Result<DetectedLanguage, RuntimeError> {
    services::detection::detect_supported_language(&text)
}

#[tauri::command(async)]
fn list_history(
    state: tauri::State<'_, AppState>,
    query: Option<String>,
    offset: Option<i64>,
) -> Result<Vec<HistoryEntry>, RuntimeError> {
    state
        .history
        .lock()
        .map_err(|_| RuntimeError::Connection("history lock is poisoned".into()))?
        .list_page(query.as_deref(), 20, offset.unwrap_or(0))
}

#[tauri::command(async)]
fn clear_history(state: tauri::State<'_, AppState>) -> Result<(), RuntimeError> {
    state
        .history
        .lock()
        .map_err(|_| RuntimeError::Connection("history lock is poisoned".into()))?
        .clear()
}

#[tauri::command]
fn set_history_favorite(
    state: tauri::State<'_, AppState>,
    id: i64,
    favorite: bool,
) -> Result<(), RuntimeError> {
    state
        .history
        .lock()
        .map_err(|_| RuntimeError::Connection("history lock is poisoned".into()))?
        .set_favorite(id, favorite)
}

#[tauri::command(async)]
fn export_history(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, RuntimeError> {
    let data_dir = app
        .path()
        .download_dir()
        .map_err(|error| RuntimeError::Connection(error.to_string()))?;
    std::fs::create_dir_all(&data_dir)
        .map_err(|error| RuntimeError::Connection(error.to_string()))?;
    let path = data_dir.join("LingvoLoc-history.csv");
    let csv = state
        .history
        .lock()
        .map_err(|_| RuntimeError::Connection("history lock is poisoned".into()))?
        .export_csv()?;
    std::fs::write(&path, csv).map_err(|error| RuntimeError::Connection(error.to_string()))?;
    Ok(path.display().to_string())
}

#[tauri::command]
fn take_clipboard_request(
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, RuntimeError> {
    state
        .pending_clipboard
        .lock()
        .map_err(|_| RuntimeError::Connection("clipboard request lock is poisoned".into()))
        .map(|mut value| value.take())
}

#[tauri::command(async)]
fn lookup_lexicon(
    query: String,
    language: Option<String>,
    enabled_dictionaries: Option<Vec<String>>,
    directory: Option<String>,
) -> Vec<services::lexical::LexicalEntry> {
    let directory = directory
        .filter(|value| !value.trim().is_empty())
        .map(std::path::PathBuf::from);
    services::lexical::lookup_with_user_directory(
        &query,
        language.as_deref(),
        directory.as_deref(),
        enabled_dictionaries.as_deref(),
    )
}

#[tauri::command(async)]
fn read_dictionary_media(directory: String, resource: String) -> Result<String, RuntimeError> {
    services::lexical::read_media_data_uri(std::path::Path::new(&directory), &resource)
        .map_err(RuntimeError::InvalidInput)
}

#[tauri::command(async)]
fn list_user_dictionaries(
    directory: Option<String>,
) -> Result<Vec<services::lexical::UserDictionary>, RuntimeError> {
    let directory = directory
        .filter(|value| !value.trim().is_empty())
        .map(std::path::PathBuf::from)
        .ok_or_else(|| RuntimeError::Connection("dictionary folder is not selected".into()))?;
    Ok(services::lexical::list_user_dictionaries(&directory))
}

#[tauri::command(async)]
fn translate_word(
    state: tauri::State<'_, AppState>,
    request: TranslationRequest,
) -> Result<TranslationResult, RuntimeError> {
    let current = settings(&state)?;
    if request.model_id.trim().is_empty() {
        return Err(RuntimeError::InvalidInput(
            "a model must be selected".into(),
        ));
    }
    let request = neutral_word_request(request);
    let snapshot = services::inference_coordinator::snapshot(&current, &request.model_id);
    state
        .inference
        .run_interactive(&snapshot, || translation::translate(&current, request))
}

fn neutral_word_request(mut request: TranslationRequest) -> TranslationRequest {
    request.translation_style = TranslationStyle::Neutral;
    request
}

#[cfg(test)]
mod translation_style_tests {
    use super::*;

    #[test]
    fn neutral_word_request_forces_neutral_style() {
        let request = TranslationRequest {
            model_id: "model".into(),
            adapter_id: "adapter".into(),
            source_language: "en".into(),
            target_language: "ru".into(),
            text: "word".into(),
            translation_style: TranslationStyle::Conversational,
        };
        assert_eq!(
            neutral_word_request(request).translation_style,
            TranslationStyle::Neutral
        );
    }
}

fn translate_clipboard(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("popup") else {
        return;
    };
    let _ = window.show();
    let _ = window.set_focus();
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        if let Ok(text) = clipboard.get_text() {
            if let Ok(mut pending) = app.state::<AppState>().pending_clipboard.lock() {
                *pending = Some(text);
            }
        }
    }
}

pub fn run() {
    let show_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyL);
    let translate_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyT);
    let handler_translate_shortcut = translate_shortcut;
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }
                    if shortcut == &handler_translate_shortcut {
                        translate_clipboard(app);
                    } else if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            std::fs::create_dir_all(&data_dir)?;
            let database_path = data_dir.join("lingvoloc.sqlite");
            let legacy_database_path = data_dir.join("lingoloc.sqlite");
            let history_path = if database_path.exists() || !legacy_database_path.exists() {
                database_path
            } else {
                legacy_database_path
            };
            let history = HistoryStore::open(&history_path).map_err(std::io::Error::other)?;
            app.manage(AppState::new(history));
            if let Ok(document_store) =
                documents::DocumentJobStore::open(&data_dir.join("document-jobs.sqlite"))
            {
                let _ = document_store.recover_interrupted();
            }
            api::start(app.handle().clone());
            app.global_shortcut().register(show_shortcut)?;
            app.global_shortcut().register(translate_shortcut)?;

            if std::env::args().any(|arg| arg == services::autostart::TRAY_ARG) {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            let open = MenuItemBuilder::with_id("open", "Open LingvoLoc")
                .accelerator("Ctrl+Shift+L")
                .build(app)?;
            let translate_clip =
                MenuItemBuilder::with_id("translate_clipboard", "Translate clipboard")
                    .accelerator("Ctrl+Shift+T")
                    .build(app)?;
            let settings_item = MenuItemBuilder::with_id("settings", "Settings…").build(app)?;
            let autostart_item = CheckMenuItemBuilder::with_id("autostart", "Start with Windows")
                .checked(services::autostart::is_enabled())
                .build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&open, &translate_clip, &settings_item])
                .separator()
                .item(&autostart_item)
                .separator()
                .item(&quit)
                .build()?;

            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("LingvoLoc")
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let Some(window) = tray.app_handle().get_webview_window("main") else {
                            return;
                        };
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                        } else {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .on_menu_event(move |app, event| {
                    if event.id().as_ref() == "translate_clipboard" {
                        translate_clipboard(app);
                        return;
                    }
                    match event.id().as_ref() {
                        "open" | "settings" => {
                            let Some(window) = app.get_webview_window("main") else {
                                return;
                            };
                            let _ = window.show();
                            let _ = window.set_focus();
                            if event.id().as_ref() == "settings" {
                                let _ = app.emit("open-settings", ());
                            }
                        }
                        "autostart" => {
                            let enabled = autostart_item.is_checked().unwrap_or(false);
                            if services::autostart::set_enabled(enabled).is_err() {
                                let _ = autostart_item.set_checked(!enabled);
                            }
                        }
                        "quit" => app.exit(0),
                        _ => {}
                    }
                });
            if let Some(icon) = app.default_window_icon().cloned() {
                tray = tray.icon(icon);
            }
            tray.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_runtime_status,
            list_models,
            get_backend_capabilities,
            refresh_provider_models,
            refresh_deepl_languages,
            test_provider_connection,
            get_session_usage,
            translate,
            get_settings,
            get_api_token,
            write_clipboard,
            update_settings,
            get_provider_credential_status,
            save_provider_credential,
            delete_provider_credential,
            detect_language,
            check_llama_server,
            download_llama_cpp,
            get_gpu_info,
            get_llama_devices,
            llama_path_status,
            add_llama_to_path,
            find_llama_server,
            locate_llama_server,
            list_history,
            clear_history,
            set_history_favorite,
            export_history,
            take_clipboard_request,
            lookup_lexicon,
            read_dictionary_media,
            list_user_dictionaries,
            translate_word,
            documents::commands::analyze_txt,
            documents::commands::analyze_docx,
            documents::commands::analyze_epub,
            documents::commands::fb2::analyze_fb2,
            documents::commands::start_txt_job,
            documents::commands::start_docx_job,
            documents::commands::start_epub_job,
            documents::commands::fb2::start_fb2_job,
            documents::commands::resume_txt_job,
            documents::commands::resume_docx_job,
            documents::commands::resume_epub_job,
            documents::commands::fb2::resume_fb2_job,
            documents::commands::get_document_job,
            documents::commands::list_document_jobs,
            documents::commands::get_document_progress,
            documents::commands::pause_document_job,
            documents::commands::cancel_document_job,
            documents::commands::clear_document_job,
            documents::commands::export_txt_job,
            documents::commands::export_docx_job,
            documents::commands::export_epub_job,
            documents::commands::fb2::export_fb2_job,
            documents::commands::pdf::analyze_pdf,
            documents::commands::pdf::start_pdf_job,
            documents::commands::pdf::resume_pdf_job,
            documents::commands::pdf::export_pdf_job
        ])
        .build(tauri::generate_context!())
        .expect("error while building LingvoLoc")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = _app.try_state::<AppState>() {
                    state.inference.begin_shutdown();
                    runtimes::llama_server::interrupt("application exit");
                    state.inference.wait_for_idle();
                } else {
                    runtimes::llama_server::interrupt("application exit");
                }
            }
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn application_library_loads() {
        assert_eq!(2 + 2, 4);
    }
}
