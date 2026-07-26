use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::{
    error::{AppError, AppResult},
    monitor::{MonitorEvent, MonitorKind, MonitorSource, MONITOR_SCHEMA_VERSION},
    persistence::{Database, WatcherCheckpoint, WatcherPendingInputCall, WatcherState},
};

pub const ADAPTER_VERSION: &str = "codex-jsonl-v2";
pub const MAX_SCAN_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_SCAN_LINES: usize = 2_000;
const MAX_RETAINED_LINE_BYTES: usize = 64 * 1024;

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
struct TranscriptEnvelope<'a> {
    timestamp: &'a str,
    #[serde(rename = "type")]
    record_type: &'a str,
    #[serde(borrow)]
    payload: &'a RawValue,
}

#[derive(Debug, Deserialize)]
struct PayloadKind<'a> {
    #[serde(rename = "type")]
    payload_type: &'a str,
}

#[derive(Debug, Deserialize)]
struct SessionMetaPayload {
    cwd: String,
}

#[derive(Debug, Deserialize)]
struct TurnContextPayload {
    cwd: String,
    turn_id: String,
}

#[derive(Debug, Deserialize)]
struct TurnPayload {
    turn_id: String,
}

#[derive(Debug, Deserialize)]
struct AbortedPayload {
    turn_id: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct FunctionCallPayload {
    name: String,
    call_id: String,
}

#[derive(Debug, Deserialize)]
struct FunctionCallOutputPayload {
    call_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TranscriptContext {
    cwd: Option<String>,
    turn_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingInputCall {
    file_identity: String,
    cwd: String,
    event_id: String,
    turn_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Checkpoint {
    file_identity: String,
    byte_offset: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PartialLineProgress {
    checkpoint_offset: u64,
    discarded_bytes: u64,
}

#[derive(Debug, Default)]
pub struct ScanBatch {
    pub events: Vec<MonitorEvent>,
    pub continuations: Vec<PathBuf>,
}

#[derive(Clone, Default)]
pub struct WatcherEngine {
    checkpoints: HashMap<String, Checkpoint>,
    contexts: HashMap<String, TranscriptContext>,
    pending_input_calls: HashMap<(String, String), PendingInputCall>,
    partial_line_progress: HashMap<String, PartialLineProgress>,
}

impl WatcherEngine {
    pub fn load(database: &Database) -> AppResult<Self> {
        let state = database.load_watcher_state()?;
        let mut checkpoints = HashMap::new();
        let mut contexts = HashMap::new();
        for checkpoint in state.checkpoints {
            contexts.insert(
                checkpoint.file_identity.clone(),
                TranscriptContext {
                    cwd: checkpoint.cwd,
                    turn_id: checkpoint.turn_id,
                },
            );
            checkpoints.insert(
                checkpoint.source_path,
                Checkpoint {
                    file_identity: checkpoint.file_identity,
                    byte_offset: checkpoint.byte_offset,
                },
            );
        }
        Ok(Self {
            checkpoints,
            contexts,
            pending_input_calls: state
                .pending_input_calls
                .into_iter()
                .map(|pending| {
                    (
                        (pending.session_id, pending.call_id),
                        PendingInputCall {
                            file_identity: pending.file_identity,
                            cwd: pending.cwd,
                            event_id: pending.event_id,
                            turn_id: pending.turn_id,
                        },
                    )
                })
                .collect(),
            partial_line_progress: HashMap::new(),
        })
    }

    /// Establishes a metadata-only EOF baseline for files that already exist when the
    /// watcher starts. Historical events are never returned and file contents are not read.
    pub fn baseline_existing(&mut self, sessions: &Path, database: &Database) -> AppResult<()> {
        if !sessions.exists() {
            return Ok(());
        }
        let previous = self.clone();
        let result = (|| {
            let mut seen = HashSet::new();
            for path in transcript_paths(sessions) {
                let metadata = match path.metadata() {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error.into()),
                };
                let identity = file_identity(&path, &metadata)?;
                let path_text = path.to_string_lossy().into_owned();
                self.relocate_checkpoint(&path_text, &identity);
                seen.insert(path_text.clone());
                let checkpoint_is_valid = self.checkpoints.get(&path_text).is_some_and(|checkpoint| {
                    checkpoint.file_identity == identity && metadata.len() >= checkpoint.byte_offset
                });
                if !checkpoint_is_valid {
                    self.clear_file_state(&path, &identity);
                    self.checkpoints.insert(
                        path_text,
                        Checkpoint {
                            file_identity: identity,
                            byte_offset: metadata.len(),
                        },
                    );
                }
            }
            self.remove_missing_paths(&seen);
            self.save_state(database)
        })();
        if result.is_err() {
            *self = previous;
        }
        result
    }

    /// Returns files whose metadata differs from the persisted checkpoint. This is the
    /// low-frequency safety net for missed filesystem notifications.
    pub fn reconcile(&mut self, sessions: &Path, database: &Database) -> AppResult<Vec<PathBuf>> {
        if !sessions.exists() {
            return Ok(Vec::new());
        }
        let previous = self.clone();
        let result = (|| {
            let mut changed = Vec::new();
            let mut seen = HashSet::new();
            for path in transcript_paths(sessions) {
                let metadata = match path.metadata() {
                    Ok(metadata) => metadata,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error.into()),
                };
                let identity = file_identity(&path, &metadata)?;
                let path_text = path.to_string_lossy().into_owned();
                self.relocate_checkpoint(&path_text, &identity);
                seen.insert(path_text.clone());
                if !self.checkpoints.get(&path_text).is_some_and(|checkpoint| {
                    checkpoint.file_identity == identity && checkpoint.byte_offset == metadata.len()
                }) {
                    changed.push(path);
                }
            }
            self.remove_missing_paths(&seen);
            self.save_state(database)?;
            Ok(changed)
        })();
        if result.is_err() {
            *self = previous;
        }
        result
    }

    pub fn scan_paths(
        &mut self,
        paths: impl IntoIterator<Item = PathBuf>,
        database: &Database,
    ) -> AppResult<ScanBatch> {
        let previous = self.clone();
        let result = (|| {
            let mut batch = ScanBatch::default();
            let mut unique = HashSet::new();
            for path in paths {
                if !is_transcript(&path) || !unique.insert(path.clone()) {
                    continue;
                }
                let Some(scan) = self.scan_file(&path)? else {
                    continue;
                };
                batch.events.extend(scan.events);
                if scan.has_more {
                    batch.continuations.push(path);
                }
            }
            let state = self.snapshot_state()?;
            batch.events = database.record_events_and_watcher_state(&batch.events, &state)?;
            Ok(batch)
        })();
        if result.is_err() {
            *self = previous;
        }
        result
    }

    fn scan_file(&mut self, path: &Path) -> AppResult<Option<ScanFileResult>> {
        let metadata = match path.metadata() {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_file() {
            return Ok(None);
        }
        let identity = file_identity(path, &metadata)?;
        let path_text = path.to_string_lossy().into_owned();
        self.relocate_checkpoint(&path_text, &identity);
        let session_id = session_id(path)?;
        let offset = match self.checkpoints.get(&path_text) {
            Some(checkpoint)
                if checkpoint.file_identity == identity && metadata.len() >= checkpoint.byte_offset =>
            {
                checkpoint.byte_offset
            }
            _ => {
                self.clear_file_state(path, &identity);
                0
            }
        };
        let context_needs_seed = self
            .contexts
            .get(&identity)
            .map_or(true, |context| context.cwd.is_none() || context.turn_id.is_none());
        if offset > 0 && context_needs_seed {
            self.seed_context(path, &identity, offset)?;
        }
        if offset == metadata.len() {
            self.checkpoints.insert(
                path_text,
                Checkpoint {
                    file_identity: identity,
                    byte_offset: offset,
                },
            );
            return Ok(Some(ScanFileResult::default()));
        }

        let stored_partial_progress = self
            .partial_line_progress
            .get(&identity)
            .filter(|progress| progress.checkpoint_offset == offset)
            .cloned();
        let partial_progress = stored_partial_progress
            .as_ref()
            .filter(|progress| {
                offset
                    .checked_add(progress.discarded_bytes)
                    .is_some_and(|value| value <= metadata.len())
            })
            .cloned();
        if stored_partial_progress.is_some() && partial_progress.is_none() {
            self.partial_line_progress.remove(&identity);
        }
        let scan_offset = partial_progress
            .as_ref()
            .and_then(|progress| offset.checked_add(progress.discarded_bytes))
            .unwrap_or(offset);

        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(scan_offset))?;
        let mut next_offset = scan_offset;
        let mut bytes_read = 0_u64;
        let mut lines_read = 0_usize;
        let mut events = Vec::new();
        let mut stopped_for_budget = false;
        let mut line = Vec::new();
        let resumes_partial_line = partial_progress.is_some() || (offset > 0 && scan_offset == offset);
        if resumes_partial_line {
            let starts_inside_line = if partial_progress.is_some() {
                true
            } else {
                let mut previous = File::open(path)?;
                previous.seek(SeekFrom::Start(offset - 1))?;
                let mut byte = [0_u8; 1];
                previous.read_exact(&mut byte)?;
                byte[0] != b'\n'
            };
            if starts_inside_line {
                let partial = discard_bounded_line(&mut reader, MAX_SCAN_BYTES)?;
                if !partial.complete {
                    if partial.bytes_consumed > 0 {
                        self.partial_line_progress.insert(
                            identity.clone(),
                            PartialLineProgress {
                                checkpoint_offset: offset,
                                discarded_bytes: scan_offset
                                    .saturating_sub(offset)
                                    .saturating_add(partial.bytes_consumed),
                            },
                        );
                    }
                    self.checkpoints.insert(
                        path_text.clone(),
                        Checkpoint {
                            file_identity: identity.clone(),
                            byte_offset: offset,
                        },
                    );
                    return Ok(Some(ScanFileResult {
                        events: Vec::new(),
                        has_more: partial.limit_reached,
                    }));
                }
                self.partial_line_progress.remove(&identity);
                next_offset += partial.bytes_consumed;
                bytes_read += partial.bytes_consumed;
            }
        }
        loop {
            if lines_read >= MAX_SCAN_LINES || bytes_read >= MAX_SCAN_BYTES {
                stopped_for_budget = true;
                break;
            }
            let line_offset = next_offset;
            let remaining_bytes = MAX_SCAN_BYTES.saturating_sub(bytes_read);
            let record = read_bounded_line(&mut reader, &mut line, MAX_RETAINED_LINE_BYTES, remaining_bytes)?;
            if record.bytes_consumed == 0 {
                break;
            }
            if !record.complete {
                if record.limit_reached {
                    self.partial_line_progress.insert(
                        identity.clone(),
                        PartialLineProgress {
                            checkpoint_offset: line_offset,
                            discarded_bytes: record.bytes_consumed,
                        },
                    );
                    self.checkpoints.insert(
                        path_text.clone(),
                        Checkpoint {
                            file_identity: identity.clone(),
                            byte_offset: line_offset,
                        },
                    );
                    return Ok(Some(ScanFileResult {
                        events,
                        has_more: true,
                    }));
                }
                break;
            }
            next_offset += record.bytes_consumed;
            bytes_read += record.bytes_consumed;
            lines_read += 1;
            if record.oversized {
                tracing::warn!(
                    path = %path.display(),
                    bytes = record.bytes_consumed,
                    "skipping oversized transcript record"
                );
                continue;
            }
            let line = std::str::from_utf8(&line)
                .map_err(|_| AppError::IncompatibleFormat("transcript line is not UTF-8".into()))?;
            if let Some(event) = self.parse_line(line, &session_id, &identity, line_offset)? {
                events.push(event);
            }
        }
        self.checkpoints.insert(
            path_text,
            Checkpoint {
                file_identity: identity,
                byte_offset: next_offset,
            },
        );
        Ok(Some(ScanFileResult {
            events,
            has_more: stopped_for_budget && next_offset < metadata.len(),
        }))
    }

    fn seed_context(&mut self, path: &Path, file_identity: &str, end_offset: u64) -> AppResult<()> {
        if end_offset == 0 {
            return Ok(());
        }
        // session_meta is the only unbounded-distance context worth reading. Bound the
        // first line too, so a malformed transcript cannot monopolize startup.
        let mut first_reader = BufReader::new(File::open(path)?);
        let mut first_line = Vec::new();
        first_reader
            .by_ref()
            .take(end_offset.min(MAX_SCAN_BYTES + 1))
            .read_until(b'\n', &mut first_line)?;
        if first_line.ends_with(b"\n") && first_line.len() as u64 <= MAX_SCAN_BYTES {
            let line = std::str::from_utf8(&first_line)
                .map_err(|_| AppError::IncompatibleFormat("transcript line is not UTF-8".into()))?;
            self.seed_context_line(line, file_identity)?;
        }

        let start = end_offset.saturating_sub(MAX_SCAN_BYTES);
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(start))?;
        let mut tail = Vec::with_capacity((end_offset - start) as usize);
        file.take(end_offset - start).read_to_end(&mut tail)?;
        let relative_start = if start == 0 {
            0
        } else {
            tail.iter()
                .position(|byte| *byte == b'\n')
                .map_or(tail.len(), |index| index + 1)
        };
        let complete_end = tail
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(relative_start, |index| index + 1);
        if complete_end <= relative_start {
            return Ok(());
        }

        let complete = &tail[relative_start..complete_end];
        let mut lines = complete
            .split_inclusive(|byte| *byte == b'\n')
            .collect::<Vec<_>>();
        if lines.len() > MAX_SCAN_LINES {
            lines.drain(..lines.len() - MAX_SCAN_LINES);
        }
        for line in lines {
            let text = std::str::from_utf8(line)
                .map_err(|_| AppError::IncompatibleFormat("transcript line is not UTF-8".into()))?;
            self.seed_context_line(text, file_identity)?;
        }
        Ok(())
    }

    fn seed_context_line(&mut self, line: &str, file_identity: &str) -> AppResult<()> {
        let envelope = parse_envelope(line)?;
        match envelope.record_type {
            "session_meta" => {
                let payload: SessionMetaPayload = parse_payload(envelope.payload)?;
                let cwd = required_owned(payload.cwd, "cwd")?;
                self.contexts.entry(file_identity.to_owned()).or_default().cwd = Some(cwd);
            }
            "turn_context" => {
                let payload: TurnContextPayload = parse_payload(envelope.payload)?;
                self.contexts.insert(
                    file_identity.to_owned(),
                    TranscriptContext {
                        cwd: Some(required_owned(payload.cwd, "cwd")?),
                        turn_id: Some(required_owned(payload.turn_id, "turn_id")?),
                    },
                );
            }
            _ => {}
        }
        Ok(())
    }

    fn parse_line(
        &mut self,
        line: &str,
        session_id: &str,
        file_identity: &str,
        offset: u64,
    ) -> AppResult<Option<MonitorEvent>> {
        let envelope = parse_envelope(line)?;
        let occurred_at = DateTime::parse_from_rfc3339(envelope.timestamp)
            .map_err(|_| AppError::IncompatibleFormat(format!("{ADAPTER_VERSION}: invalid timestamp")))?
            .with_timezone(&Utc);
        if envelope.record_type == "session_meta" {
            let payload: SessionMetaPayload = parse_payload(envelope.payload)?;
            required_owned(payload.cwd, "cwd").map(|cwd| {
                self.contexts.entry(file_identity.to_owned()).or_default().cwd = Some(cwd);
            })?;
            return Ok(None);
        }
        if envelope.record_type == "turn_context" {
            let payload: TurnContextPayload = parse_payload(envelope.payload)?;
            self.contexts.insert(
                file_identity.to_owned(),
                TranscriptContext {
                    cwd: Some(required_owned(payload.cwd, "cwd")?),
                    turn_id: Some(required_owned(payload.turn_id, "turn_id")?),
                },
            );
            return Ok(None);
        }
        if !matches!(envelope.record_type, "event_msg" | "response_item") {
            return Ok(None);
        }
        let payload_kind: PayloadKind<'_> = parse_payload(envelope.payload)?;
        if payload_kind.payload_type.is_empty() {
            return Err(AppError::IncompatibleFormat(format!(
                "{ADAPTER_VERSION}: payload.type is required"
            )));
        }

        let fields = match (envelope.record_type, payload_kind.payload_type) {
            ("event_msg", "task_started") => {
                let payload: TurnPayload = parse_payload(envelope.payload)?;
                let turn_id = required_owned(payload.turn_id, "turn_id")?;
                let cwd = context_cwd(&self.contexts, file_identity)?;
                self.contexts.entry(file_identity.to_owned()).or_default().turn_id = Some(turn_id.clone());
                Some((turn_id, cwd, (MonitorKind::Running, "task_started")))
            }
            ("event_msg", "task_complete") => {
                let payload: TurnPayload = parse_payload(envelope.payload)?;
                Some((
                    required_owned(payload.turn_id, "turn_id")?,
                    context_cwd(&self.contexts, file_identity)?,
                    (MonitorKind::Ready, "task_complete"),
                ))
            }
            ("event_msg", "turn_aborted") => {
                let payload: AbortedPayload = parse_payload(envelope.payload)?;
                let (kind, safe_reason) = match payload.reason.as_str() {
                    "interrupted" => (MonitorKind::Stopped, "interrupted"),
                    "failed" => (MonitorKind::Blocked, "explicit_failure"),
                    _ => {
                        return Err(AppError::IncompatibleFormat(format!(
                            "{ADAPTER_VERSION}: unknown turn_aborted reason"
                        )))
                    }
                };
                Some((
                    required_owned(payload.turn_id, "turn_id")?,
                    context_cwd(&self.contexts, file_identity)?,
                    (kind, safe_reason),
                ))
            }
            ("response_item", "function_call") => {
                let payload: FunctionCallPayload = parse_payload(envelope.payload)?;
                if payload.name != "request_user_input" {
                    None
                } else {
                    let context = self.contexts.get(file_identity).ok_or_else(|| {
                        AppError::IncompatibleFormat(format!("{ADAPTER_VERSION}: turn context is required"))
                    })?;
                    let turn_id = context.turn_id.clone().ok_or_else(|| {
                        AppError::IncompatibleFormat(format!(
                            "{ADAPTER_VERSION}: turn_context.turn_id is required"
                        ))
                    })?;
                    let cwd = context.cwd.clone().ok_or_else(|| {
                        AppError::IncompatibleFormat(format!(
                            "{ADAPTER_VERSION}: turn_context.cwd is required"
                        ))
                    })?;
                    let call_id = required_owned(payload.call_id, "call_id")?;
                    let event_id = stable_event_id(file_identity, offset, MonitorKind::NeedsInput, line);
                    self.pending_input_calls.insert(
                        (session_id.to_owned(), call_id),
                        PendingInputCall {
                            file_identity: file_identity.to_owned(),
                            cwd: cwd.clone(),
                            event_id,
                            turn_id: turn_id.clone(),
                        },
                    );
                    Some((turn_id, cwd, (MonitorKind::NeedsInput, "request_user_input")))
                }
            }
            ("response_item", "function_call_output") => {
                let payload: FunctionCallOutputPayload = parse_payload(envelope.payload)?;
                let call_id = required_owned(payload.call_id, "call_id")?;
                self.pending_input_calls
                    .remove(&(session_id.to_owned(), call_id))
                    .map(|pending| {
                        (
                            pending.turn_id,
                            pending.cwd,
                            (MonitorKind::Running, "user_input_received"),
                        )
                    })
            }
            ("event_msg" | "response_item", _) => None,
            _ => unreachable!("record type was validated above"),
        };

        let Some((turn_id, cwd, (kind, reason))) = fields else {
            return Ok(None);
        };
        Ok(Some(MonitorEvent {
            schema_version: MONITOR_SCHEMA_VERSION,
            event_id: stable_event_id(file_identity, offset, kind, line),
            source: MonitorSource::CodexTranscript,
            session_id: session_id.to_owned(),
            turn_id,
            kind,
            occurred_at,
            cwd,
            reason: Some(reason.to_owned()),
        }))
    }

    fn clear_file_state(&mut self, path: &Path, new_identity: &str) {
        let path_text = path.to_string_lossy();
        if let Some(checkpoint) = self.checkpoints.get(path_text.as_ref()) {
            self.partial_line_progress.remove(&checkpoint.file_identity);
            if checkpoint.file_identity != new_identity {
                self.contexts.remove(&checkpoint.file_identity);
                self.pending_input_calls
                    .retain(|_, pending| pending.file_identity != checkpoint.file_identity);
            }
        }
        self.partial_line_progress.remove(new_identity);
        if let Ok(session_id) = session_id(path) {
            self.pending_input_calls
                .retain(|(pending_session_id, _), _| pending_session_id != &session_id);
        }
    }

    fn remove_path_state(&mut self, path: &Path) -> bool {
        let path_text = path.to_string_lossy();
        let Some(checkpoint) = self.checkpoints.remove(path_text.as_ref()) else {
            return false;
        };
        self.partial_line_progress.remove(&checkpoint.file_identity);
        self.contexts.remove(&checkpoint.file_identity);
        self.pending_input_calls
            .retain(|_, pending| pending.file_identity != checkpoint.file_identity);
        true
    }

    fn relocate_checkpoint(&mut self, source_path: &str, file_identity: &str) {
        if self.checkpoints.contains_key(source_path) {
            return;
        }
        let previous_path = self
            .checkpoints
            .iter()
            .find_map(|(path, checkpoint)| (checkpoint.file_identity == file_identity).then(|| path.clone()));
        if let Some(previous_path) = previous_path {
            if let Some(checkpoint) = self.checkpoints.remove(&previous_path) {
                self.checkpoints.insert(source_path.to_owned(), checkpoint);
            }
        }
    }

    fn remove_missing_paths(&mut self, seen: &HashSet<String>) -> bool {
        let missing = self
            .checkpoints
            .keys()
            .filter(|source_path| !seen.contains(*source_path))
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        let changed = !missing.is_empty();
        for path in missing {
            self.remove_path_state(&path);
        }
        changed
    }

    fn snapshot_state(&self) -> AppResult<WatcherState> {
        let checkpoints = self
            .checkpoints
            .iter()
            .map(|(source_path, checkpoint)| {
                let context = self.contexts.get(&checkpoint.file_identity);
                WatcherCheckpoint {
                    source_path: source_path.clone(),
                    file_identity: checkpoint.file_identity.clone(),
                    byte_offset: checkpoint.byte_offset,
                    cwd: context.and_then(|value| value.cwd.clone()),
                    turn_id: context.and_then(|value| value.turn_id.clone()),
                }
            })
            .collect::<Vec<_>>();
        let mut pending_input_calls = Vec::with_capacity(self.pending_input_calls.len());
        for ((session_id, call_id), pending) in &self.pending_input_calls {
            let source_path = self
                .checkpoints
                .iter()
                .find_map(|(source_path, checkpoint)| {
                    (checkpoint.file_identity == pending.file_identity).then(|| source_path.clone())
                })
                .ok_or_else(|| {
                    AppError::InvalidConfig("pending watcher input has no matching file checkpoint".into())
                })?;
            pending_input_calls.push(WatcherPendingInputCall {
                source_path,
                file_identity: pending.file_identity.clone(),
                session_id: session_id.clone(),
                call_id: call_id.clone(),
                cwd: pending.cwd.clone(),
                event_id: pending.event_id.clone(),
                turn_id: pending.turn_id.clone(),
            });
        }
        let mut state = WatcherState {
            checkpoints,
            pending_input_calls,
        };
        state
            .checkpoints
            .sort_unstable_by(|left, right| left.source_path.cmp(&right.source_path));
        state.pending_input_calls.sort_unstable_by(|left, right| {
            (&left.source_path, &left.call_id).cmp(&(&right.source_path, &right.call_id))
        });
        Ok(state)
    }

    fn save_state(&self, database: &Database) -> AppResult<()> {
        database.save_watcher_state(&self.snapshot_state()?)
    }
}

#[derive(Default)]
struct ScanFileResult {
    events: Vec<MonitorEvent>,
    has_more: bool,
}

#[derive(Debug, Default, Eq, PartialEq)]
struct BoundedLine {
    bytes_consumed: u64,
    complete: bool,
    oversized: bool,
    limit_reached: bool,
}

fn read_bounded_line<R: BufRead>(
    reader: &mut R,
    output: &mut Vec<u8>,
    max_retained_bytes: usize,
    max_bytes: u64,
) -> io::Result<BoundedLine> {
    output.clear();
    let mut result = BoundedLine::default();
    loop {
        if result.bytes_consumed >= max_bytes {
            result.limit_reached = true;
            return Ok(result);
        }
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(result);
        }
        let remaining = usize::try_from(max_bytes - result.bytes_consumed).unwrap_or(usize::MAX);
        let available = &available[..available.len().min(remaining)];
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        if !result.oversized {
            if output.len().saturating_add(consumed) <= max_retained_bytes {
                output.extend_from_slice(&available[..consumed]);
            } else {
                result.oversized = true;
                output.clear();
            }
        }
        reader.consume(consumed);
        result.bytes_consumed = result.bytes_consumed.saturating_add(consumed as u64);
        if newline.is_some() {
            result.complete = true;
            return Ok(result);
        }
    }
}

/// Discards an already-incomplete line without retaining any of its payload. A checkpoint is
/// never advanced until the line is terminated, so progress remains in memory only and each
/// pass is capped to keep malformed tails from monopolizing the watcher thread.
fn discard_bounded_line<R: BufRead>(reader: &mut R, max_bytes: u64) -> io::Result<BoundedLine> {
    let mut result = BoundedLine::default();
    loop {
        if result.bytes_consumed >= max_bytes {
            result.limit_reached = true;
            return Ok(result);
        }
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(result);
        }
        let remaining = usize::try_from(max_bytes - result.bytes_consumed).unwrap_or(usize::MAX);
        let available = &available[..available.len().min(remaining)];
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        reader.consume(consumed);
        result.bytes_consumed = result.bytes_consumed.saturating_add(consumed as u64);
        if newline.is_some() {
            result.complete = true;
            return Ok(result);
        }
    }
}

