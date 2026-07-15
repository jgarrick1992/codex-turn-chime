use std::{fs, io::Write, path::Path};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SoundSetting {
    pub enabled: bool,
    pub path: Option<String>,
    pub volume: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppSettings {
    pub language: String,
    pub muted: bool,
    pub launch_at_login: bool,
    pub transcript_watcher_enabled: bool,
    pub history_retention_days: u16,
    pub onboarding_complete: bool,
    pub needs_input_sound: SoundSetting,
    pub ready_sound: SoundSetting,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "en".into(),
            muted: false,
            launch_at_login: false,
            transcript_watcher_enabled: false,
            history_retention_days: 30,
            onboarding_complete: false,
            needs_input_sound: SoundSetting {
                enabled: true,
                path: None,
                volume: 0.70,
            },
            ready_sound: SoundSetting {
                enabled: true,
                path: None,
                volume: 0.58,
            },
        }
    }
}

impl AppSettings {
    pub fn validate(&self) -> AppResult<()> {
        if !matches!(self.language.as_str(), "en" | "zh-CN") {
            return Err(AppError::InvalidConfig("language must be en or zh-CN".into()));
        }
        if self.history_retention_days != 30 {
            return Err(AppError::InvalidConfig("history_retention_days must be 30 in v0.1".into()));
        }
        for (name, sound) in [
            ("needs_input_sound", &self.needs_input_sound),
            ("ready_sound", &self.ready_sound),
        ] {
            if !(0.0..=1.0).contains(&sound.volume) {
                return Err(AppError::InvalidConfig(format!("{name}.volume must be between 0 and 1")));
            }
        }
        Ok(())
    }
}

pub fn load(path: &Path) -> AppResult<AppSettings> {
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let value: AppSettings = serde_json::from_slice(&fs::read(path)?)?;
    value.validate()?;
    Ok(value)
}

pub fn save(path: &Path, settings: &AppSettings) -> AppResult<()> {
    settings.validate()?;
    let parent = path.parent().ok_or_else(|| AppError::Path("settings parent".into()))?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&serde_json::to_vec_pretty(settings)?)?;
    temporary.write_all(b"\n")?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_setting_key() {
        let value = r#"{"language":"en","muted":false,"launch_at_login":false,"transcript_watcher_enabled":false,"history_retention_days":30,"onboarding_complete":false,"needs_input_sound":{"enabled":true,"path":null,"volume":0.7},"ready_sound":{"enabled":true,"path":null,"volume":0.5},"legacy_key":true}"#;
        assert!(serde_json::from_str::<AppSettings>(value).is_err());
    }
}
