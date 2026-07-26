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
    collections::HashSet,
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use notify::{EventKind, RecursiveMode, Watcher};
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
    watcher_control: mpsc::Sender<bool>,
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

fn start_hook_worker(app: tauri::AppHandle, paths: AppPaths, database: Database) {
    thread::spawn(move || loop {
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

        thread::sleep(Duration::from_millis(450));
    });
}

fn start_watcher_worker(
    app: tauri::AppHandle,
    database: Database,
    health: SharedWatcherHealth,
    control_rx: mpsc::Receiver<bool>,
    mut requested: bool,
) {
    thread::spawn(move || {
        let codex_home = match paths::codex_home() {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(%error, "cannot resolve Codex home for transcript watcher");
                return;
            }
        };
        let sessions = codex_home.join("sessions");
        let (notify_tx, notify_rx) = mpsc::channel();
        let notify_callback_tx = notify_tx.clone();
        let mut filesystem_watcher = match notify::recommended_watcher(move |result| {
            let _ = notify_callback_tx.send(result);
        }) {
            Ok(watcher) => Some(watcher),
            Err(error) => {
                tracing::warn!(%error, "failed to initialize filesystem transcript watcher; using reconciliation only");
                None
            }
        };
        let mut watched_sessions = false;
        let mut enabled = false;
        let mut engine: Option<WatcherEngine> = None;
        let mut pending_paths = HashSet::<PathBuf>::new();
        let mut last_notification: Option<Instant> = None;
        let mut last_reconcile = Instant::now();
        let _notify_channel_keepalive = notify_tx;

        loop {
            for next in control_rx.try_iter() {
                requested = next;
            }
            match notify_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(Ok(event)) => {
                    if enabled && matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                        for path in event.paths {
                            if watcher::is_transcript(&path) {
                                pending_paths.insert(path);
                                last_notification = Some(Instant::now());
                            }
                        }
                    }
                }
                Ok(Err(error)) => tracing::warn!(%error, "filesystem transcript watcher error"),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    tracing::warn!("filesystem transcript watcher disconnected; using reconciliation only");
                    filesystem_watcher = None;
                }
            }
            for result in notify_rx.try_iter() {
                match result {
                    Ok(event)
                        if enabled && matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) =>
                    {
                        for path in event.paths {
                            if watcher::is_transcript(&path) {
                                pending_paths.insert(path);
                                last_notification = Some(Instant::now());
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "filesystem transcript watcher error"),
                }
            }

            if !sessions.exists() {
                watched_sessions = false;
            } else if !watched_sessions {
                if let Some(filesystem_watcher) = filesystem_watcher.as_mut() {
                    match filesystem_watcher.watch(&sessions, RecursiveMode::Recursive) {
                        Ok(()) => watched_sessions = true,
                        Err(error) => tracing::warn!(%error, "failed to watch Codex sessions directory"),
                    }
                }
            }

            let compatible = health.lock().map(|value| value.compatible).unwrap_or(false);
            let should_enable = requested && compatible;
            if should_enable && !enabled {
                match WatcherEngine::load(&database).and_then(|mut loaded| {
                    loaded.baseline_existing(&sessions, &database)?;
                    let changed = loaded.reconcile(&sessions, &database)?;
                    Ok((loaded, changed))
                }) {
                    Ok((loaded, changed)) => {
                        engine = Some(loaded);
                        pending_paths.clear();
                        pending_paths.extend(changed);
                        last_notification = if pending_paths.is_empty() {
                            None
                        } else {
                            Some(Instant::now() - Duration::from_millis(250))
                        };
                        last_reconcile = Instant::now();
                        enabled = true;
                    }
                    Err(error::AppError::IncompatibleFormat(message)) => {
                        disable_watcher(&app, &health, message);
                    }
                    Err(error) => {
                        tracing::warn!(%error, "failed to establish transcript watcher baseline")
                    }
                }
            } else if !should_enable && enabled {
                enabled = false;
                engine = None;
                pending_paths.clear();
                last_notification = None;
            }

            if enabled && last_reconcile.elapsed() >= Duration::from_secs(60) {
                last_reconcile = Instant::now();
                if let Some(engine) = engine.as_mut() {
                    match engine.reconcile(&sessions, &database) {
                        Ok(paths) => {
                            if !paths.is_empty() {
                                pending_paths.extend(paths);
                                last_notification = Some(Instant::now() - Duration::from_millis(250));
                            }
                        }
                        Err(error) => tracing::warn!(%error, "transcript metadata reconciliation failed"),
                    }
                }
            }

            let debounce_elapsed =
                last_notification.is_some_and(|instant| instant.elapsed() >= Duration::from_millis(250));
            if enabled && debounce_elapsed && !pending_paths.is_empty() {
                let paths = pending_paths.drain().collect::<Vec<_>>();
                last_notification = None;
                let Some(active_engine) = engine.as_mut() else {
                    continue;
                };
                match active_engine.scan_paths(paths.clone(), &database) {
                    Ok(batch) => {
                        if !batch.continuations.is_empty() {
                            pending_paths.extend(batch.continuations);
                            last_notification = Some(Instant::now() - Duration::from_millis(250));
                        }
                        for event in batch.events {
                            let _ = app.emit("monitor-event", event);
                        }
                    }
                    Err(error::AppError::IncompatibleFormat(message)) => {
                        enabled = false;
                        engine = None;
                        pending_paths.clear();
                        last_notification = None;
                        disable_watcher(&app, &health, message);
                    }
                    Err(error) => {
                        pending_paths.extend(paths);
                        last_notification = Some(Instant::now() - Duration::from_millis(250));
                        tracing::warn!(%error, "transcript watcher scan failed; queued for retry");
                    }
                }
            }
        }
    });
}

fn disable_watcher(app: &tauri::AppHandle, health: &SharedWatcherHealth, message: String) {
    if let Ok(mut value) = health.lock() {
        value.compatible = false;
        value.message = Some(message.clone());
    }
    let _ = app.emit("watcher-disabled", message);
}

fn start_maintenance_worker(paths: AppPaths, database: Database) {
    thread::spawn(move || loop {
        let days = settings::load(&paths.settings)
            .unwrap_or_default()
            .history_retention_days;
        if let Err(error) = database.cleanup_retention(days) {
            tracing::warn!(%error, "history retention cleanup failed");
        }
        thread::sleep(Duration::from_secs(3600));
    });
}

fn start_monitor_workers(
    app: tauri::AppHandle,
    paths: AppPaths,
    database: Database,
    health: SharedWatcherHealth,
    watcher_control_rx: mpsc::Receiver<bool>,
    watcher_requested: bool,
) {
    start_hook_worker(app.clone(), paths.clone(), database.clone());
    start_watcher_worker(
        app,
        database.clone(),
        health,
        watcher_control_rx,
        watcher_requested,
    );
    start_maintenance_worker(paths, database);
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
    let watcher_requested = settings::load(&paths.settings)
        .unwrap_or_default()
        .transcript_watcher_enabled;
    let (watcher_control, watcher_control_rx) = mpsc::channel();
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
            watcher_control,
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
            start_monitor_workers(
                app.handle().clone(),
                worker_paths.clone(),
                worker_database.clone(),
                worker_health.clone(),
                watcher_control_rx,
                watcher_requested,
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
