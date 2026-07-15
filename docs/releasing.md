# Release process

CodexTurnChime follows Semantic Versioning and Keep a Changelog. The first tag is `v0.1.0-beta.1`.

## Before tagging

1. Confirm `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and `CHANGELOG.md` use the same version.
2. Pass frontend and Rust checks on clean macOS and Windows runners.
3. Pass `cargo audit`, `cargo deny`, and CodeQL.
4. Complete real-device acceptance for macOS 13+ Apple Silicon and Windows 11 x64.
5. Review third-party licenses, generated SBOMs, privacy behavior, Hook backup/uninstall, and the unsigned-beta warning.

## Automated workflow

A `v*` tag starts `.github/workflows/release.yml`:

- macOS builds an ARM64 DMG with ad-hoc signing.
- Windows builds x64 NSIS EXE and MSI installers.
- The helper is built first and staged as a target-triple Tauri sidecar.
- Each job creates CycloneDX SBOMs, SHA-256 files, and GitHub build provenance.
- Assets are attached to a Draft Prerelease.

A maintainer must inspect every asset and manually publish the draft. Do not tag again merely to replace a broken immutable artifact; document and issue a new prerelease version.

## Future signing secrets

Reserved names, not required for the unsigned beta:

- `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`
- `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`
- `WINDOWS_CERTIFICATE`, `WINDOWS_CERTIFICATE_PASSWORD`
- `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

Never print secrets or put them in repository files. Notarization and updater signing require a reviewed workflow change.
