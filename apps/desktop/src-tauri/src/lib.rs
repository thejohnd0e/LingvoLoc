mod adapters;
mod api;
mod domain;
mod runtimes;
mod services;

use domain::{
    DetectedLanguage, LocalModel, RuntimeError, RuntimeStatus, Settings, TranslationRequest,
    TranslationResult,
};
use services::{
    history::{HistoryEntry, HistoryStore},
    translation,
};
use std::sync::Mutex;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
    Manager, WindowEvent,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub struct AppState {
    pub(crate) settings: Mutex<Settings>,
    pub(crate) history: Mutex<HistoryStore>,
    pub(crate) pending_clipboard: Mutex<Option<String>>,
    pub(crate) api_token: String,
}

impl AppState {
    fn new(history: HistoryStore) -> Self {
        Self {
            settings: Mutex::new(Settings {
                endpoint: "http://127.0.0.1:1234/v1".into(),
                model_id: String::new(),
                adapter_id: "translategemma".into(),
                source_language: "auto".into(),
                target_language: "en".into(),
                primary_language: "en".into(),
                secondary_language: "ru".into(),
            }),
            history: Mutex::new(history),
            pending_clipboard: Mutex::new(None),
            api_token: api::generate_token(),
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

#[tauri::command]
fn get_runtime_status(state: tauri::State<'_, AppState>) -> Result<RuntimeStatus, RuntimeError> {
    translation::status(&settings(&state)?)
}

#[tauri::command]
fn list_models(state: tauri::State<'_, AppState>) -> Result<Vec<LocalModel>, RuntimeError> {
    translation::list_models(&settings(&state)?)
}

#[tauri::command]
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
    let result = translation::translate(&current, request.clone())?;
    state
        .history
        .lock()
        .map_err(|_| RuntimeError::Connection("history lock is poisoned".into()))?
        .add(&request.text, &request, &result)?;
    Ok(result)
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
    let mut current = state
        .settings
        .lock()
        .map_err(|_| RuntimeError::Connection("settings lock is poisoned".into()))?;
    *current = next.clone();
    Ok(next)
}

#[tauri::command]
fn detect_language(text: String) -> Result<DetectedLanguage, RuntimeError> {
    services::detection::detect_supported_language(&text)
}

#[tauri::command]
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

#[tauri::command]
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

#[tauri::command]
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

#[tauri::command]
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

#[tauri::command]
fn list_user_dictionaries(
    directory: Option<String>,
) -> Result<Vec<services::lexical::UserDictionary>, RuntimeError> {
    let directory = directory
        .filter(|value| !value.trim().is_empty())
        .map(std::path::PathBuf::from)
        .ok_or_else(|| RuntimeError::Connection("dictionary folder is not selected".into()))?;
    Ok(services::lexical::list_user_dictionaries(&directory))
}

#[tauri::command]
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
    translation::translate(&current, request)
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
                    let is_translate = shortcut == &handler_translate_shortcut;
                    let window = if is_translate {
                        app.get_webview_window("popup")
                    } else {
                        app.get_webview_window("main")
                    };
                    let Some(window) = window else {
                        return;
                    };
                    let _ = window.show();
                    let _ = window.set_focus();
                    if is_translate {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            if let Ok(text) = clipboard.get_text() {
                                if let Ok(mut pending) =
                                    app.state::<AppState>().pending_clipboard.lock()
                                {
                                    *pending = Some(text);
                                }
                            }
                        }
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
            api::start(app.handle().clone());
            app.global_shortcut().register(show_shortcut)?;
            app.global_shortcut().register(translate_shortcut)?;

            let show = MenuItemBuilder::with_id("show", "Show LingvoLoc").build(app)?;
            let hide = MenuItemBuilder::with_id("hide", "Hide LingvoLoc").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show, &hide])
                .separator()
                .item(&quit)
                .build()?;

            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("LingvoLoc")
                .on_menu_event(|app, event| {
                    let Some(window) = app.get_webview_window("main") else {
                        return;
                    };
                    match event.id().as_ref() {
                        "show" => {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                        "hide" => {
                            let _ = window.hide();
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
            translate,
            get_settings,
            get_api_token,
            write_clipboard,
            update_settings,
            detect_language,
            list_history,
            clear_history,
            set_history_favorite,
            export_history,
            take_clipboard_request,
            lookup_lexicon,
            list_user_dictionaries,
            translate_word
        ])
        .run(tauri::generate_context!())
        .expect("error while running LingvoLoc");
}

#[cfg(test)]
mod tests {
    #[test]
    fn application_library_loads() {
        assert_eq!(2 + 2, 4);
    }
}
