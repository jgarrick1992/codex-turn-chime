use std::{
    str::FromStr,
    sync::{Arc, Mutex},
};

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::error::{AppError, AppResult};

pub const DEFAULT_DISMISS_REMINDER_SHORTCUT: &str = "CommandOrControl+Shift+K";

#[derive(Clone, Debug, Default, Serialize)]
pub struct ShortcutRegistrationStatus {
    pub active: bool,
    pub shortcut: Option<String>,
    pub message: Option<String>,
}

pub type SharedShortcutStatus = Arc<Mutex<ShortcutRegistrationStatus>>;

pub fn new_status() -> SharedShortcutStatus {
    Arc::new(Mutex::new(ShortcutRegistrationStatus::default()))
}

pub fn validate(shortcut: Option<&str>) -> AppResult<()> {
    let Some(value) = shortcut else {
        return Ok(());
    };
    if value.trim().is_empty() || value != value.trim() {
        return Err(AppError::InvalidConfig(
            "dismiss_reminder_shortcut must be null or a non-empty accelerator".into(),
        ));
    }
    let parsed = Shortcut::from_str(value)
        .map_err(|error| AppError::InvalidConfig(format!("invalid dismiss reminder shortcut: {error}")))?;
    if parsed.mods.is_empty() {
        return Err(AppError::InvalidConfig(
            "dismiss_reminder_shortcut must contain a modifier and a non-modifier key".into(),
        ));
    }
    Ok(())
}

fn replace_status(status: &SharedShortcutStatus, next: ShortcutRegistrationStatus) -> AppResult<()> {
    *status
        .lock()
        .map_err(|_| AppError::InvalidConfig("shortcut status lock is poisoned".into()))? = next;
    Ok(())
}

pub fn register_initial(app: &AppHandle, status: &SharedShortcutStatus, configured: Option<&str>) {
    register_initial_with(status, configured, |value| {
        app.global_shortcut()
            .register(value)
            .map_err(|error| error.to_string())
    });
}

fn register_initial_with(
    status: &SharedShortcutStatus,
    configured: Option<&str>,
    mut register: impl FnMut(&str) -> Result<(), String>,
) {
    let next = match configured {
        None => ShortcutRegistrationStatus {
            active: false,
            shortcut: None,
            message: None,
        },
        Some(value) => match register(value) {
            Ok(()) => ShortcutRegistrationStatus {
                active: true,
                shortcut: Some(value.to_owned()),
                message: None,
            },
            Err(error) => {
                tracing::warn!(shortcut = value, error = %error, "global shortcut is not active");
                ShortcutRegistrationStatus {
                    active: false,
                    shortcut: Some(value.to_owned()),
                    message: Some(error),
                }
            }
        },
    };
    if let Err(error) = replace_status(status, next) {
        tracing::warn!(%error, "failed to record global shortcut status");
    }
}

pub fn rebind(
    app: &AppHandle,
    status: &SharedShortcutStatus,
    old: Option<&str>,
    new: Option<&str>,
) -> AppResult<()> {
    rebind_with(
        status,
        old,
        new,
        |value| {
            app.global_shortcut()
                .register(value)
                .map_err(|error| error.to_string())
        },
        |value| {
            app.global_shortcut()
                .unregister(value)
                .map_err(|error| error.to_string())
        },
    )
}

