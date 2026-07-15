# Test strategy

The release gate covers:

- Exact Hook transitions for Running, Needs input, and Ready
- Interrupted turns becoming Stopped, never Blocked
- Transcript `request_user_input` call/output matching
- Duplicate, late, equal-time, and out-of-order events
- JSONL partial lines, invalid records, file truncation, and rotation checkpoints
- Fail-closed behavior for changed recognized transcript structures
- Hook preservation, idempotent install, backup, verification, and exact uninstall
- Missing, corrupt, oversized, or unreadable custom sound files
- App restart, tray restore, startup opt-in, mute, and retention cleanup
- Privacy assertions that schemas and fixtures have no content fields
- Clean GitHub-runner packaging on both supported platforms

Fixtures under `tests/fixtures` contain synthetic IDs and paths only. Real user transcripts are prohibited in the repository.