fn parse_envelope(line: &str) -> AppResult<TranscriptEnvelope<'_>> {
    serde_json::from_str(line.trim_end())
        .map_err(|error| AppError::IncompatibleFormat(format!("{ADAPTER_VERSION}: {error}")))
}

fn parse_payload<'a, T: Deserialize<'a>>(payload: &'a RawValue) -> AppResult<T> {
    serde_json::from_str(payload.get())
        .map_err(|error| AppError::IncompatibleFormat(format!("{ADAPTER_VERSION}: {error}")))
}

fn required_owned(value: String, key: &str) -> AppResult<String> {
    if value.is_empty() {
        Err(AppError::IncompatibleFormat(format!(
            "{ADAPTER_VERSION}: payload.{key} is required"
        )))
    } else {
        Ok(value)
    }
}

fn context_cwd(contexts: &HashMap<String, TranscriptContext>, file_identity: &str) -> AppResult<String> {
    contexts
        .get(file_identity)
        .and_then(|context| context.cwd.clone())
        .ok_or_else(|| {
            AppError::IncompatibleFormat(format!("{ADAPTER_VERSION}: session context cwd is required"))
        })
}

fn session_id(path: &Path) -> AppResult<String> {
    path.file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::IncompatibleFormat("transcript filename is not valid UTF-8".into()))
        .map(str::to_owned)
}

