# Security Policy

## Supported versions

| Version | Supported |
| --- | --- |
| Latest beta | Yes |
| Older prereleases | No |

## Report a vulnerability

Do not open a public issue. Use GitHub's private vulnerability reporting for this repository. If that is unavailable, contact the repository owner through the verified email on the GitHub profile and include `CodexTurnChime security` in the subject.

Include the affected version, platform, reproduction steps, impact, and any suggested mitigation. Do not attach real transcripts, prompts, credentials, or private Hook configuration. Maintainers aim to acknowledge a report within 5 working days and will coordinate disclosure after a fix is available.

## Security boundaries

- No remote service, account, telemetry, or crash upload exists in v0.1.
- The Hook helper reads one bounded JSON object from stdin and exits successfully even if monitoring fails.
- Hook configuration is previewed, backed up, atomically replaced, and verified.
- Transcript monitoring is opt-in, read-only, strict, and disabled on an incompatible recognized format.
- Custom audio is read only after an explicit user file selection and is limited to validated WAV/MP3 files up to 25 MiB.
- Tauri capabilities and CSP are intentionally narrow. Arbitrary shell execution is prohibited.

See [Security model](docs/security-model.md).
