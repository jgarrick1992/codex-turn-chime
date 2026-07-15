use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MONITOR_SCHEMA_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorSource {
    CodexHook,
    CodexTranscript,
}

impl MonitorSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CodexHook => "codex_hook",
            Self::CodexTranscript => "codex_transcript",
        }
    }

    pub fn parse_exact(value: &str) -> Option<Self> {
        match value {
            "codex_hook" => Some(Self::CodexHook),
            "codex_transcript" => Some(Self::CodexTranscript),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorKind {
    Running,
    NeedsInput,
    Ready,
    Stopped,
    Blocked,
    Unknown,
}

impl MonitorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::NeedsInput => "needs_input",
            Self::Ready => "ready",
            Self::Stopped => "stopped",
            Self::Blocked => "blocked",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse_exact(value: &str) -> Option<Self> {
        match value {
            "running" => Some(Self::Running),
            "needs_input" => Some(Self::NeedsInput),
            "ready" => Some(Self::Ready),
            "stopped" => Some(Self::Stopped),
            "blocked" => Some(Self::Blocked),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MonitorEvent {
    pub schema_version: u8,
    pub event_id: String,
    pub source: MonitorSource,
    pub session_id: String,
    pub turn_id: String,
    pub kind: MonitorKind,
    pub occurred_at: DateTime<Utc>,
    pub cwd: String,
    pub reason: Option<String>,
}

impl MonitorEvent {
    pub fn new_hook(
        session_id: String,
        turn_id: String,
        kind: MonitorKind,
        cwd: String,
        reason: &'static str,
    ) -> Self {
        Self {
            schema_version: MONITOR_SCHEMA_VERSION,
            event_id: Uuid::new_v4().to_string(),
            source: MonitorSource::CodexHook,
            session_id,
            turn_id,
            kind,
            occurred_at: Utc::now(),
            cwd,
            reason: Some(reason.to_owned()),
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != MONITOR_SCHEMA_VERSION {
            return Err("schema_version must be 1");
        }
        if self.event_id.is_empty() || self.session_id.is_empty() || self.turn_id.is_empty() {
            return Err("event_id, session_id, and turn_id are required");
        }
        if self.cwd.is_empty() {
            return Err("cwd is required");
        }
        Ok(())
    }
}
