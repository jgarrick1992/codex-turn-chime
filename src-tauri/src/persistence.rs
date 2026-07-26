use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::{
    error::{AppError, AppResult},
    monitor::{reduce_state, MonitorEvent, MonitorKind, MonitorSource, TaskSnapshot, MONITOR_SCHEMA_VERSION},
};

const SQLITE_BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(400);

#[derive(Clone, Debug)]
pub struct Database {
    path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WatcherCheckpoint {
    pub(crate) source_path: String,
    pub(crate) file_identity: String,
    pub(crate) byte_offset: u64,
    pub(crate) cwd: Option<String>,
    pub(crate) turn_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WatcherPendingInputCall {
    pub(crate) source_path: String,
    pub(crate) file_identity: String,
    pub(crate) session_id: String,
    pub(crate) call_id: String,
    pub(crate) cwd: String,
    pub(crate) event_id: String,
    pub(crate) turn_id: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct WatcherState {
    pub(crate) checkpoints: Vec<WatcherCheckpoint>,
    pub(crate) pending_input_calls: Vec<WatcherPendingInputCall>,
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
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.busy_timeout(SQLITE_BUSY_TIMEOUT)?;
        Ok(connection)
    }

    fn migrate(&self) -> AppResult<()> {
        let connection = self.connect()?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        if watcher_schema_requires_reset(&connection)? {
            connection.execute_batch(
                "DROP TABLE IF EXISTS watcher_pending_inputs;
                 DROP TABLE IF EXISTS watcher_pending_input_calls;
                 DROP TABLE IF EXISTS watcher_contexts;
                 DROP TABLE IF EXISTS watcher_checkpoints;",
            )?;
        }
        connection.execute_batch(
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
            CREATE INDEX IF NOT EXISTS idx_monitor_events_occurred_at
                ON monitor_events(occurred_at);
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
                cwd TEXT,
                turn_id TEXT,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS watcher_pending_inputs (
                source_path TEXT NOT NULL,
                file_identity TEXT NOT NULL,
                session_id TEXT NOT NULL,
                call_id TEXT NOT NULL,
                cwd TEXT NOT NULL,
                event_id TEXT NOT NULL,
                turn_id TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(source_path, call_id),
                FOREIGN KEY(source_path) REFERENCES watcher_checkpoints(source_path) ON DELETE CASCADE
            );",
        )?;
        Ok(())
    }

    pub fn record_events(&self, events: &[MonitorEvent]) -> AppResult<Vec<MonitorEvent>> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let accepted = record_events_in_transaction(&transaction, events)?;
        transaction.commit()?;
        Ok(accepted)
    }

    pub(crate) fn record_events_and_watcher_state(
        &self,
        events: &[MonitorEvent],
        state: &WatcherState,
    ) -> AppResult<Vec<MonitorEvent>> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        let accepted = record_events_in_transaction(&transaction, events)?;
        replace_watcher_state(&transaction, state)?;
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
            "DELETE FROM task_states;
             DELETE FROM monitor_events;
             DELETE FROM watcher_pending_inputs;
             DELETE FROM watcher_checkpoints;",
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

    pub(crate) fn load_watcher_state(&self) -> AppResult<WatcherState> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;

        let checkpoints = {
            let mut statement = transaction.prepare(
                "SELECT source_path, file_identity, byte_offset, cwd, turn_id
                 FROM watcher_checkpoints ORDER BY source_path",
            )?;
            let rows = statement.query_map([], |row| {
                let byte_offset = row.get::<_, i64>(2)?;
                let byte_offset = u64::try_from(byte_offset)
                    .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(2, byte_offset))?;
                Ok(WatcherCheckpoint {
                    source_path: row.get(0)?,
                    file_identity: row.get(1)?,
                    byte_offset,
                    cwd: row.get(3)?,
                    turn_id: row.get(4)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let pending_input_calls = {
            let mut statement = transaction.prepare(
                "SELECT source_path, file_identity, session_id, call_id, cwd, event_id, turn_id
                 FROM watcher_pending_inputs ORDER BY source_path, call_id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok(WatcherPendingInputCall {
                    source_path: row.get(0)?,
                    file_identity: row.get(1)?,
                    session_id: row.get(2)?,
                    call_id: row.get(3)?,
                    cwd: row.get(4)?,
                    event_id: row.get(5)?,
                    turn_id: row.get(6)?,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

        transaction.commit()?;
        Ok(WatcherState {
            checkpoints,
            pending_input_calls,
        })
    }

    pub(crate) fn save_watcher_state(&self, state: &WatcherState) -> AppResult<()> {
        let mut connection = self.connect()?;
        let transaction = connection.transaction()?;
        replace_watcher_state(&transaction, state)?;
        transaction.commit()?;
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

fn record_events_in_transaction(
    transaction: &Transaction<'_>,
    events: &[MonitorEvent],
) -> AppResult<Vec<MonitorEvent>> {
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
        let current = load_snapshot(transaction, &event.session_id, &event.turn_id)?;
        let next = reduce_state(current.as_ref(), event);
        save_snapshot(transaction, &next)?;
        accepted.push(event.clone());
    }
    Ok(accepted)
}

fn replace_watcher_state(transaction: &Transaction<'_>, state: &WatcherState) -> AppResult<()> {
    let updated_at = Utc::now().to_rfc3339();
    transaction.execute("DELETE FROM watcher_checkpoints", [])?;
    {
        let mut statement = transaction.prepare(
            "INSERT INTO watcher_checkpoints
             (source_path, file_identity, byte_offset, cwd, turn_id, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for checkpoint in &state.checkpoints {
            let byte_offset = i64::try_from(checkpoint.byte_offset).map_err(|_| {
                AppError::InvalidConfig("watcher checkpoint byte_offset exceeds SQLite INTEGER".into())
            })?;
            statement.execute(params![
                checkpoint.source_path,
                checkpoint.file_identity,
                byte_offset,
                checkpoint.cwd,
                checkpoint.turn_id,
                updated_at,
            ])?;
        }
    }
    transaction.execute("DELETE FROM watcher_pending_inputs", [])?;
    {
        let mut statement = transaction.prepare(
            "INSERT INTO watcher_pending_inputs
             (source_path, file_identity, session_id, call_id, cwd, event_id, turn_id, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for pending in &state.pending_input_calls {
            statement.execute(params![
                pending.source_path,
                pending.file_identity,
                pending.session_id,
                pending.call_id,
                pending.cwd,
                pending.event_id,
                pending.turn_id,
                updated_at,
            ])?;
        }
    }
    Ok(())
}

fn watcher_schema_requires_reset(connection: &Connection) -> AppResult<bool> {
    let table_exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'watcher_checkpoints')",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if !table_exists {
        return Ok(false);
    }
    let mut statement = connection.prepare("PRAGMA table_info(watcher_checkpoints)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<std::collections::HashSet<_>>>()?;
    Ok(!columns.contains("cwd") || !columns.contains("turn_id"))
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

    fn watcher_state(label: &str) -> WatcherState {
        WatcherState {
            checkpoints: vec![WatcherCheckpoint {
                source_path: format!("/sessions/{label}.jsonl"),
                file_identity: format!("identity-{label}"),
                byte_offset: 42,
                cwd: Some(format!("/work/{label}")),
                turn_id: Some(format!("turn-{label}")),
            }],
            pending_input_calls: vec![WatcherPendingInputCall {
                source_path: format!("/sessions/{label}.jsonl"),
                file_identity: format!("identity-{label}"),
                session_id: format!("session-{label}"),
                call_id: format!("call-{label}"),
                cwd: format!("/work/{label}"),
                event_id: format!("event-{label}"),
                turn_id: format!("turn-{label}"),
            }],
        }
    }

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

    #[test]
    fn watcher_state_round_trips_and_replaces_the_previous_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        assert_eq!(db.load_watcher_state().unwrap(), WatcherState::default());

        db.save_watcher_state(&watcher_state("first")).unwrap();
        assert_eq!(db.load_watcher_state().unwrap(), watcher_state("first"));

        db.save_watcher_state(&watcher_state("second")).unwrap();
        assert_eq!(db.load_watcher_state().unwrap(), watcher_state("second"));
    }

    #[test]
    fn watcher_state_replacement_rolls_back_as_one_transaction() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let original = watcher_state("original");
        db.save_watcher_state(&original).unwrap();

        let mut invalid = watcher_state("replacement");
        invalid.checkpoints.push(invalid.checkpoints[0].clone());
        assert!(db.save_watcher_state(&invalid).is_err());
        assert_eq!(db.load_watcher_state().unwrap(), original);
    }

    #[test]
    fn events_and_watcher_state_roll_back_together() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let original = watcher_state("original");
        db.save_watcher_state(&original).unwrap();
        let event = MonitorEvent::new_hook(
            "session".into(),
            "turn".into(),
            MonitorKind::Running,
            "/work".into(),
            "test",
        );

        let mut invalid = watcher_state("replacement");
        invalid.pending_input_calls[0].source_path = "/missing.jsonl".into();
        assert!(db.record_events_and_watcher_state(&[event], &invalid).is_err());
        assert_eq!(db.load_watcher_state().unwrap(), original);
        assert!(db.list_tasks().unwrap().is_empty());
    }

    #[test]
    fn migration_creates_occurred_at_index_and_enables_wal() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let connection = db.connect().unwrap();
        let journal_mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        let index_count: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master
                 WHERE type = 'index' AND name = 'idx_monitor_events_occurred_at'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(journal_mode, "wal");
        assert_eq!(index_count, 1);
    }

    #[test]
    fn migration_replaces_legacy_watcher_state_without_mapping_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE watcher_checkpoints (
                    source_path TEXT PRIMARY KEY,
                    file_identity TEXT NOT NULL,
                    byte_offset INTEGER NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 INSERT INTO watcher_checkpoints VALUES ('old', 'identity', 12, 'now');",
            )
            .unwrap();
        drop(connection);

        let database = Database::open(path).unwrap();
        assert_eq!(database.load_watcher_state().unwrap(), WatcherState::default());
        let connection = database.connect().unwrap();
        let columns = connection
            .prepare("PRAGMA table_info(watcher_checkpoints)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert!(columns.iter().any(|column| column == "cwd"));
        assert!(columns.iter().any(|column| column == "turn_id"));
    }

    #[test]
    fn a_write_lock_cannot_block_the_hook_database_path_for_one_second() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("locked.db");
        let database = Database::open(&path).unwrap();
        let lock = Connection::open(path).unwrap();
        lock.execute_batch("BEGIN IMMEDIATE").unwrap();
        let event = MonitorEvent::new_hook(
            "session".into(),
            "turn".into(),
            MonitorKind::Running,
            "/work".into(),
            "test",
        );

        let started = std::time::Instant::now();
        assert!(database.record_events(&[event]).is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }
}
