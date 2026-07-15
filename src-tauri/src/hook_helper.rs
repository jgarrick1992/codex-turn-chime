use std::io::Read;

use serde::Deserialize;

use crate::{
    error::{AppError, AppResult},
    monitor::{MonitorEvent, MonitorKind},
    paths::AppPaths,
    queue,
};

#[derive(Debug, Deserialize)]
struct HookInput {
    session_id: String,
    turn_id: String,
    cwd: String,
    hook_event_name: String,
}

fn convert(input: HookInput) -> AppResult<MonitorEvent> {
    let (kind, reason) = match input.hook_event_name.as_str() {
        "UserPromptSubmit" => (MonitorKind::Running, "user_prompt_submitted"),
        "PermissionRequest" => (MonitorKind::NeedsInput, "permission_requested"),
        "Stop" => (MonitorKind::Ready, "turn_stopped"),
        event => return Err(AppError::IncompatibleFormat(format!("unsupported hook event: {event}"))),
    };
    Ok(MonitorEvent::new_hook(input.session_id, input.turn_id, kind, input.cwd, reason))
}

pub fn run_from_reader(mut reader: impl Read) -> AppResult<()> {
    let mut buffer = Vec::new();
    reader.by_ref().take(1_048_577).read_to_end(&mut buffer)?;
    if buffer.len() > 1_048_576 {
        return Err(AppError::InvalidConfig("hook input exceeds 1 MiB".into()));
    }
    let input: HookInput = serde_json::from_slice(&buffer)?;
    let event = convert(input)?;
    let paths = AppPaths::discover()?;
    paths.ensure()?;
    queue::append(&paths.queue, &event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_request_needs_input() {
        let event = convert(HookInput {
            session_id: "s".into(),
            turn_id: "t".into(),
            cwd: "/work".into(),
            hook_event_name: "PermissionRequest".into(),
        })
        .unwrap();
        assert_eq!(event.kind, MonitorKind::NeedsInput);
        assert_eq!(event.reason.as_deref(), Some("permission_requested"));
    }

    #[test]
    fn unknown_hook_name_is_not_guessed() {
        let result = convert(HookInput {
            session_id: "s".into(),
            turn_id: "t".into(),
            cwd: "/work".into(),
            hook_event_name: "LegacyStop".into(),
        });
        assert!(matches!(result, Err(AppError::IncompatibleFormat(_))));
    }
}
