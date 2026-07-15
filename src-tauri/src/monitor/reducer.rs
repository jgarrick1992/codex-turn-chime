use chrono::{DateTime, Utc};
use serde::Serialize;

use super::{MonitorEvent, MonitorKind, MonitorSource};

#[derive(Clone, Debug, Serialize)]
pub struct TaskSnapshot {
    pub session_id: String,
    pub turn_id: String,
    pub current_kind: MonitorKind,
    pub last_event_id: String,
    pub last_event_at: DateTime<Utc>,
    pub cwd: String,
    pub source: MonitorSource,
    pub reason: Option<String>,
    pub is_read: bool,
}

pub fn reduce_state(current: Option<&TaskSnapshot>, event: &MonitorEvent) -> TaskSnapshot {
    if let Some(current) = current {
        if event.occurred_at < current.last_event_at {
            return current.clone();
        }
        if event.occurred_at == current.last_event_at && event.event_id <= current.last_event_id {
            return current.clone();
        }
    }
    TaskSnapshot {
        session_id: event.session_id.clone(),
        turn_id: event.turn_id.clone(),
        current_kind: event.kind,
        last_event_id: event.event_id.clone(),
        last_event_at: event.occurred_at,
        cwd: event.cwd.clone(),
        source: event.source,
        reason: event.reason.clone(),
        is_read: !matches!(event.kind, MonitorKind::NeedsInput | MonitorKind::Ready | MonitorKind::Blocked),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::monitor::{MonitorEvent, MONITOR_SCHEMA_VERSION};

    fn event(id: &str, kind: MonitorKind, second: i64) -> MonitorEvent {
        MonitorEvent {
            schema_version: MONITOR_SCHEMA_VERSION,
            event_id: id.into(),
            source: MonitorSource::CodexHook,
            session_id: "session".into(),
            turn_id: "turn".into(),
            kind,
            occurred_at: Utc.timestamp_opt(second, 0).unwrap(),
            cwd: "/work".into(),
            reason: None,
        }
    }

    #[test]
    fn interrupted_turn_stays_stopped() {
        let stopped = event("2", MonitorKind::Stopped, 2);
        let snapshot = reduce_state(None, &stopped);
        assert_eq!(snapshot.current_kind, MonitorKind::Stopped);
    }

    #[test]
    fn late_event_cannot_regress_state() {
        let ready = reduce_state(None, &event("2", MonitorKind::Ready, 20));
        let result = reduce_state(Some(&ready), &event("1", MonitorKind::Running, 10));
        assert_eq!(result.current_kind, MonitorKind::Ready);
        assert_eq!(result.last_event_id, "2");
    }

    #[test]
    fn attention_states_start_unread() {
        for kind in [MonitorKind::NeedsInput, MonitorKind::Ready, MonitorKind::Blocked] {
            assert!(!reduce_state(None, &event("1", kind, 1)).is_read);
        }
    }
}
