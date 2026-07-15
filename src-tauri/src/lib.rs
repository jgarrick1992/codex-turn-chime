mod commands;
mod error;
pub mod hook_helper;
mod hooks;
mod monitor;
mod paths;
mod persistence;
mod queue;
mod settings;
mod shortcuts;
mod watcher;

use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_global_shortcut::ShortcutState;

use crate::{
    error::AppResult,
    paths::AppPaths,
    persistence::Database,
    watcher::{SharedWatcherHealth, WatcherEngine, WatcherHealth},
};

pub struct AppState {
    paths: AppPaths,
    database: Database,
    watcher_health: SharedWatcherHealth,
    shortcut_status: shortcuts::SharedShortcutStatus,
}

fn show_main_window(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.show();
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn start_monitor_worker(
    app: tauri::AppHandle,
    paths: AppPaths,
    database: Database,
    health: SharedWatcherHealth,
) {
    thread::spawn(move || {
        let mut watcher = WatcherEngine::default();
        let mut last_watcher_scan = Instant::now() - Duration::from_secs(5);
        let mut last_cleanup = Instant::now() - Duration::from_secs(3600);
        loop {
            match queue::drain(&paths.queue) {
                Ok(drained) => {
                    if drained.rejected_lines > 0 {
                        tracing::warn!(
                            count = drained.rejected_lines,
                            "rejected invalid local queue records"
                        );
                    }
                    if !drained.events.is_empty() {
                        match database.record_events(&drained.events) {
                            Ok(events) => {
                                for event in events {
                                    let _ = app.emit("monitor-event", event);
                                }
                            }
                            Err(error) => {
                                tracing::error!(%error, "failed to persist hook events; returning them to the local queue");
                                for event in drained.events {
                                    if let Err(queue_error) = queue::append(&paths.queue, &event) {
                                        tracing::error!(%queue_error, "failed to restore an event to the local queue");
                                    }
                                }
                            }
                        }
                    }
                }
                Err(error) => tracing::error!(%error, "failed to drain local hook queue"),
            }

            if last_watcher_scan.elapsed() >= Duration::from_secs(2) {
                last_watcher_scan = Instant::now();
                let app_settings = settings::load(&paths.settings).unwrap_or_default();
                let compatible = health.lock().map(|value| value.compatible).unwrap_or(false);
                if app_settings.transcript_watcher_enabled && compatible {
                    let result = paths::codex_home().and_then(|home| watcher.scan_all(&home, &database));
                    match result {
                        Ok(events) => match database.record_events(&events) {
                            Ok(events) => {
                                for event in events {
                                    let _ = app.emit("monitor-event", event);
                                }
                            }
                            Err(error) => tracing::error!(%error, "failed to persist transcript events"),
                        },
                        Err(error::AppError::IncompatibleFormat(message)) => {
                            if let Ok(mut value) = health.lock() {
                                value.compatible = false;
                                value.message = Some(message.clone());
                            }
                            let _ = app.emit("watcher-disabled", message);
                        }
                        Err(error) => tracing::warn!(%error, "transcript watcher scan failed"),
                    }
                }
            }

            if last_cleanup.elapsed() >= Duration::from_secs(3600) {
                last_cleanup = Instant::now();
                let days = settings::load(&paths.settings)
                    .unwrap_or_default()
                    .history_retention_days;
                if let Err(error) = database.cleanup_retention(days) {
                    tracing::warn!(%error, "history retention cleanup failed");
                }
            }
            thread::sleep(Duration::from_millis(450));
        }
    });
}

fn initialize() -> AppResult<(AppPaths, Database, SharedWatcherHealth)> {
    let paths = AppPaths::discover()?;
    paths.ensure()?;
    if !paths.settings.exists() {
        settings::save(&paths.settings, &settings::AppSettings::default())?;
    } else {
        settings::load(&paths.settings)?;
    }
    let database = Database::open(&paths.database)?;
    let health = Arc::new(Mutex::new(WatcherHealth::default()));
    Ok((paths, database, health))
}

pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .try_init();
    let (paths, database, watcher_health) =
        initialize().expect("failed to initialize CodexTurnChime local storage");
    let worker_paths = paths.clone();
    let worker_database = database.clone();
    let worker_health = watcher_health.clone();
    let shortcut_status = shortcuts::new_status();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        let _ = app.emit("dismiss-reminder", ());
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ))
        .manage(AppState {
            paths,
            database,
            watcher_health,
            shortcut_status,
        })
        .setup(move |app| {
            let state = app.state::<AppState>();
            if let Ok(app_settings) = settings::load(&state.paths.settings) {
                shortcuts::register_initial(
                    app.handle(),
                    &state.shortcut_status,
                    app_settings.dismiss_reminder_shortcut.as_deref(),
                );
            }
            let open = MenuItem::with_id(app, "open", "Open Dashboard", true, None::<&str>)?;
            let mute = MenuItem::with_id(app, "mute", "Mute / Unmute", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Quit CodexTurnChime", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &mute, &separator, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        show_main_window(tray.app_handle());
                    }
                })
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main_window(app),
                    "mute" => {
                        let state = app.state::<AppState>();
                        if let Ok(mut value) = settings::load(&state.paths.settings) {
                            value.muted = !value.muted;
                            let _ = settings::save(&state.paths.settings, &value);
                            let _ = app.emit("settings-changed", value);
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            if std::env::args().any(|argument| argument == "--background") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            start_monitor_worker(
                app.handle().clone(),
                worker_paths.clone(),
                worker_database.clone(),
                worker_health.clone(),
            );
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_tasks,
            commands::list_events,
            commands::mark_task_read,
            commands::mark_all_tasks_read,
            commands::clear_history,
            commands::get_settings,
            commands::save_settings,
            commands::preview_hook_install,
            commands::install_hook,
            commands::uninstall_hook,
            commands::get_diagnostics,
            commands::read_sound_file,
        ])
        .build(tauri::generate_context!())
        .expect("error while building CodexTurnChime")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                show_main_window(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}
