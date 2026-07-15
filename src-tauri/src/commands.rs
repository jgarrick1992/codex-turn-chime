use std::{fs, path::Path};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

use crate::{
    error::{AppError, AppResult},
    hooks::{self, HookPreview},
    monitor::{MonitorEvent, TaskSnapshot},
    paths, queue,
    settings::{self, AppSettings},
    AppState,
};

#[derive(Debug, Serialize)]
pub struct Diagnostics {
    app_data_dir: String,
    codex_home: String,
    hook_config_path: String,
    hook_installed: bool,
    helper_found: bool,
    queue_readable: bool,
    database_ready: bool,
    watcher_enabled: bool,
    watcher_compatible: bool,
    watcher_message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SoundPayload {
    mime: &'static str,
    base64: String,
}

#[tauri::command]
pub fn list_tasks(state: State<'_, AppState>) -> AppResult<Vec<TaskSnapshot>> {
    state.database.list_tasks()
}

#[tauri::command(rename_all = "camelCase")]
pub fn list_events(
    state: State<'_, AppState>,
    session_id: String,
    turn_id: String,
) -> AppResult<Vec<MonitorEvent>> {
    state.database.list_events(&session_id, &turn_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn mark_task_read(state: State<'_, AppState>, session_id: String, turn_id: String) -> AppResult<()> {
    state.database.mark_read(&session_id, &turn_id)
}

#[tauri::command]
pub fn clear_history(state: State<'_, AppState>) -> AppResult<()> {
    state.database.clear_history()
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppResult<AppSettings> {
    settings::load(&state.paths.settings)
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> AppResult<AppSettings> {
    settings.validate()?;
    if settings.launch_at_login {
        app.autolaunch()
            .enable()
            .map_err(|error| AppError::InvalidConfig(error.to_string()))?;
    } else if app.autolaunch().is_enabled().unwrap_or(false) {
        app.autolaunch()
            .disable()
            .map_err(|error| AppError::InvalidConfig(error.to_string()))?;
    }
    let old = settings::load(&state.paths.settings).unwrap_or_default();
    if settings.transcript_watcher_enabled && !old.transcript_watcher_enabled {
        let mut health = state
            .watcher_health
            .lock()
            .map_err(|_| AppError::InvalidConfig("watcher health lock is poisoned".into()))?;
        health.compatible = true;
        health.message = None;
    }
    settings::save(&state.paths.settings, &settings)?;
    Ok(settings)
}

#[tauri::command]
pub fn preview_hook_install() -> AppResult<HookPreview> {
    hooks::preview_install()
}

#[tauri::command]
pub fn install_hook(state: State<'_, AppState>) -> AppResult<HookPreview> {
    hooks::install(&state.paths.backups_dir)
}

#[tauri::command]
pub fn uninstall_hook(state: State<'_, AppState>) -> AppResult<HookPreview> {
    hooks::uninstall(&state.paths.backups_dir)
}

#[tauri::command]
pub fn get_diagnostics(state: State<'_, AppState>) -> AppResult<Diagnostics> {
    let app_settings = settings::load(&state.paths.settings)?;
    let health = state
        .watcher_health
        .lock()
        .map_err(|_| AppError::InvalidConfig("watcher health lock is poisoned".into()))?
        .clone();
    let codex_home = paths::codex_home()?;
    let hook_config = hooks::hook_config_path()?;
    Ok(Diagnostics {
        app_data_dir: state.paths.data_dir.to_string_lossy().into_owned(),
        codex_home: codex_home.to_string_lossy().into_owned(),
        hook_config_path: hook_config.to_string_lossy().into_owned(),
        hook_installed: hooks::is_installed(),
        helper_found: hooks::helper_path().is_ok_and(|path| path.is_file()),
        queue_readable: queue::is_readable(&state.paths.queue),
        database_ready: state.database.is_ready(),
        watcher_enabled: app_settings.transcript_watcher_enabled,
        watcher_compatible: health.compatible,
        watcher_message: health.message,
    })
}

#[tauri::command]
pub fn read_sound_file(path: String) -> AppResult<SoundPayload> {
    let path = Path::new(&path);
    let metadata = fs::metadata(path).map_err(|error| AppError::Sound(error.to_string()))?;
    if !metadata.is_file() || metadata.len() > 25 * 1024 * 1024 {
        return Err(AppError::Sound(
            "sound must be a readable file no larger than 25 MiB".into(),
        ));
    }
    let bytes = fs::read(path).map_err(|error| AppError::Sound(error.to_string()))?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime = match extension.as_str() {
        "wav" if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" => "audio/wav",
        "mp3" if bytes.starts_with(b"ID3") || bytes.first().is_some_and(|first| *first == 0xff) => {
            "audio/mpeg"
        }
        "wav" | "mp3" => return Err(AppError::Sound("file signature does not match WAV or MP3".into())),
        _ => return Err(AppError::Sound("only WAV and MP3 files are supported".into())),
    };
    Ok(SoundPayload {
        mime,
        base64: STANDARD.encode(bytes),
    })
}
