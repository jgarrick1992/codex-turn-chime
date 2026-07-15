# Third-party notices

CodexTurnChime is distributed under the MIT License. Its dependencies retain their own licenses.

Primary runtime projects include Tauri, React, Rust crates, Lucide, SQLite, and the platform WebView runtimes. CI produces CycloneDX SBOM files for the exact JavaScript and Rust dependency graphs included in each release.

The original application icon and code-generated built-in chimes were created for this project and do not use OpenAI/Codex official logos or redistributed audio samples.

Before publishing a release, maintainers must run `cargo deny check licenses` and review the generated SBOM and license exceptions. If an asset or dependency cannot be redistributed under compatible terms, it must not ship.
