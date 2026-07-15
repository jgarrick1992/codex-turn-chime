use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::{
    error::{AppError, AppResult},
    monitor::{reduce_state, MonitorEvent, MonitorKind, MonitorSource, TaskSnapshot, MONITOR_SCHEMA_VERSION},
};

#[derive(Clone, Debug)]
pub struct Database {
    path: PathBuf,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> AppResult<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let database = Self { path };
        database.migrate()?;
        Ok(database)
    }

    fn connect(&self) -> AppResult<Connection> {
        let connection = Connection::open(&self.path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.busy_timeout(std::time::Duration::from_secs(2))?;
        Ok(connection)
    }

    fn migrate(&self) -> AppResult<()> {
        self.connect()?.execute_batch(
            "CREATE TABLE IF NOT EXISTS monitor_events (
                event_id TEXT PRIMARY KEY,
                schema_version INTEGER NOT NULL,
                source TEXT NOT NULL,
                session_id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                occurred_at TEXT NOT NULL,
                cwd TEXT NOT NULL,
                reason TEXT,
                received_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_monitor_events_task_time
                ON monitor_events(session_id, turn_id, occurred_at DESC);
            CREATE TABLE IF NOT EXISTS task_states (
                session_id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                current_kind TEXT NOT NULL,
                last_event_id TEXT NOT NULL,
                last_event_at TEXT NOT NULL,
                cwd TEXT NOT NULL,
                source TEXT NOT NULL,
                reason TEXT,
                is_read INTEGER NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(session_id, turn_id)
            );
            CREATE TABLE IF NOT EXISTS watcher_checkpoints (
                source_path TEXT PRIMARY KEY,
                file_identity TEXT NOT NULL,
                byte_offset INTEGER NOT NULL,
                updated_at TEXT NOT NULL
            );",
        )?;
        Ok(())
    }

    pub fn record_events(&self, events: &[MonitorEvent]) -> AppResult<Vec<MonitorEvent>> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let mut accepted = Vec::new();
        for event in events {
            event
                .validate()
                .map_err(|message| AppError::InvalidConfig(message.into()))?;
            let changed = transaction.execute(
                "INSERT OR IGNORE INTO monitor_events
                 (event_id, schema_version, source, session_id, turn_id, kind, occurred_at, cwd, reason, received_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    event.event_id,
                    event.schema_version,
                    event.source.as_str(),
                    event.session_id,
                    event.turn_id,
                    event.kind.as_str(),
                    event.occurred_at.to_rfc3339(),
                    event.cwd,
                    event.reason,
                    Utc::now().to_rfc3339(),
                ],
            )?;
            if changed == 0 {
                continue;
            }
            let current = load_snapshot(&transaction, &event.session_id, &event.turn_id)?;
            let next = reduce_state(current.as_ref(), event);
            save_snapshot(&transaction, &next)?;
            accepted.push(event.clone());
        }
        transaction.commit()?;
        Ok(accepted)
    }

    pub fn list_tasks(&self) -> AppResult<Vec<TaskSnapshot>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT session_id, turn_id, current_kind, last_event_id, last_event_at, cwd, source, reason, is_read
             FROM task_states
             ORDER BY CASE current_kind
               WHEN 'needs_input' THEN 6 WHEN 'blocked' THEN 5 WHEN 'ready' THEN 4
               WHEN 'running' THEN 3 WHEN 'stopped' THEN 2 ELSE 1 END DESC,
               last_event_at DESC",
        )?;
        let rows = statement.query_map([], snapshot_from_row)?;
        rows.map(|row| row.map_err(AppError::from)).collect()
    }

    pub fn list_events(&self, session_id: &str, turn_id: &str) -> AppResult<Vec<MonitorEvent>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(
            "SELECT event_id, schema_version, source, session_id, turn_id, kind, occurred_at, cwd, reason
             FROM monitor_events WHERE session_id = ?1 AND turn_id = ?2
             ORDER BY occurred_at DESC, event_id DESC LIMIT 250",
        )?;
        let rows = statement.query_map(params![session_id, turn_id], event_from_row)?;
        rows.map(|row| row.map_err(AppError::from)).collect()
    }

    pub fn mark_read(&self, session_id: &str, turn_id: &str) -> AppResult<()> {
        self.connect()?.execute(
            "UPDATE task_states SET is_read = 1, updated_at = ?3 WHERE session_id = ?1 AND turn_id = ?2",
            params![session_id, turn_id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn mark_all_read(&self) -> AppResult<()> {
        self.connect()?.execute(
            "UPDATE task_states SET is_read = 1, updated_at = ?1 WHERE is_read = 0",
            [Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn clear_history(&self) -> AppResult<()> {
        self.connect()?.execute_batch(
            "DELETE FROM task_states; DELETE FROM monitor_events; DELETE FROM watcher_checkpoints;",
        )?;
        Ok(())
    }

    pub fn cleanup_retention(&self, days: u16) -> AppResult<()> {
        let cutoff = Utc::now() - chrono::Duration::days(i64::from(days));
        let connection = self.connect()?;
        connection.execute(
            "DELETE FROM monitor_events WHERE occurred_at < ?1",
            [cutoff.to_rfc3339()],
        )?;
        connection.execute(
            "DELETE FROM task_states WHERE NOT EXISTS (
              SELECT 1 FROM monitor_events e WHERE e.session_id = task_states.session_id AND e.turn_id = task_states.turn_id
            )",
            [],
        )?;
        Ok(())
    }

    pub fn checkpoint(&self, source_path: &str) -> AppResult<Option<(String, u64)>> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT file_identity, byte_offset FROM watcher_checkpoints WHERE source_path = ?1",
                [source_path],
                |row| Ok((row.get(0)?, row.get::<_, i64>(1)? as u64)),
            )
            .optional()
            .map_err(AppError::from)
    }

    pub fn save_checkpoint(&self, source_path: &str, file_identity: &str, byte_offset: u64) -> AppResult<()> {
        self.connect()?.execute(
            "INSERT INTO watcher_checkpoints(source_path, file_identity, byte_offset, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(source_path) DO UPDATE SET file_identity=excluded.file_identity, byte_offset=excluded.byte_offset, updated_at=excluded.updated_at",
            params![source_path, file_identity, byte_offset as i64, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn is_ready(&self) -> bool {
        self.connect()
            .and_then(|connection| {
                connection
                    .query_row("SELECT 1", [], |_| Ok(()))
                    .map_err(AppError::from)
            })
            .is_ok()
    }
}

fn parse_time(value: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
        })
}

fn parse_kind(value: String) -> rusqlite::Result<MonitorKind> {
    MonitorKind::parse_exact(&value)
        .ok_or_else(|| rusqlite::Error::InvalidColumnType(0, value, rusqlite::types::Type::Text))
}

fn parse_source(value: String) -> rusqlite::Result<MonitorSource> {
    MonitorSource::parse_exact(&value)
        .ok_or_else(|| rusqlite::Error::InvalidColumnType(0, value, rusqlite::types::Type::Text))
}

fn snapshot_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskSnapshot> {
    Ok(TaskSnapshot {
        session_id: row.get(0)?,
        turn_id: row.get(1)?,
        current_kind: parse_kind(row.get(2)?)?,
        last_event_id: row.get(3)?,
        last_event_at: parse_time(row.get(4)?)?,
        cwd: row.get(5)?,
        source: parse_source(row.get(6)?)?,
        reason: row.get(7)?,
        is_read: row.get::<_, i64>(8)? != 0,
    })
}

fn event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MonitorEvent> {
    let schema_version = row.get::<_, u8>(1)?;
    if schema_version != MONITOR_SCHEMA_VERSION {
        return Err(rusqlite::Error::IntegralValueOutOfRange(
            1,
            i64::from(schema_version),
        ));
    }
    Ok(MonitorEvent {
        event_id: row.get(0)?,
        schema_version,
        source: parse_source(row.get(2)?)?,
        session_id: row.get(3)?,
        turn_id: row.get(4)?,
        kind: parse_kind(row.get(5)?)?,
        occurred_at: parse_time(row.get(6)?)?,
        cwd: row.get(7)?,
        reason: row.get(8)?,
    })
}

fn load_snapshot(
    transaction: &Transaction<'_>,
    session_id: &str,
    turn_id: &str,
) -> AppResult<Option<TaskSnapshot>> {
    transaction
        .query_row(
            "SELECT session_id, turn_id, current_kind, last_event_id, last_event_at, cwd, source, reason, is_read
             FROM task_states WHERE session_id = ?1 AND turn_id = ?2",
            params![session_id, turn_id],
            snapshot_from_row,
        )
        .optional()
        .map_err(AppError::from)
}

fn save_snapshot(transaction: &Transaction<'_>, snapshot: &TaskSnapshot) -> AppResult<()> {
    transaction.execute(
        "INSERT INTO task_states(session_id, turn_id, current_kind, last_event_id, last_event_at, cwd, source, reason, is_read, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(session_id, turn_id) DO UPDATE SET
           current_kind=excluded.current_kind, last_event_id=excluded.last_event_id, last_event_at=excluded.last_event_at,
           cwd=excluded.cwd, source=excluded.source, reason=excluded.reason, is_read=excluded.is_read, updated_at=excluded.updated_at",
        params![
            snapshot.session_id,
            snapshot.turn_id,
            snapshot.current_kind.as_str(),
            snapshot.last_event_id,
            snapshot.last_event_at.to_rfc3339(),
            snapshot.cwd,
            snapshot.source.as_str(),
            snapshot.reason,
            if snapshot.is_read { 1_i64 } else { 0_i64 },
            Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deduplicates_event_ids() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let event = MonitorEvent::new_hook(
            "s".into(),
            "t".into(),
            MonitorKind::Running,
            "/work".into(),
            "test",
        );
        assert_eq!(db.record_events(std::slice::from_ref(&event)).unwrap().len(), 1);
        assert!(db.record_events(&[event]).unwrap().is_empty());
        assert_eq!(db.list_tasks().unwrap().len(), 1);
    }

    #[test]
    fn marks_all_tasks_read() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let events = [
            MonitorEvent::new_hook(
                "session-1".into(),
                "turn-1".into(),
                MonitorKind::NeedsInput,
                "/work/one".into(),
                "permission_requested",
            ),
            MonitorEvent::new_hook(
                "session-2".into(),
                "turn-2".into(),
                MonitorKind::Ready,
                "/work/two".into(),
                "task_complete",
            ),
        ];
        db.record_events(&events).unwrap();
        assert!(db.list_tasks().unwrap().iter().all(|task| !task.is_read));

        db.mark_all_read().unwrap();

        assert!(db.list_tasks().unwrap().iter().all(|task| task.is_read));
    }
}