fn rebind_with(
    status: &SharedShortcutStatus,
    old: Option<&str>,
    new: Option<&str>,
    mut register: impl FnMut(&str) -> Result<(), String>,
    mut unregister: impl FnMut(&str) -> Result<(), String>,
) -> AppResult<()> {
    validate(new)?;
    let current = status
        .lock()
        .map_err(|_| AppError::InvalidConfig("shortcut status lock is poisoned".into()))?
        .clone();
    if old == new {
        return Ok(());
    }
    let old_is_registered = current.active && current.shortcut.as_deref() == old;

    if old_is_registered {
        if let Some(value) = old {
            unregister(value).map_err(|error| {
                AppError::InvalidConfig(format!("failed to unregister the previous shortcut: {error}"))
            })?;
        }
    }

    let registration = new.map_or(Ok(()), &mut register);
    if let Err(error) = registration {
        let rollback_error = if old_is_registered {
            old.and_then(|value| register(value).err())
        } else {
            None
        };
        let rollback_succeeded = old_is_registered && rollback_error.is_none();
        let message = if let Some(rollback_error) = rollback_error {
            format!("{error}; restoring the previous shortcut also failed: {rollback_error}")
        } else {
            error
        };
        replace_status(
            status,
            ShortcutRegistrationStatus {
                active: rollback_succeeded,
                shortcut: old.map(str::to_owned),
                message: (!rollback_succeeded).then_some(message.clone()),
            },
        )?;
        return Err(AppError::InvalidConfig(format!(
            "could not activate dismiss reminder shortcut: {message}"
        )));
    }

    replace_status(
        status,
        ShortcutRegistrationStatus {
            active: new.is_some(),
            shortcut: new.map(str::to_owned),
            message: None,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn accepts_default_and_disabled_shortcuts() {
        assert!(validate(Some(DEFAULT_DISMISS_REMINDER_SHORTCUT)).is_ok());
        assert!(validate(None).is_ok());
    }

    #[test]
    fn rejects_shortcuts_without_a_modifier_or_key() {
        assert!(validate(Some("K")).is_err());
        assert!(validate(Some("Shift")).is_err());
        assert!(validate(Some("")).is_err());
    }

    #[test]
    fn records_a_startup_registration_failure_without_returning_an_error() {
        let status = new_status();
        register_initial_with(&status, Some(DEFAULT_DISMISS_REMINDER_SHORTCUT), |_| {
            Err("already registered".into())
        });
        let value = status.lock().expect("status should be readable").clone();
        assert!(!value.active);
        assert_eq!(value.shortcut.as_deref(), Some(DEFAULT_DISMISS_REMINDER_SHORTCUT));
        assert_eq!(value.message.as_deref(), Some("already registered"));
    }

    #[test]
    fn rebinds_to_a_new_shortcut() {
        let status = Arc::new(Mutex::new(ShortcutRegistrationStatus {
            active: true,
            shortcut: Some(DEFAULT_DISMISS_REMINDER_SHORTCUT.into()),
            message: None,
        }));
        let calls = Rc::new(RefCell::new(Vec::new()));
        let register_calls = calls.clone();
        let unregister_calls = calls.clone();

        rebind_with(
            &status,
            Some(DEFAULT_DISMISS_REMINDER_SHORTCUT),
            Some("CommandOrControl+Alt+J"),
            move |value| {
                register_calls.borrow_mut().push(format!("register:{value}"));
                Ok(())
            },
            move |value| {
                unregister_calls.borrow_mut().push(format!("unregister:{value}"));
                Ok(())
            },
        )
        .expect("rebind should succeed");

        assert_eq!(
            calls.borrow().as_slice(),
            [
                format!("unregister:{DEFAULT_DISMISS_REMINDER_SHORTCUT}"),
                "register:CommandOrControl+Alt+J".into(),
            ]
        );
        let value = status.lock().expect("status should be readable").clone();
        assert!(value.active);
        assert_eq!(value.shortcut.as_deref(), Some("CommandOrControl+Alt+J"));
    }

    #[test]
    fn restores_the_previous_shortcut_when_the_new_one_conflicts() {
        let status = Arc::new(Mutex::new(ShortcutRegistrationStatus {
            active: true,
            shortcut: Some(DEFAULT_DISMISS_REMINDER_SHORTCUT.into()),
            message: None,
        }));
        let calls = Rc::new(RefCell::new(Vec::new()));
        let register_calls = calls.clone();

        let result = rebind_with(
            &status,
            Some(DEFAULT_DISMISS_REMINDER_SHORTCUT),
            Some("CommandOrControl+Alt+J"),
            move |value| {
                register_calls.borrow_mut().push(format!("register:{value}"));
                if value == "CommandOrControl+Alt+J" {
                    Err("already registered".into())
                } else {
                    Ok(())
                }
            },
            |_| Ok(()),
        );

        assert!(result.is_err());
        assert_eq!(
            calls.borrow().as_slice(),
            [
                "register:CommandOrControl+Alt+J".to_owned(),
                format!("register:{DEFAULT_DISMISS_REMINDER_SHORTCUT}"),
            ]
        );
        let value = status.lock().expect("status should be readable").clone();
        assert!(value.active);
        assert_eq!(value.shortcut.as_deref(), Some(DEFAULT_DISMISS_REMINDER_SHORTCUT));
        assert!(value.message.is_none());
    }
}
