# Troubleshooting

## No events arrive

Open **Diagnostics** and verify the helper, Hook, queue, and database. Preview the Hook config and confirm that all three handlers are installed. Restart Codex after changing Hook configuration. If `CODEX_HOME` is customized, launch CodexTurnChime with the same environment.

## Helper is missing

Development and release builds must stage the target-triple sidecar before Tauri packaging. Build `codex-turn-chime-hook`, then run `node scripts/stage-sidecar.mjs <target> release`. Do not install the Hook with an arbitrary or stale helper path.

## Custom audio fails

Only readable WAV and MP3 files up to 25 MiB are accepted. The file extension and signature must agree. Re-select a moved file. CodexTurnChime reports the error and does not silently substitute another custom file.

## Transcript watcher disabled itself

This is expected fail-closed behavior. Disable the watcher, review the diagnostic message, and report the exact application/Codex versions without attaching transcript content. A new adapter version must be reviewed and released; old keys are not mapped automatically.

## Gatekeeper or SmartScreen warning

The first beta is not formally signed. Verify the release SHA-256, SBOM, and GitHub artifact attestation. Do not globally disable Gatekeeper, SmartScreen, or other operating-system protections.

## Data reset

Use **Clear history** for event data. Settings and Hook backups are separate files in the app data directory. Uninstall the Hook before deleting the application if you no longer want Codex to invoke the helper.
