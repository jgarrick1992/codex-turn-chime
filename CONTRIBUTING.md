# Contributing

Thanks for improving CodexTurnChime. Contributions should preserve its local-first privacy model and strict event contract.

## Before opening a pull request

1. Search existing issues and open a focused issue for behavior changes.
2. Keep prompts, answers, commands, tool input, and tool output outside every model, fixture, log, screenshot, and database migration.
3. Do not add field aliases, legacy-key maps, or guessed transcript compatibility. An unsupported format must fail closed.
4. Do not map a user interruption to `blocked`; use `stopped`.
5. Do not add network requests, telemetry, shell execution, or broad Tauri capabilities.

## Development checks

```bash
npm ci
npm run lint
npm run typecheck
npm test
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Use Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`, `chore:`). Update `CHANGELOG.md` for user-visible changes. Add boundary tests for malformed, duplicated, partial, reordered, or incompatible inputs.

## Pull requests

- Keep one coherent change per PR.
- Explain privacy and platform impact.
- Include macOS and/or Windows evidence when platform behavior changes.
- Never commit generated signing credentials, transcripts, application data, or user Hook configuration.
- Agree to the [Code of Conduct](CODE_OF_CONDUCT.md).
