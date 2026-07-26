# Architecture

## Trust boundary

CodexTurnChime is a local desktop process. v0.1 has no account, remote API, telemetry, crash upload, or network synchronization. The only inputs are official Codex Hook JSON, optional local transcript JSONL, user settings, and explicitly selected audio files.

## Components

1. `codex-turn-chime-hook` reads one Hook JSON object from stdin, maps an exact official event name, appends one `MonitorEvent v1` line under a file lock, and exits. Every failure still returns process exit code 0 so monitoring cannot stop Codex.
2. A dedicated queue worker drains complete JSONL lines every 450 ms, rejects invalid records, preserves an incomplete trailing line, and returns events to the queue if SQLite persistence fails. Transcript work and retention cleanup run on separate workers and cannot delay Hook delivery.
3. SQLite deduplicates `event_id`, rejects schema versions other than 1, applies timestamp ordering, and materializes current task state.
4. The optional watcher uses filesystem Create/Modify notifications with 250 ms debounce, plus a 60-second metadata-only reconciliation. It baselines existing files at EOF, persists parser context and pending input state, and reads changed files in bounded 8 MiB/2,000-line batches.
5. The Tauri shell emits accepted events to the React webview, which updates the dashboard and plays local audio. The webview cannot execute arbitrary shell commands.

## Event contract

`MonitorEvent v1` has exactly these fields:

| Field | Type | Notes |
| --- | --- | --- |
| `schema_version` | integer | Must equal `1` |
| `event_id` | string | Hook UUID or deterministic transcript ID |
| `source` | enum | `codex_hook` or `codex_transcript` |
| `session_id` | string | Required |
| `turn_id` | string | Required |
| `kind` | enum | One of the six defined states |
| `occurred_at` | RFC 3339 UTC | Event time, not receive time |
| `cwd` | string | Working directory only |
| `reason` | string or null | Privacy-safe reason code only |

Unknown fields are rejected in the internal event queue and settings file. No compatibility aliases exist.

## State transitions

| Input | State |
| --- | --- |
| `UserPromptSubmit`, `task_started` | `running` |
| `PermissionRequest`, unmatched `request_user_input` call | `needs_input` |
| Matching `function_call_output` | `running` |
| `Stop`, `task_complete` | `ready` |
| `turn_aborted(reason: interrupted)` | `stopped` |
| Explicit failure | `blocked` |
| Unrecognized internal state | `unknown` or adapter disabled |

Late events cannot replace a newer task snapshot. Equal-time events use `event_id` as a deterministic tie-breaker. Global tray priority is Needs input, Blocked, Ready, Running, Stopped, Unknown.

## Storage

macOS: `~/Library/Application Support/io.github.jgarrick1992.codexturnchime/`

macOS development: `~/Library/Application Support/io.github.jgarrick1992.codexturnchime.dev/`

Windows: `%APPDATA%\io.github.jgarrick1992.codexturnchime\`

The directory contains `codex-turn-chime.db`, `settings.json`, `hook-events.jsonl`, `logs/`, and `backups/`. Database tables are `monitor_events`, `task_states`, `watcher_checkpoints`, and `watcher_pending_inputs`. History retention is fixed at 30 days in v0.1. CodexTurnChime never queries, vacuums, or writes Codex's own SQLite databases.
