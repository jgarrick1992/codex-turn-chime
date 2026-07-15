use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Utc;
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::{
    error::{AppError, AppResult},
    paths,
};

const HANDLER_STATUS: &str = "CodexTurnChime: recording task state";
const EVENTS: [&str; 3] = ["UserPromptSubmit", "PermissionRequest", "Stop"];

#[derive(Clone, Debug, Serialize)]
pub struct HookPreview {
    pub config_path: String,
    pub backup_path: Option<String>,
    pub before_json: String,
    pub after_json: String,
    pub diff: String,
    pub already_installed: bool,
}

fn config_path() -> AppResult<PathBuf> {
    Ok(paths::codex_home()?.join("hooks.json"))
}

pub fn hook_config_path() -> AppResult<PathBuf> {
    config_path()
}

pub fn helper_path() -> AppResult<PathBuf> {
    let current = std::env::current_exe()?;
    let name = if cfg!(windows) {
        "codex-turn-chime-hook.exe"
    } else {
        "codex-turn-chime-hook"
    };
    Ok(current.with_file_name(name))
}

#[cfg(not(target_os = "windows"))]
fn shell_quote(path: &Path) -> String {
    let raw = path.to_string_lossy();
    format!("'{}'", raw.replace('\'', "'\\''"))
}

#[cfg(target_os = "windows")]
fn windows_quote(path: &Path) -> String {
    format!("\"{}\"", path.to_string_lossy().replace('"', "\\\""))
}

fn commands(helper: &Path) -> (String, String) {
    #[cfg(target_os = "windows")]
    {
        let exact = windows_quote(helper);
        (exact.clone(), exact)
    }
    #[cfg(not(target_os = "windows"))]
    {
        (
            shell_quote(helper),
            "\"%LOCALAPPDATA%\\CodexTurnChime\\codex-turn-chime-hook.exe\"".into(),
        )
    }
}

fn load_config(path: &Path) -> AppResult<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }
    let value: Value = serde_json::from_slice(&fs::read(path)?)?;
    if !value.is_object() {
        return Err(AppError::InvalidConfig(
            "Codex hooks root must be a JSON object".into(),
        ));
    }
    validate_hooks_shape(&value)?;
    Ok(value)
}

fn validate_hooks_shape(root: &Value) -> AppResult<()> {
    let Some(hooks_value) = root.get("hooks") else {
        return Ok(());
    };
    let hooks = hooks_value
        .as_object()
        .ok_or_else(|| AppError::InvalidConfig("hooks must be a JSON object".into()))?;
    for event in EVENTS {
        let Some(groups_value) = hooks.get(event) else {
            continue;
        };
        let groups = groups_value
            .as_array()
            .ok_or_else(|| AppError::InvalidConfig(format!("hooks.{event} must be a JSON array")))?;
        for group in groups {
            let group = group.as_object().ok_or_else(|| {
                AppError::InvalidConfig(format!("hooks.{event} entries must be JSON objects"))
            })?;
            let handlers = group.get("hooks").and_then(Value::as_array).ok_or_else(|| {
                AppError::InvalidConfig(format!("hooks.{event}[].hooks must be a JSON array"))
            })?;
            if handlers.iter().any(|handler| !handler.is_object()) {
                return Err(AppError::InvalidConfig(format!(
                    "hooks.{event}[].hooks entries must be JSON objects"
                )));
            }
        }
    }
    Ok(())
}

fn is_our_handler(value: &Value) -> bool {
    value.get("type").and_then(Value::as_str) == Some("command")
        && value.get("statusMessage").and_then(Value::as_str) == Some(HANDLER_STATUS)
        && value.get("command").and_then(Value::as_str).is_some()
}

fn event_groups_mut<'a>(root: &'a mut Value, event: &str) -> AppResult<&'a mut Vec<Value>> {
    let root = root
        .as_object_mut()
        .ok_or_else(|| AppError::InvalidConfig("Codex hooks root must be a JSON object".into()))?;
    let hooks = root.entry("hooks").or_insert_with(|| Value::Object(Map::new()));
    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| AppError::InvalidConfig("hooks must be a JSON object".into()))?;
    let groups = hooks.entry(event).or_insert_with(|| Value::Array(Vec::new()));
    groups
        .as_array_mut()
        .ok_or_else(|| AppError::InvalidConfig(format!("hooks.{event} must be a JSON array")))
}

fn contains_handler(root: &Value, event: &str) -> bool {
    root.get("hooks")
        .and_then(Value::as_object)
        .and_then(|hooks| hooks.get(event))
        .and_then(Value::as_array)
        .is_some_and(|groups| {
            groups.iter().any(|group| {
                group
                    .get("hooks")
                    .and_then(Value::as_array)
                    .is_some_and(|handlers| handlers.iter().any(is_our_handler))
            })
        })
}

fn all_handlers_installed(root: &Value) -> bool {
    EVENTS.iter().all(|event| contains_handler(root, event))
}

fn add_handlers(mut root: Value, helper: &Path) -> AppResult<Value> {
    let (command, command_windows) = commands(helper);
    for event in EVENTS {
        if contains_handler(&root, event) {
            continue;
        }
        event_groups_mut(&mut root, event)?.push(json!({
            "hooks": [{
                "type": "command",
                "command": command.clone(),
                "commandWindows": command_windows.clone(),
                "timeout": 1,
                "statusMessage": HANDLER_STATUS
            }]
        }));
    }
    Ok(root)
}

