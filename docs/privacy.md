# Privacy

CodexTurnChime is designed so conversation content is unnecessary.

## Stored

- Event schema version and ID
- Source, session ID, and turn ID
- One of six status values
- Event time and working directory
- A fixed, privacy-safe reason code
- Watcher file identity and byte offset
- Local application settings and selected audio path

## Never stored

- Prompts or answers
- Command text
- Tool input or output
- Transcript line contents
- Environment variables, credentials, tokens, or clipboard data
- Analytics, device identifiers, or crash reports

The transcript watcher necessarily reads new local JSONL lines in memory to inspect structural fields. It does not persist raw lines and stops on an incompatible recognized format. It is off by default.

All data stays in the application data directory. History is automatically removed after 30 days and **Clear history** deletes events, materialized states, and watcher checkpoints immediately. Hook-config backups are not conversation history and remain until the user removes them.
