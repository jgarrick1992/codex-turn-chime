import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  Diagnostics,
  HookPreview,
  MonitorEvent,
  SoundPayload,
  TaskState,
} from "./types";

export const bridge = {
  listTasks: () => invoke<TaskState[]>("list_tasks"),
  listEvents: (sessionId: string, turnId: string) =>
    invoke<MonitorEvent[]>("list_events", { sessionId, turnId }),
  markRead: (sessionId: string, turnId: string) =>
    invoke<void>("mark_task_read", { sessionId, turnId }),
  markAllRead: () => invoke<void>("mark_all_tasks_read"),
  clearHistory: () => invoke<void>("clear_history"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) =>
    invoke<AppSettings>("save_settings", { settings }),
  diagnostics: () => invoke<Diagnostics>("get_diagnostics"),
  previewHook: () => invoke<HookPreview>("preview_hook_install"),
  installHook: () => invoke<HookPreview>("install_hook"),
  uninstallHook: () => invoke<HookPreview>("uninstall_hook"),
  readSound: (path: string) => invoke<SoundPayload>("read_sound_file", { path }),
  onMonitorEvent: (handler: (event: MonitorEvent) => void): Promise<UnlistenFn> =>
    listen<MonitorEvent>("monitor-event", ({ payload }) => handler(payload)),
  onSettingsChanged: (handler: (settings: AppSettings) => void): Promise<UnlistenFn> =>
    listen<AppSettings>("settings-changed", ({ payload }) => handler(payload)),
  onWatcherDisabled: (handler: (message: string) => void): Promise<UnlistenFn> =>
    listen<string>("watcher-disabled", ({ payload }) => handler(payload)),
  onDismissReminder: (handler: () => void): Promise<UnlistenFn> =>
    listen("dismiss-reminder", handler),
};

export function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}
