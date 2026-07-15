use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::{
    error::{AppError, AppResult},
    monitor::{MonitorEvent, MonitorKind, MonitorSource, MONITOR_SCHEMA_VERSION},
    persistence::Database,
};

pub const ADAPTER_VERSION: &str = "codex-jsonl-v1";

#[derive(Clone, Debug)]
pub struct WatcherHealth {
    pub compatible: bool,
    pub message: Option<String>,
}

impl Default for WatcherHealth {
    fn default() -> Self {
        Self {
            compatible: true,
            message: None,
        }
    }
}

pub type SharedWatcherHealth = Arc<Mutex<WatcherHealth>>;

#[derive(Debug, Deserialize)]
struct TranscriptEnvelope {
    timestamp: String,
    #[serde(rename = "type")]
    record_type: String,
    payload: Value,
}

#[derive(Default)]
pub struct WatcherEngine {
    pending_input_calls: HashMap<(String, String), HashSet<String>>,
}

impl WatcherEngine {
    pub fn scan_all(&mut self, codex_home: &Path, database: &Database) -> AppResult<Vec<MonitorEvent>> {
        let sessions = codex_home.join("sessions");
        if !sessions.exists() {
            return Ok(Vec::new());
        }
        let mut events = Vec::new();
        for entry in WalkDir::new(sessions)
            .max_depth(8)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("jsonl"))
        {
            events.extend(self.scan_file(entry.path(), database)?);
        }
        Ok(events)
    }

    fn scan_file(&mut self, path: &Path, database: &Database) -> AppResult<Vec<MonitorEvent>> {
        let metadata = path.metadata()?;
        let identity = file_identity(path, &metadata)?;
        let path_text = path.to_string_lossy().into_owned();
        let (saved_identity, saved_offset) = database
            .checkpoint(&path_text)?
            .unwrap_or_else(|| (identity.clone(), 0));
        let mut offset = if saved_identity == identity && metadata.len() >= saved_offset {
            saved_offset
        } else {
            0
        };
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(offset))?;
        let session_id = path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::IncompatibleFormat("transcript filename is not valid UTF-8".into()))?
            .to_owned();
        let mut events = Vec::new();
        loop {
            let line_offset = offset;
            let mut line = Vec::new();
            let read = reader.read_until(b'\n', &mut line)?;
            if read == 0 {
                break;
            }
            if !line.ends_with(b"\n") {
                break;
            }
            offset += read as u64;
            let line = std::str::from_utf8(&line)
                .map_err(|_| AppError::IncompatibleFormat("transcript line is not UTF-8".into()))?;
            if let Some(event) = self.parse_line(line, &session_id, &identity, line_offset)? {
                events.push(event);
            }
        }
        database.save_checkpoint(&path_text, &identity, offset)?;
        Ok(events)
    }

    fn parse_line(
        &mut self,
        line: &str,
        session_id: &str,
        file_identity: &str,
        offset: u64,
    ) -> AppResult<Option<MonitorEvent>> {
        let envelope: TranscriptEnvelope = serde_json::from_str(line.trim_end())
            .map_err(|error| AppError::IncompatibleFormat(format!("{ADAPTER_VERSION}: {error}")))?;
        let occurred_at = DateTime::parse_from_rfc3339(&envelope.timestamp)
            .map_err(|_| AppError::IncompatibleFormat(format!("{ADAPTER_VERSION}: invalid timestamp")))?
            .with_timezone(&Utc);
        if matches!(envelope.record_type.as_str(), "session_meta" | "turn_context") {
            return Ok(None);
        }
        if !matches!(envelope.record_type.as_str(), "event_msg" | "response_item") {
            return Err(AppError::IncompatibleFormat(format!(
                "{ADAPTER_VERSION}: unknown record type"
            )));
        }
        let payload_type = envelope
            .payload
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AppError::IncompatibleFormat(format!("{ADAPTER_VERSION}: payload.type is required"))
            })?;

        let fields = match (envelope.record_type.as_str(), payload_type) {
            ("event_msg", "task_started") => Some(event_fields(
                &envelope.payload,
                MonitorKind::Running,
                "task_started",
            )?),
            ("event_msg", "task_complete") => Some(event_fields(
                &envelope.payload,
                MonitorKind::Ready,
                "task_complete",
            )?),
            ("event_msg", "turn_aborted") => {
                let reason = required_string(&envelope.payload, "reason")?;
                let (kind, safe_reason) = match reason {
                    "interrupted" => (MonitorKind::Stopped, "interrupted"),
                    "failed" => (MonitorKind::Blocked, "explicit_failure"),
                    _ => {
                        return Err(AppError::IncompatibleFormat(format!(
                            "{ADAPTER_VERSION}: unknown turn_aborted reason"
                        )))
                    }
                };
                Some(event_fields(&envelope.payload, kind, safe_reason)?)
            }
            ("response_item", "function_call") => {
                if required_string(&envelope.payload, "name")? != "request_user_input" {
                    None
                } else {
                    let (turn_id, cwd, _) =
                        event_fields(&envelope.payload, MonitorKind::NeedsInput, "request_user_input")?;
                    let call_id = required_string(&envelope.payload, "call_id")?.to_owned();
                    self.pending_input_calls
                        .entry((session_id.to_owned(), turn_id.clone()))
                        .or_default()
                        .insert(call_id);
                    Some((turn_id, cwd, (MonitorKind::NeedsInput, "request_user_input")))
                }
            }
            ("response_item", "function_call_output") => {
                let turn_id = required_string(&envelope.payload, "turn_id")?.to_owned();
                let call_id = required_string(&envelope.payload, "call_id")?;
                let key = (session_id.to_owned(), turn_id.clone());
                let matched = self
                    .pending_input_calls
                    .get_mut(&key)
                    .is_some_and(|pending| pending.remove(call_id));
                if matched {
                    let cwd = required_string(&envelope.payload, "cwd")?.to_owned();
                    Some((turn_id, cwd, (MonitorKind::Running, "user_input_received")))
                } else {
                    None
                }
            }
            ("event_msg" | "response_item", _) => None,
            _ => unreachable!("record type was validated above"),
        };

        let Some((turn_id, cwd, (kind, reason))) = fields else {
            return Ok(None);
        };
        let event_id = stable_event_id(file_identity, offset, kind);
        Ok(Some(MonitorEvent {
            schema_version: MONITOR_SCHEMA_VERSION,
            event_id,
            source: MonitorSource::CodexTranscript,
            session_id: session_id.to_owned(),
            turn_id,
            kind,
            occurred_at,
            cwd,
            reason: Some(reason.to_owned()),
        }))
    }
}

