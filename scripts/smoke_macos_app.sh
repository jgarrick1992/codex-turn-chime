#!/usr/bin/env bash

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
TARGET="${1:-aarch64-apple-darwin}"
APP="${PROJECT_ROOT}/src-tauri/target/${TARGET}/release/bundle/macos/CodexTurnChime.app"
MAIN="${APP}/Contents/MacOS/CodexTurnChime"
HELPER="${APP}/Contents/MacOS/codex-turn-chime-hook"
EXPECTED_ID="io.github.jgarrick1992.codexturnchime"
SOCKET="/tmp/io_github_jgarrick1992_codexturnchime_si.sock"
TEMP_ROOT=""
APP_PID=""

log() {
  printf '[smoke_macos_app] %s\n' "$*"
}

fail() {
  log "ERROR: $*"
  exit 1
}

cleanup() {
  if [[ -n "${APP_PID}" ]] && kill -0 "${APP_PID}" 2>/dev/null; then
    kill -TERM "${APP_PID}" 2>/dev/null || true
    wait "${APP_PID}" 2>/dev/null || true
  fi
  if [[ -n "${TEMP_ROOT}" ]]; then
    rm -rf "${TEMP_ROOT}"
  fi
}
trap cleanup EXIT INT TERM

socket_owner_pid() {
  lsof -U 2>/dev/null | awk -v socket="${SOCKET}" '$NF == socket { print $2; exit }'
}

[[ "$(uname -s)" == "Darwin" ]] || fail "this smoke test only supports macOS"
[[ -d "${APP}" ]] || fail "application bundle not found: ${APP}"
[[ -x "${MAIN}" ]] || fail "main executable is missing or not executable"
[[ -x "${HELPER}" ]] || fail "Hook helper is missing or not executable"

BUNDLE_ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "${APP}/Contents/Info.plist")"
[[ "${BUNDLE_ID}" == "${EXPECTED_ID}" ]] || fail "unexpected bundle identifier: ${BUNDLE_ID}"

ARCHS="$(lipo -archs "${MAIN}")"
[[ " ${ARCHS} " == *" arm64 "* ]] || fail "main executable is not arm64: ${ARCHS}"
HELPER_ARCHS="$(lipo -archs "${HELPER}")"
[[ " ${HELPER_ARCHS} " == *" arm64 "* ]] || fail "Hook helper is not arm64: ${HELPER_ARCHS}"

codesign --verify --deep --strict --verbose=2 "${APP}"

if lsof -U 2>/dev/null | grep -Fq "${SOCKET}"; then
  fail "a production CodexTurnChime instance is already running; quit it before the smoke test"
fi
rm -f "${SOCKET}"

TEMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/codex-turn-chime-smoke.XXXXXX")"
mkdir -p "${TEMP_ROOT}/home" "${TEMP_ROOT}/codex"

log "starting the packaged .app through LaunchServices in an isolated home directory"
/usr/bin/open -n \
  --env "HOME=${TEMP_ROOT}/home" \
  --env "CODEX_HOME=${TEMP_ROOT}/codex" \
  --env "RUST_LOG=warn" \
  "${APP}" --args --background

for _ in 1 2 3 4 5; do
  APP_PID="$(socket_owner_pid)"
  if [[ -n "${APP_PID}" ]]; then
    break
  fi
  sleep 1
done

[[ -n "${APP_PID}" ]] || fail "packaged app did not create its single-instance socket"
kill -0 "${APP_PID}" 2>/dev/null || fail "packaged app exited during startup"

log "verifying that a second production launch wakes the first instance"
/usr/bin/open \
  --env "HOME=${TEMP_ROOT}/home" \
  --env "CODEX_HOME=${TEMP_ROOT}/codex" \
  "${APP}" --args --background
sleep 2
kill -0 "${APP_PID}" 2>/dev/null || fail "first production instance stopped after the second launch"
[[ "$(socket_owner_pid)" == "${APP_PID}" ]] \
  || fail "second production launch did not retain the original single-instance owner"

log "packaged app smoke test passed"