fn transcript_paths(sessions: &Path) -> impl Iterator<Item = PathBuf> {
    WalkDir::new(sessions)
        .max_depth(8)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && is_transcript(entry.path()))
        .map(|entry| entry.into_path())
}

pub fn is_transcript(path: &Path) -> bool {
    path.extension().and_then(|value| value.to_str()) == Some("jsonl")
}

fn stable_event_id(file_identity: &str, offset: u64, kind: MonitorKind, line: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(file_identity.as_bytes());
    hasher.update(offset.to_le_bytes());
    hasher.update(kind.as_str().as_bytes());
    hasher.update(line.as_bytes());
    format!("transcript-{:x}", hasher.finalize())
}

fn file_identity(path: &Path, metadata: &std::fs::Metadata) -> AppResult<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = path;
        Ok(format!("{}:{}", metadata.dev(), metadata.ino()))
    }
    #[cfg(not(unix))]
    {
        let canonical = path.canonicalize().unwrap_or_else(|_| PathBuf::from(path));
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
    use std::io::Write as _;

    use super::*;

    #[test]
    fn interruption_is_stopped_not_blocked() {
        let mut engine = WatcherEngine::default();
        let session =
            r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"cwd":"/work"}}"#;
        let line = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"turn_aborted","turn_id":"t","reason":"interrupted"}}"#;
        assert!(engine.parse_line(session, "s", "file", 0).unwrap().is_none());
        let event = engine.parse_line(line, "s", "file", 10).unwrap().unwrap();
        assert_eq!(event.kind, MonitorKind::Stopped);
    }

    #[test]
    fn current_request_input_schema_uses_turn_context() {
        let mut engine = WatcherEngine::default();
        let session =
            r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"cwd":"/work"}}"#;
        let started = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"task_started","turn_id":"t"}}"#;
        let context = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"turn_context","payload":{"turn_id":"t","cwd":"/work"}}"#;
        let call = r#"{"timestamp":"2026-01-01T00:00:03Z","type":"response_item","payload":{"type":"function_call","name":"request_user_input","call_id":"c"}}"#;
        let output = r#"{"timestamp":"2026-01-01T00:00:04Z","type":"response_item","payload":{"type":"function_call_output","call_id":"c"}}"#;
        assert!(engine.parse_line(session, "s", "file", 0).unwrap().is_none());
        assert_eq!(
            engine.parse_line(started, "s", "file", 1).unwrap().unwrap().kind,
            MonitorKind::Running
        );
        assert!(engine.parse_line(context, "s", "file", 2).unwrap().is_none());
        assert_eq!(
            engine.parse_line(call, "s", "file", 3).unwrap().unwrap().kind,
            MonitorKind::NeedsInput
        );
        assert_eq!(
            engine.parse_line(output, "s", "file", 4).unwrap().unwrap().kind,
            MonitorKind::Running
        );
        assert!(engine.parse_line(output, "s", "file", 5).unwrap().is_none());
    }

    #[test]
    fn unrelated_top_level_records_are_ignored_without_materializing_payload() {
        let mut engine = WatcherEngine::default();
        let world_state =
            r#"{"timestamp":"2026-01-01T00:00:00Z","type":"world_state","payload":{"full":true,"state":{}}}"#;
        let compacted =
            r#"{"timestamp":"2026-01-01T00:00:01Z","type":"compacted","payload":{"window_id":"w"}}"#;
        assert!(engine.parse_line(world_state, "s", "file", 0).unwrap().is_none());
        assert!(engine.parse_line(compacted, "s", "file", 1).unwrap().is_none());
    }

    #[test]
    fn incomplete_tail_discard_is_bounded_without_retaining_payload() {
        let mut reader = BufReader::new(std::io::Cursor::new(vec![b'x'; 1_024]));
        let result = discard_bounded_line(&mut reader, 512).unwrap();

        assert_eq!(result.bytes_consumed, 512);
        assert!(!result.complete);
        assert!(result.limit_reached);
    }

    #[test]
    fn existing_file_baselines_at_eof_without_historical_events() {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(dir.path().join("test.db")).unwrap();
        let sessions = dir.path().join("sessions");
        std::fs::create_dir(&sessions).unwrap();
        let transcript = sessions.join("rollout-session.jsonl");
        std::fs::write(
            &transcript,
            concat!(
                "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/work\"}}\n",
                "{\"timestamp\":\"2026-01-01T00:00:01Z\",\"type\":\"turn_context\",\"payload\":{\"turn_id\":\"current\",\"cwd\":\"/work\"}}\n",
                "{\"timestamp\":\"2026-01-01T00:00:02Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"request_user_input\",\"call_id\":\"waiting\"}}\n",
            ),
        )
        .unwrap();

        let mut engine = WatcherEngine::load(&database).unwrap();
        engine.baseline_existing(&sessions, &database).unwrap();
        assert!(engine.reconcile(&sessions, &database).unwrap().is_empty());
        assert!(database.list_tasks().unwrap().is_empty());
        drop(engine);
        let mut engine = WatcherEngine::load(&database).unwrap();

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&transcript)
            .unwrap();
        file.write_all(
            concat!(
                "{\"timestamp\":\"2026-01-01T00:00:03Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"call_id\":\"waiting\"}}\n",
                "{\"timestamp\":\"2026-01-01T00:00:04Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"current\"}}\n",
            )
            .as_bytes(),
        )
        .unwrap();

        let batch = engine.scan_paths([transcript], &database).unwrap();
        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.events[0].kind, MonitorKind::Ready);
    }

    #[test]
    fn metadata_only_baseline_handles_four_hundred_logical_two_gib_files() {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(dir.path().join("test.db")).unwrap();
        let sessions = dir.path().join("sessions");
        std::fs::create_dir(&sessions).unwrap();
        const FILE_SIZE: u64 = 5 * 1024 * 1024;
        for index in 0..400 {
            let file = File::create(sessions.join(format!("session-{index}.jsonl"))).unwrap();
            file.set_len(FILE_SIZE).unwrap();
        }

        let mut engine = WatcherEngine::load(&database).unwrap();
        engine.baseline_existing(&sessions, &database).unwrap();

        assert_eq!(engine.checkpoints.len(), 400);
        assert!(engine
            .checkpoints
            .values()
            .all(|checkpoint| checkpoint.byte_offset == FILE_SIZE));
        assert!(engine.contexts.is_empty());
        assert!(engine.reconcile(&sessions, &database).unwrap().is_empty());
    }

    #[test]
    fn baseline_inside_an_incomplete_historical_line_skips_its_remainder() {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(dir.path().join("test.db")).unwrap();
        let sessions = dir.path().join("sessions");
        std::fs::create_dir(&sessions).unwrap();
        let transcript = sessions.join("session.jsonl");
        std::fs::write(
            &transcript,
            concat!(
                "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/work\"}}\n",
                "{\"timestamp\":\"2026-01-01T00:00:01Z\",\"type\":\"world_state\",\"payload\":{}",
            ),
        )
        .unwrap();
        let mut engine = WatcherEngine::load(&database).unwrap();
        engine.baseline_existing(&sessions, &database).unwrap();

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&transcript)
            .unwrap();
        file.write_all(
            concat!(
                "}\n",
                "{\"timestamp\":\"2026-01-01T00:00:02Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"t\"}}\n",
            )
            .as_bytes(),
        )
        .unwrap();

        let batch = engine.scan_paths([transcript], &database).unwrap();
        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.events[0].kind, MonitorKind::Running);
    }

    #[test]
    fn new_file_is_read_from_zero() {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(dir.path().join("test.db")).unwrap();
        let sessions = dir.path().join("sessions");
        std::fs::create_dir(&sessions).unwrap();
        let mut engine = WatcherEngine::load(&database).unwrap();
        engine.baseline_existing(&sessions, &database).unwrap();

        let transcript = sessions.join("new-session.jsonl");
        std::fs::write(
            &transcript,
            concat!(
                "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/work\"}}\n",
                "{\"timestamp\":\"2026-01-01T00:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"t\"}}\n",
            ),
        )
        .unwrap();
        let batch = engine.scan_paths([transcript], &database).unwrap();
        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.events[0].kind, MonitorKind::Running);
    }

    #[test]
    fn rename_preserves_the_checkpoint_without_replaying_history() {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(dir.path().join("test.db")).unwrap();
        let sessions = dir.path().join("sessions");
        std::fs::create_dir(&sessions).unwrap();
        let original = sessions.join("original.jsonl");
        std::fs::write(
            &original,
            concat!(
                "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/work\"}}\n",
                "{\"timestamp\":\"2026-01-01T00:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"t\"}}\n",
            ),
        )
        .unwrap();
        let mut engine = WatcherEngine::load(&database).unwrap();
        assert_eq!(
            engine
                .scan_paths([original.clone()], &database)
                .unwrap()
                .events
                .len(),
            1
        );

        let renamed = sessions.join("renamed.jsonl");
        std::fs::rename(&original, &renamed).unwrap();
        assert!(engine.reconcile(&sessions, &database).unwrap().is_empty());
        assert!(!engine
            .checkpoints
            .contains_key(original.to_string_lossy().as_ref()));
        assert!(engine
            .checkpoints
            .contains_key(renamed.to_string_lossy().as_ref()));

        let mut file = std::fs::OpenOptions::new().append(true).open(&renamed).unwrap();
        file.write_all(
            "{\"timestamp\":\"2026-01-01T00:00:02Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"t\"}}\n"
                .as_bytes(),
        )
        .unwrap();
        let batch = engine.scan_paths([renamed], &database).unwrap();
        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.events[0].kind, MonitorKind::Ready);
    }

    #[test]
    fn truncate_below_the_checkpoint_restarts_from_zero() {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(dir.path().join("test.db")).unwrap();
        let transcript = dir.path().join("session.jsonl");
        std::fs::write(
            &transcript,
            concat!(
                "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/old\"}}\n",
                "{\"timestamp\":\"2026-01-01T00:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"old\"}}\n",
            ),
        )
        .unwrap();
        let mut engine = WatcherEngine::load(&database).unwrap();
        engine.scan_paths([transcript.clone()], &database).unwrap();

        File::create(&transcript).unwrap();
        engine.scan_paths([transcript.clone()], &database).unwrap();
        std::fs::write(
            &transcript,
            concat!(
                "{\"timestamp\":\"2026-01-01T00:00:02Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/new\"}}\n",
                "{\"timestamp\":\"2026-01-01T00:00:03Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new\"}}\n",
            ),
        )
        .unwrap();
        let batch = engine.scan_paths([transcript], &database).unwrap();
        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.events[0].cwd, "/new");
        assert_eq!(batch.events[0].turn_id, "new");
    }

    #[cfg(unix)]
    #[test]
    fn inode_replacement_discards_the_previous_parser_state() {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(dir.path().join("test.db")).unwrap();
        let transcript = dir.path().join("session.jsonl");
        std::fs::write(
            &transcript,
            concat!(
                "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/old\"}}\n",
                "{\"timestamp\":\"2026-01-01T00:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"old\"}}\n",
            ),
        )
        .unwrap();
        let mut engine = WatcherEngine::load(&database).unwrap();
        engine.scan_paths([transcript.clone()], &database).unwrap();

        let replacement = dir.path().join("replacement.jsonl");
        std::fs::write(
            &replacement,
            concat!(
                "{\"timestamp\":\"2026-01-01T00:00:02Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/new\"}}\n",
                "{\"timestamp\":\"2026-01-01T00:00:03Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new\"}}\n",
            ),
        )
        .unwrap();
        std::fs::rename(replacement, &transcript).unwrap();

        let batch = engine.scan_paths([transcript], &database).unwrap();
        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.events[0].cwd, "/new");
        assert_eq!(batch.events[0].turn_id, "new");
    }

    #[test]
    fn complete_line_budget_returns_a_continuation() {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(dir.path().join("test.db")).unwrap();
        let transcript = dir.path().join("new-session.jsonl");
        let mut body = String::from(
            "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/work\"}}\n",
        );
        for _ in 0..MAX_SCAN_LINES {
            body.push_str(
                "{\"timestamp\":\"2026-01-01T00:00:01Z\",\"type\":\"world_state\",\"payload\":{}}\n",
            );
        }
        std::fs::write(&transcript, body).unwrap();
        let mut engine = WatcherEngine::load(&database).unwrap();
        let first = engine.scan_paths([transcript.clone()], &database).unwrap();
        assert_eq!(first.continuations, vec![transcript.clone()]);
        let second = engine.scan_paths([transcript], &database).unwrap();
        assert!(second.continuations.is_empty());
    }

    #[test]
    fn byte_budget_returns_a_continuation() {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(dir.path().join("test.db")).unwrap();
        let transcript = dir.path().join("large-session.jsonl");
        let padding = "x".repeat(9_000);
        let mut body = String::from(
            "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/work\"}}\n",
        );
        for _ in 0..1_000 {
            body.push_str(&format!(
                "{{\"timestamp\":\"2026-01-01T00:00:01Z\",\"type\":\"world_state\",\"payload\":{{\"padding\":\"{padding}\"}}}}\n"
            ));
        }
        std::fs::write(&transcript, body).unwrap();
        let mut engine = WatcherEngine::load(&database).unwrap();
        let first = engine.scan_paths([transcript.clone()], &database).unwrap();
        assert_eq!(first.continuations, vec![transcript.clone()]);
        let second = engine.scan_paths([transcript], &database).unwrap();
        assert!(second.continuations.is_empty());
    }

    #[test]
    fn watcher_context_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(dir.path().join("test.db")).unwrap();
        let sessions = dir.path().join("sessions");
        std::fs::create_dir(&sessions).unwrap();
        let transcript = sessions.join("session.jsonl");
        std::fs::write(
            &transcript,
            concat!(
                "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/work\"}}\n",
                "{\"timestamp\":\"2026-01-01T00:00:01Z\",\"type\":\"turn_context\",\"payload\":{\"turn_id\":\"t\",\"cwd\":\"/work\"}}\n",
            ),
        )
        .unwrap();
        let mut engine = WatcherEngine::load(&database).unwrap();
        engine.baseline_existing(&sessions, &database).unwrap();
        drop(engine);

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&transcript)
            .unwrap();
        file.write_all(
            "{\"timestamp\":\"2026-01-01T00:00:02Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"t\"}}\n"
                .as_bytes(),
        )
        .unwrap();
        let mut restarted = WatcherEngine::load(&database).unwrap();
        let batch = restarted.scan_paths([transcript], &database).unwrap();
        assert_eq!(batch.events[0].kind, MonitorKind::Ready);
    }

    #[test]
    fn pending_input_survives_restart_with_its_file_identity() {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(dir.path().join("test.db")).unwrap();
        let transcript = dir.path().join("session.jsonl");
        std::fs::write(
            &transcript,
            concat!(
                "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/work\"}}\n",
                "{\"timestamp\":\"2026-01-01T00:00:01Z\",\"type\":\"turn_context\",\"payload\":{\"turn_id\":\"t\",\"cwd\":\"/work\"}}\n",
                "{\"timestamp\":\"2026-01-01T00:00:02Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"request_user_input\",\"call_id\":\"c\"}}\n",
            ),
        )
        .unwrap();
        let mut engine = WatcherEngine::load(&database).unwrap();
        let batch = engine.scan_paths([transcript.clone()], &database).unwrap();
        assert_eq!(batch.events[0].kind, MonitorKind::NeedsInput);
        drop(engine);

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&transcript)
            .unwrap();
        file.write_all(
            "{\"timestamp\":\"2026-01-01T00:00:03Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"call_id\":\"c\"}}\n"
                .as_bytes(),
        )
        .unwrap();
        let mut restarted = WatcherEngine::load(&database).unwrap();
        let batch = restarted.scan_paths([transcript], &database).unwrap();
        assert_eq!(batch.events[0].kind, MonitorKind::Running);
    }

    #[test]
    fn scan_rolls_back_memory_when_sqlite_commit_is_busy() {
        let dir = tempfile::tempdir().unwrap();
        let database_path = dir.path().join("test.db");
        let database = Database::open(&database_path).unwrap();
        let transcript = dir.path().join("session.jsonl");
        std::fs::write(
            &transcript,
            concat!(
                "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/work\"}}\n",
                "{\"timestamp\":\"2026-01-01T00:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"t\"}}\n",
            ),
        )
        .unwrap();
        let mut engine = WatcherEngine::load(&database).unwrap();
        let lock = rusqlite::Connection::open(&database_path).unwrap();
        lock.execute_batch("BEGIN IMMEDIATE").unwrap();

        assert!(engine.scan_paths([transcript.clone()], &database).is_err());
        drop(lock);

        let batch = engine.scan_paths([transcript], &database).unwrap();
        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.events[0].kind, MonitorKind::Running);
        assert_eq!(database.list_tasks().unwrap().len(), 1);
    }

    #[test]
    fn reconcile_rolls_back_memory_when_sqlite_commit_is_busy() {
        let dir = tempfile::tempdir().unwrap();
        let database_path = dir.path().join("test.db");
        let database = Database::open(&database_path).unwrap();
        let sessions = dir.path().join("sessions");
        std::fs::create_dir(&sessions).unwrap();
        let transcript = sessions.join("session.jsonl");
        std::fs::write(
            &transcript,
            "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/work\"}}\n",
        )
        .unwrap();
        let mut engine = WatcherEngine::load(&database).unwrap();
        engine.scan_paths([transcript.clone()], &database).unwrap();
        std::fs::remove_file(&transcript).unwrap();
        let path_text = transcript.to_string_lossy().into_owned();
        let lock = rusqlite::Connection::open(&database_path).unwrap();
        lock.execute_batch("BEGIN IMMEDIATE").unwrap();

        assert!(engine.reconcile(&sessions, &database).is_err());
        assert!(engine.checkpoints.contains_key(&path_text));
        drop(lock);

        assert!(engine.reconcile(&sessions, &database).unwrap().is_empty());
        assert!(!engine.checkpoints.contains_key(&path_text));
        assert_eq!(WatcherEngine::load(&database).unwrap().checkpoints.len(), 0);
    }

    #[test]
    fn oversized_unrelated_record_is_streamed_without_disabling_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(dir.path().join("test.db")).unwrap();
        let transcript = dir.path().join("large-record.jsonl");
        let mut body = Vec::with_capacity(MAX_SCAN_BYTES as usize + 512);
        body.extend_from_slice(
            b"{\"timestamp\":\"2026-01-01T00:00:00Z\",\"type\":\"world_state\",\"payload\":{\"padding\":\"",
        );
        body.resize(MAX_SCAN_BYTES as usize + 128, b'x');
        body.extend_from_slice(
            concat!(
                "\"}}\n",
                "{\"timestamp\":\"2026-01-01T00:00:01Z\",\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/work\"}}\n",
                "{\"timestamp\":\"2026-01-01T00:00:02Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"t\"}}\n",
            )
            .as_bytes(),
        );
        std::fs::write(&transcript, body).unwrap();

        let mut engine = WatcherEngine::load(&database).unwrap();
        let first = engine.scan_paths([transcript.clone()], &database).unwrap();
        assert!(first.events.is_empty());
        assert_eq!(first.continuations, vec![transcript.clone()]);
        let second = engine.scan_paths([transcript], &database).unwrap();
        assert_eq!(second.events[0].kind, MonitorKind::Running);
    }

    #[test]
    fn old_key_is_not_accepted_as_alias() {
        let mut engine = WatcherEngine::default();
        let session =
            r#"{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{"cwd":"/work"}}"#;
        let line = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"task_started","turnId":"t"}}"#;
        assert!(engine.parse_line(session, "s", "file", 0).unwrap().is_none());
        assert!(matches!(
            engine.parse_line(line, "s", "file", 1),
            Err(AppError::IncompatibleFormat(_))
        ));
    }
}