fn required_string<'a>(value: &'a Value, key: &str) -> AppResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::IncompatibleFormat(format!("{ADAPTER_VERSION}: payload.{key} is required")))
}

fn event_fields(
    value: &Value,
    kind: MonitorKind,
    reason: &'static str,
) -> AppResult<(String, String, (MonitorKind, &'static str))> {
    Ok((
        required_string(value, "turn_id")?.to_owned(),
        required_string(value, "cwd")?.to_owned(),
        (kind, reason),
    ))
}

fn stable_event_id(file_identity: &str, offset: u64, kind: MonitorKind) -> String {
    let mut hasher = Sha256::new();
    hasher.update(file_identity.as_bytes());
    hasher.update(offset.to_le_bytes());
    hasher.update(kind.as_str().as_bytes());
    format!("transcript-{:x}", hasher.finalize())
}

fn file_identity(path: &Path, metadata: &std::fs::Metadata) -> AppResult<String> {
    let canonical = path.canonicalize().unwrap_or_else(|_| PathBuf::from(path));
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(format!(
            "{}:{}:{}",
            canonical.display(),
            metadata.dev(),
            metadata.ino()
        ));
    }
    #[cfg(not(unix))]
    {
        let created = metadata
            .created()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |value| value.as_nanos());
        Ok(format!("{}:{created}", canonical.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interruption_is_stopped_not_blocked() {
        let mut engine = WatcherEngine::default();
        let line = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"event_msg","payload":{"type":"turn_aborted","turn_id":"t","cwd":"/work","reason":"interrupted"}}"#;
        let event = engine.parse_line(line, "s", "file", 10).unwrap().unwrap();
        assert_eq!(event.kind, MonitorKind::Stopped);
    }

    #[test]
    fn request_output_returns_to_running_only_when_call_matches() {
        let mut engine = WatcherEngine::default();
        let call = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"response_item","payload":{"type":"function_call","name":"request_user_input","call_id":"c","turn_id":"t","cwd":"/work"}}"#;
        let output = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"response_item","payload":{"type":"function_call_output","call_id":"c","turn_id":"t","cwd":"/work"}}"#;
        assert_eq!(
            engine.parse_line(call, "s", "file", 1).unwrap().unwrap().kind,
            MonitorKind::NeedsInput
        );
        assert_eq!(
            engine.parse_line(output, "s", "file", 2).unwrap().unwrap().kind,
            MonitorKind::Running
        );
        assert!(engine.parse_line(output, "s", "file", 3).unwrap().is_none());
    }

    #[test]
    fn old_key_is_not_accepted_as_alias() {
        let mut engine = WatcherEngine::default();
        let line = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"event_msg","payload":{"type":"task_started","turnId":"t","cwd":"/work"}}"#;
        assert!(matches!(
            engine.parse_line(line, "s", "file", 1),
            Err(AppError::IncompatibleFormat(_))
        ));
    }
}
