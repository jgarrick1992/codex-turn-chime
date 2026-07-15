export type MonitorKind =
  | "running"
  | "needs_input"
  | "ready"
  | "stopped"
  | "blocked"
  | "unknown";

export type MonitorSource = "codex_hook" | "codex_transcript";

export interface MonitorEvent {
  schema_version: 1;
  event_id: string;
  source: MonitorSource;
  session_id: string;
  turn_id: string;
  kind: MonitorKind;
  occurred_at: string;
  cwd: string;
  reason: string | null;
}

export interface TaskState {
  session_id: string;
  turn_id: string;
  current_kind: MonitorKind;
  last_event_id: string;
  last_event_at: string;
  cwd: string;
  source: MonitorSource;
  reason: string | null;
  is_read: boolean;
}

export interface SoundSetting {
  enabled: boolean;
  path: string | null;
  volume: number;
}

export interface AppSettings {
  language: "en" | "zh-CN";
  muted: boolean;
  launch_at_login: boolean;
  transcript_watcher_enabled: boolean;
  history_retention_days: 30;
  onboarding_complete: boolean;
  needs_input_sound: SoundSetting;
  ready_sound: SoundSetting;
}

export interface Diagnostics {
  app_data_dir: string;
  codex_home: string;
  hook_config_path: string;
  hook_installed: boolean;
  helper_found: boolean;
  queue_readable: boolean;
  database_ready: boolean;
  watcher_enabled: boolean;
  watcher_compatible: boolean;
  watcher_message: string | null;
}

export interface HookPreview {
  config_path: string;
  backup_path: string | null;
  before_json: string;
  after_json: string;
  diff: string;
  already_installed: boolean;
}

export interface SoundPayload {
  mime: "audio/wav" | "audio/mpeg";
  base64: string;
}

export type Section =
  | "all"
  | MonitorKind
  | "settings"
  | "diagnostics";