fn remove_handlers(mut root: Value) -> AppResult<Value> {
    for event in EVENTS {
        let root_object = root
            .as_object_mut()
            .ok_or_else(|| AppError::InvalidConfig("Codex settings root must be a JSON object".into()))?;
        let Some(hooks_value) = root_object.get_mut("hooks") else {
            return Ok(root);
        };
        let hooks = hooks_value
            .as_object_mut()
            .ok_or_else(|| AppError::InvalidConfig("hooks must be a JSON object".into()))?;
        let Some(groups_value) = hooks.get_mut(event) else {
            continue;
        };
        let groups = groups_value
            .as_array_mut()
            .ok_or_else(|| AppError::InvalidConfig(format!("hooks.{event} must be a JSON array")))?;
        for group in groups.iter_mut() {
            let object = group.as_object_mut().ok_or_else(|| {
                AppError::InvalidConfig(format!("hooks.{event} entries must be JSON objects"))
            })?;
            if let Some(handlers) = object.get_mut("hooks") {
                let handlers = handlers.as_array_mut().ok_or_else(|| {
                    AppError::InvalidConfig(format!("hooks.{event}[].hooks must be a JSON array"))
                })?;
                handlers.retain(|handler| !is_our_handler(handler));
            }
        }
        groups.retain(|group| {
            group
                .get("hooks")
                .and_then(Value::as_array)
                .map_or(true, |handlers| !handlers.is_empty())
        });
    }
    Ok(root)
}

fn pretty(value: &Value) -> AppResult<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(value)?))
}

fn diff(before: &str, after: &str) -> String {
    if before == after {
        return "No changes.\n".into();
    }
    let removed = before
        .lines()
        .map(|line| format!("- {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let added = after
        .lines()
        .map(|line| format!("+ {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("--- current\n+++ proposed\n{removed}\n{added}\n")
}

fn make_preview(
    path: &Path,
    before: Value,
    after: Value,
    backup_path: Option<PathBuf>,
) -> AppResult<HookPreview> {
    let before_json = pretty(&before)?;
    let after_json = pretty(&after)?;
    Ok(HookPreview {
        config_path: path.to_string_lossy().into_owned(),
        backup_path: backup_path.map(|path| path.to_string_lossy().into_owned()),
        diff: diff(&before_json, &after_json),
        already_installed: all_handlers_installed(&after) && before == after,
        before_json,
        after_json,
    })
}

pub fn preview_install() -> AppResult<HookPreview> {
    let path = config_path()?;
    let before = load_config(&path)?;
    let after = add_handlers(before.clone(), &helper_path()?)?;
    make_preview(&path, before, after, None)
}

fn backup(path: &Path, backup_dir: &Path) -> AppResult<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    fs::create_dir_all(backup_dir)?;
    let backup_path = backup_dir.join(format!(
        "codex-hooks-{}.json",
        Utc::now().format("%Y%m%dT%H%M%S%.6fZ")
    ));
    fs::copy(path, &backup_path)?;
    Ok(Some(backup_path))
}

fn atomic_write(path: &Path, value: &Value) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Path("Codex hooks parent".into()))?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(pretty(value)?.as_bytes())?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    let verified = load_config(path)?;
    if &verified != value {
        return Err(AppError::InvalidConfig(
            "Hook config verification failed after atomic replace".into(),
        ));
    }
    Ok(())
}

pub fn install(backup_dir: &Path) -> AppResult<HookPreview> {
    let path = config_path()?;
    let helper = helper_path()?;
    if !helper.is_file() {
        return Err(AppError::Path(format!(
            "Hook helper not found at {}",
            helper.display()
        )));
    }
    let before = load_config(&path)?;
    let after = add_handlers(before.clone(), &helper)?;
    let backup_path = if before == after {
        None
    } else {
        backup(&path, backup_dir)?
    };
    if before != after {
        atomic_write(&path, &after)?;
    }
    make_preview(&path, before, after, backup_path)
}

pub fn uninstall(backup_dir: &Path) -> AppResult<HookPreview> {
    let path = config_path()?;
    let before = load_config(&path)?;
    let after = remove_handlers(before.clone())?;
    let backup_path = if before == after {
        None
    } else {
        backup(&path, backup_dir)?
    };
    if before != after {
        atomic_write(&path, &after)?;
    }
    make_preview(&path, before, after, backup_path)
}

pub fn is_installed() -> bool {
    config_path()
        .and_then(|path| load_config(&path))
        .is_ok_and(|root| all_handlers_installed(&root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_uses_codex_hooks_json() {
        assert_eq!(config_path().unwrap().file_name().unwrap(), "hooks.json");
    }

    #[test]
    fn add_is_idempotent_and_preserves_unrelated_hooks() {
        let source = json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":"other"}]}]}});
        let once = add_handlers(source.clone(), Path::new("/Applications/CodexTurnChime/helper")).unwrap();
        let twice = add_handlers(once.clone(), Path::new("/Applications/CodexTurnChime/helper")).unwrap();
        assert_eq!(once, twice);
        assert!(once.to_string().contains("other"));
        assert!(all_handlers_installed(&once));
    }

    #[test]
    fn uninstall_only_removes_exact_project_handler() {
        let source = add_handlers(
            json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":"other","statusMessage":"Other app"}]}]}}),
            Path::new("/Applications/CodexTurnChime/helper"),
        )
        .unwrap();
        let removed = remove_handlers(source).unwrap();
        assert!(removed.to_string().contains("other"));
        assert!(!removed.to_string().contains(HANDLER_STATUS));
    }

    #[test]
    fn invalid_hooks_shape_is_rejected() {
        let source = json!({"hooks": []});
        assert!(add_handlers(source, Path::new("helper")).is_err());
    }
}
