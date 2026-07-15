# Security model

## Assets

- User Codex configuration and unrelated Hooks
- Local task metadata and working-directory paths
- User-selected audio files
- Release artifacts and update trust

## Threats and controls

| Threat | Control |
| --- | --- |
| Hook notifier blocks Codex | Helper always exits 0, one-second Hook timeout, bounded input, no network |
| Configuration loss | Parse before mutation, explicit diff/confirmation, backup, same-directory atomic replace, verification |
| Removing user Hooks | Idempotent append and exact project-handler removal only |
| Content collection | Event schema has no content fields; no raw transcript persistence; privacy tests |
| Transcript schema drift | Explicit opt-in adapter, exact keys, checkpointed reads, fail-closed health state |
| Duplicate/late events | Unique event ID and timestamp/tie-breaker reducer |
| Arbitrary file read | File dialog plus backend WAV/MP3 signature and 25 MiB limit |
| WebView command execution | No shell plugin, strict invoke allowlist, minimal capabilities, CSP |
| Dependency compromise | Lockfiles, Dependabot, CodeQL, cargo-audit, cargo-deny, SBOM, checksums, attestations |

`cargo-audit` remains a blocking vulnerability check. `cargo-deny` explicitly
ignores RustSec advisory `RUSTSEC-2025-0098` because the Tauri dependency graph
currently includes `unic-ucd-version`, for which RustSec records no safe
upgrade; this narrow exception does not suppress other vulnerability
advisories.
| Unsigned beta impersonation | Draft/manual release, SHA-256, provenance, explicit signing warning |

## Deliberate limitations

The local user account can modify application data and Hook settings. CodexTurnChime is not a security boundary against a compromised local account. Working-directory paths may themselves contain sensitive names, so issue reports should redact them.

Future App Server control, network features, automatic updates, or content display require a new threat model and are not authorized by the v0.1 design.
