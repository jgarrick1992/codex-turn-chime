use std::path::PathBuf;

use crate::error::{AppError, AppResult};

pub const APP_ID: &str = "io.github.jgarrick1992.codexturnchime";

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub database: PathBuf,
    pub settings: PathBuf,
    pub queue: PathBuf,
    pub logs_dir: PathBuf,
    pub backups_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> AppResult<Self> {
        let root = dirs::data_dir()
            .ok_or_else(|| AppError::Path("operating-system data directory".into()))?
            .join(APP_ID);
        Ok(Self {
            database: root.join("codex-turn-chime.db"),
            settings: root.join("settings.json"),
            queue: root.join("hook-events.jsonl"),
            logs_dir: root.join("logs"),
            backups_dir: root.join("backups"),
            data_dir: root,
        })
    }

    pub fn ensure(&self) -> AppResult<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.logs_dir)?;
        std::fs::create_dir_all(&self.backups_dir)?;
        Ok(())
    }
}

pub fn codex_home() -> AppResult<PathBuf> {
    if let Some(path) = std::env::var_os("CODEX_HOME") {
        return Ok(PathBuf::from(path));
    }
    dirs::home_dir()
        .map(|home| home.join(".codex"))
        .ok_or_else(|| AppError::Path("CODEX_HOME or user home".into()))
}
