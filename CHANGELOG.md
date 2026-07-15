# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-beta.2] - 2026-07-16

### Added

- Initial Tauri 2, React, TypeScript, and Rust project implementation.
- `MonitorEvent v1`, local queue, SQLite state reducer, and strict transcript adapter.
- Safe Hook installer, bilingual desktop interface, custom sound controls, diagnostics, CI, and release workflows.
- Offline Chinese voice prompts with distinct messages for permission, input, and completion events.
- Repeat sound reminders with a configurable interval (default: 5 seconds) and automatic cancellation when the main window gains focus.
- Sound amplification up to 200% with compression on boosted playback.
- Default bilingual Lumi AI Voice 01 scheme with all eight attention and outcome prompts, automatically following the UI language.
- One-click action to mark every unread task as read.
- Configurable global shortcut to stop the current reminder and cancel its repeat loop without focusing the app (default: `CommandOrControl+Shift+K`).

### Fixed

- Install Codex lifecycle handlers into the supported `hooks.json` configuration layer so Hook events can reach the local monitor.
- Restore the hidden main window from the macOS Dock, a tray-icon left click, the tray menu, or a second application launch after the window is closed.
- Stage the target sidecar placeholder before compiling the Hook helper so clean macOS and Windows build environments can package the app.
- Build the frontend distribution before compiling release binaries on clean GitHub runners.

[Unreleased]: https://github.com/jgarrick1992/codex-turn-chime/compare/v0.1.0-beta.2...HEAD
[0.1.0-beta.2]: https://github.com/jgarrick1992/codex-turn-chime/releases/tag/v0.1.0-beta.2
