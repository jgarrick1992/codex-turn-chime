use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

use fs2::FileExt;

use crate::{error::AppResult, monitor::MonitorEvent};

pub fn append(path: &Path, event: &MonitorEvent) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(path)?;
    file.lock_exclusive()?;
    let result = (|| -> AppResult<()> {
        serde_json::to_writer(&mut file, event)?;
        file.write_all(b"\n")?;
        file.flush()?;
        file.sync_data()?;
        Ok(())
    })();
    let _ = FileExt::unlock(&file);
    result
}

#[derive(Debug, Default)]
pub struct DrainResult {
    pub events: Vec<MonitorEvent>,
    pub rejected_lines: usize,
}

pub fn drain(path: &Path) -> AppResult<DrainResult> {
    if !path.exists() {
        return Ok(DrainResult::default());
    }
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    file.lock_exclusive()?;
    let result = drain_locked(&mut file);
    let _ = FileExt::unlock(&file);
    result
}

fn drain_locked(file: &mut File) -> AppResult<DrainResult> {
    file.seek(SeekFrom::Start(0))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let mut result = DrainResult::default();
    let mut partial = "";
    for segment in contents.split_inclusive('\n') {
        if !segment.ends_with('\n') {
            partial = segment;
            continue;
        }
        let line = segment.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<MonitorEvent>(line) {
            Ok(event) if event.validate().is_ok() => result.events.push(event),
            _ => result.rejected_lines += 1,
        }
    }
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    if !partial.is_empty() {
        file.write_all(partial.as_bytes())?;
    }
    file.flush()?;
    file.sync_data()?;
    Ok(result)
}

pub fn is_readable(path: &Path) -> bool {
    !path.exists()
        || OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .is_ok()
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use crate::monitor::{MonitorEvent, MonitorKind};

    use super::*;

    #[test]
    fn preserves_partial_jsonl_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("queue.jsonl");
        let event = MonitorEvent::new_hook(
            "s".into(),
            "t".into(),
            MonitorKind::Running,
            "/work".into(),
            "test",
        );
        append(&path, &event).unwrap();
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"schema_version\":").unwrap();
        let drained = drain(&path).unwrap();
        assert_eq!(drained.events, vec![event]);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "{\"schema_version\":");
    }

    #[test]
    fn rejects_unknown_monitor_event_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("queue.jsonl");
        std::fs::write(&path, "{\"schema_version\":1,\"legacy\":true}\n").unwrap();
        let drained = drain(&path).unwrap();
        assert_eq!(drained.rejected_lines, 1);
        assert!(drained.events.is_empty());
    }
}
