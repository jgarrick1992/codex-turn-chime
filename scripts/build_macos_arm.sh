#!/usr/bin/env bash

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
TARGET="aarch64-apple-darwin"
TAURI_MANIFEST="${PROJECT_ROOT}/src-tauri/Cargo.toml"
SIDECAR_PLACEHOLDER="${PROJECT_ROOT}/src-tauri/binaries/codex-turn-chime-hook-${TARGET}"

log() {
  printf '[build_macos_arm] %s\n' "$*"
}

# Keep any NVM or Homebrew PATH changes active for the release build.
# shellcheck source=./init_env.sh
source "${SCRIPT_DIR}/init_env.sh" --skip-debug-sidecar

cd "${PROJECT_ROOT}"

log "Building the frontend assets required by Tauri..."
npm run build

log "Staging the sidecar placeholder required by the Tauri build script..."
mkdir -p "$(dirname -- "${SIDECAR_PLACEHOLDER}")"
: > "${SIDECAR_PLACEHOLDER}"
chmod +x "${SIDECAR_PLACEHOLDER}"

log "Building the release Hook sidecar for ${TARGET}..."
cargo build \
  --manifest-path "${TAURI_MANIFEST}" \
  --release \
  --bin codex-turn-chime-hook \
  --target "${TARGET}"

log "Staging the release Hook sidecar..."
node "${PROJECT_ROOT}/scripts/stage-sidecar.mjs" "${TARGET}" release

log "Building the macOS ARM64 application and DMG..."
npm run tauri build -- --target "${TARGET}" --bundles app,dmg

log "Build complete."
log "App: ${PROJECT_ROOT}/src-tauri/target/${TARGET}/release/bundle/macos/CodexTurnChime.app"
log "DMG: ${PROJECT_ROOT}/src-tauri/target/${TARGET}/release/bundle/dmg/"
