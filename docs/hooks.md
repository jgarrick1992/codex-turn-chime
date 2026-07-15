# Hook integration

The official Codex Hook is the default status source. CodexTurnChime installs command handlers for `UserPromptSubmit`, `PermissionRequest`, and `Stop`, following the official [Codex Hooks documentation](https://learn.chatgpt.com/docs/hooks).

## Safe installation sequence

1. Locate `${CODEX_HOME:-~/.codex}/hooks.json`.
2. If the file exists, parse it as JSON. Refuse any invalid JSON or incompatible `hooks` shape.
3. Build the complete proposed JSON in memory and show a before/after diff.
4. Wait for explicit user confirmation.
5. Copy the current file into the application backup directory.
6. Write a temporary file in the same directory, flush and sync it, then atomically replace the target.
7. Re-read the target and verify exact JSON equality.
8. Review and trust the exact command Hook definition in Codex with `/hooks`.

Existing Hook groups and handlers are preserved. Repeated installation is idempotent. Uninstall removes only command handlers with the exact project status marker and never restores an old backup over newer user changes.

The generated handler includes `command` and `commandWindows`, a one-second timeout, and the status text `CodexTurnChime: recording task state`. The packaged helper must exist before installation is allowed.

Codex skips new or changed non-managed command Hooks until the user reviews and trusts their exact definition. Restart Codex after installing or changing the Hook configuration.

## Helper behavior

The helper accepts a single JSON object no larger than 1 MiB and requires exact `session_id`, `turn_id`, `cwd`, and `hook_event_name` fields. Extra official fields are ignored, but field aliases are not accepted.

| Hook event | Monitor state | Safe reason |
| --- | --- | --- |
| `UserPromptSubmit` | `running` | `user_prompt_submitted` |
| `PermissionRequest` | `needs_input` | `permission_requested` |
| `Stop` | `ready` | `turn_stopped` |

Unknown event names produce no monitoring event. The helper writes no stdout and always exits successfully so a notifier failure cannot affect Codex.
