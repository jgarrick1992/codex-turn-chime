use std::{fs, io::Write, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    error::{AppError, AppResult},
    shortcuts,
};

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
    #[serde(default = "default_reminder_interval_seconds")]
    pub reminder_interval_seconds: u16,
    #[serde(default = "default_dismiss_reminder_shortcut")]
    pub dismiss_reminder_shortcut: Option<String>,
    pub launch_at_login: bool,
    pub transcript_watcher_enabled: bool,
    pub history_retention_days: u16,
    pub onboarding_complete: bool,
    pub needs_input_sound: SoundSetting,
    pub ready_sound: SoundSetting,
}

const fn default_reminder_interval_seconds() -> u16 {
    5
}

fn default_dismiss_reminder_shortcut() -> Option<String> {
    Some(shortcuts::DEFAULT_DISMISS_REMINDER_SHORTCUT.into())
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "en".into(),
            muted: false,
            reminder_interval_seconds: default_reminder_interval_seconds(),
            dismiss_reminder_shortcut: default_dismiss_reminder_shortcut(),
            launch_at_login: false,
            transcript_watcher_enabled: false,
            history_retention_days: 30,
            onboarding_complete: false,
            needs_input_sound: SoundSetting {
                enabled: true,
                path: Some("builtin:voice:lumi".into()),
                volume: 0.70,
            },
            ready_sound: SoundSetting {
                enabled: true,
                path: Some("builtin:voice:lumi".into()),
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
            return Err(AppError::InvalidConfig(
                "history_retention_days must be 30 in v0.1".into(),
            ));
        }
        if !(1..=60).contains(&self.reminder_interval_seconds) {
            return Err(AppError::InvalidConfig(
                "reminder_interval_seconds must be between 1 and 60".into(),
            ));
        }
        shortcuts::validate(self.dismiss_reminder_shortcut.as_deref())?;
        for (name, sound) in [
            ("needs_input_sound", &self.needs_input_sound),
            ("ready_sound", &self.ready_sound),
        ] {
            if !(0.0..=2.0).contains(&sound.volume) {
                return Err(AppError::InvalidConfig(format!(
                    "{name}.volume must be between 0 and 2"
                )));
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
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Path("settings parent".into()))?;
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

    #[test]
    fn defaults_reminder_interval_when_the_key_is_absent() {
        let value = r#"{"language":"en","muted":false,"launch_at_login":false,"transcript_watcher_enabled":false,"history_retention_days":30,"onboarding_complete":false,"needs_input_sound":{"enabled":true,"path":null,"volume":0.7},"ready_sound":{"enabled":true,"path":null,"volume":0.5}}"#;
        let settings = serde_json::from_str::<AppSettings>(value).expect("settings should load");
        assert_eq!(settings.reminder_interval_seconds, 5);
        assert_eq!(
            settings.dismiss_reminder_shortcut.as_deref(),
            Some(shortcuts::DEFAULT_DISMISS_REMINDER_SHORTCUT)
        );
    }

    #[test]
    fn allows_disabling_the_dismiss_reminder_shortcut() {
        let settings = AppSettings {
            dismiss_reminder_shortcut: None,
            ..AppSettings::default()
        };
        assert!(settings.validate().is_ok());

        let serialized = serde_json::to_string(&settings).expect("settings should serialize");
        let loaded = serde_json::from_str::<AppSettings>(&serialized).expect("settings should load");
        assert!(loaded.dismiss_reminder_shortcut.is_none());
    }

    #[test]
    fn rejects_an_invalid_dismiss_reminder_shortcut() {
        let settings = AppSettings {
            dismiss_reminder_shortcut: Some("K".into()),
            ..AppSettings::default()
        };
        assert!(settings.validate().is_err());
    }

    #[test]
    fn defaults_to_the_lumi_voice_scheme() {
        let settings = AppSettings::default();
        assert_eq!(
            settings.needs_input_sound.path.as_deref(),
            Some("builtin:voice:lumi")
        );
        assert_eq!(settings.ready_sound.path.as_deref(), Some("builtin:voice:lumi"));
    }

    #[test]
    fn rejects_reminder_interval_outside_the_supported_range() {
        let mut settings = AppSettings {
            reminder_interval_seconds: 0,
            ..AppSettings::default()
        };
        assert!(settings.validate().is_err());
        settings.reminder_interval_seconds = 61;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn accepts_up_to_200_percent_volume() {
        let mut settings = AppSettings::default();
        settings.needs_input_sound.volume = 2.0;
        settings.ready_sound.volume = 2.0;
        assert!(settings.validate().is_ok());
        settings.ready_sound.volume = 2.01;
        assert!(settings.validate().is_err());
    }
}
