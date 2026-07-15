#!/usr/bin/env bash

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
TAURI_MANIFEST="${PROJECT_ROOT}/src-tauri/Cargo.toml"

log() {
  printf '[init_env] %s\n' "$*"
}

fail() {
  printf '[init_env] ERROR: %s\n' "$*" >&2
  return 1
}

require_macos_apple_silicon() {
  if [[ "$(uname -s)" != "Darwin" ]]; then
    fail "This development script currently supports macOS only."
    return
  fi
  if [[ "$(uname -m)" != "arm64" ]]; then
    fail "CodexTurnChime v0.1 development requires an Apple Silicon Mac."
    return
  fi

  local macos_version macos_major
  macos_version="$(sw_vers -productVersion)"
  macos_major="${macos_version%%.*}"
  if (( macos_major < 13 )); then
    fail "macOS 13 or newer is required; found ${macos_version}."
    return
  fi
  log "macOS ${macos_version} on Apple Silicon"
}

ensure_xcode_tools() {
  if ! xcode-select -p >/dev/null 2>&1; then
    log "Requesting installation of the Xcode Command Line Tools..."
    xcode-select --install >/dev/null 2>&1 || true
    fail "Complete the Xcode Command Line Tools installer, then run this script again."
    return
  fi
  if ! xcrun --find clang >/dev/null 2>&1; then
    fail "Xcode Command Line Tools are present but clang is unavailable."
    return
  fi
  log "Xcode Command Line Tools ready"
}

node_major_version() {
  node -p 'process.versions.node.split(".")[0]' 2>/dev/null
}

load_nvm() {
  local nvm_script=""
  if [[ -n "${NVM_DIR:-}" ]]; then
    nvm_script="${NVM_DIR}/nvm.sh"
  elif [[ -n "${HOME:-}" ]]; then
    nvm_script="${HOME}/.nvm/nvm.sh"
  fi
  if [[ -z "${nvm_script}" || ! -s "${nvm_script}" ]]; then
    return 1
  fi

  set +u
  # shellcheck source=/dev/null
  source "${nvm_script}"
  set -u
}

ensure_node() {
  if command -v node >/dev/null 2>&1 && command -v npm >/dev/null 2>&1 && [[ "$(node_major_version)" == "22" ]]; then
    log "Node.js $(node --version) and npm $(npm --version) ready"
    return
  fi

  if load_nvm; then
    local installed_version
    installed_version="$(nvm version 22 2>/dev/null || true)"
    if [[ "${installed_version}" == "N/A" ]]; then
      log "Installing Node.js 22 LTS with NVM..."
      nvm install 22
    else
      log "Switching to Node.js ${installed_version} with NVM..."
      nvm use 22 >/dev/null
    fi
  elif command -v brew >/dev/null 2>&1; then
    if ! brew list --versions node@22 >/dev/null 2>&1; then
      log "Installing Node.js 22 LTS with Homebrew..."
      brew install node@22
    fi
    export PATH="$(brew --prefix node@22)/bin:${PATH}"
    hash -r
  else
    fail "Node.js 22 is required. Install NVM or Homebrew, then run this script again."
    return
  fi

  if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1 || [[ "$(node_major_version)" != "22" ]]; then
    fail "Unable to activate Node.js 22."
    return
  fi
  log "Node.js $(node --version) and npm $(npm --version) ready"
}

rust_version_is_supported() {
  local release major remainder minor
  release="$(rustc --version | awk '{print $2}')"
  major="${release%%.*}"
  remainder="${release#*.}"
  minor="${remainder%%.*}"
  (( major > 1 || (major == 1 && minor >= 77) ))
}

ensure_rust() {
  if ! command -v cargo >/dev/null 2>&1 || ! command -v rustc >/dev/null 2>&1; then
    if ! command -v brew >/dev/null 2>&1; then
      fail "Stable Rust 1.77 or newer is required. Install Homebrew or rustup, then run this script again."
      return
    fi
    log "Installing rustup with Homebrew..."
    brew install rustup
    export PATH="$(brew --prefix rustup)/bin:${PATH}"
    rustup default stable
    hash -r
  fi

  if ! rust_version_is_supported; then
    if command -v rustup >/dev/null 2>&1; then
      log "Updating the stable Rust toolchain..."
      rustup update stable
      rustup default stable
      hash -r
    else
      fail "Rust 1.77 or newer is required; found $(rustc --version)."
      return
    fi
  fi

  local target
  target="$(rustc -vV | awk '/^host:/ { print $2 }')"
  if [[ "${target}" != "aarch64-apple-darwin" ]]; then
    fail "Expected Rust host aarch64-apple-darwin; found ${target}."
    return
  fi
  if command -v rustup >/dev/null 2>&1; then
    rustup target add "${target}" >/dev/null
  fi
  log "$(rustc --version) and $(cargo --version) ready"
}

ensure_npm_dependencies() {
  local installed_lock="${PROJECT_ROOT}/node_modules/.package-lock.json"
  if [[ ! -x "${PROJECT_ROOT}/node_modules/.bin/tauri" || ! -x "${PROJECT_ROOT}/node_modules/.bin/vite" || ! -f "${installed_lock}" || "${PROJECT_ROOT}/package.json" -nt "${installed_lock}" || "${PROJECT_ROOT}/package-lock.json" -nt "${installed_lock}" ]]; then
    log "Installing npm dependencies..."
    npm install
  else
    log "npm dependencies ready"
  fi
}

sidecar_needs_rebuild() {
  local sidecar="$1"
  if [[ ! -x "${sidecar}" ]]; then
    return 0
  fi
  if [[ -n "$(find "${PROJECT_ROOT}/src-tauri/src" -type f -newer "${sidecar}" -print -quit)" ]]; then
    return 0
  fi
  [[ "${PROJECT_ROOT}/src-tauri/Cargo.toml" -nt "${sidecar}" || "${PROJECT_ROOT}/src-tauri/Cargo.lock" -nt "${sidecar}" ]]
}

ensure_debug_sidecar() {
  local target sidecar
  target="$(rustc -vV | awk '/^host:/ { print $2 }')"
  sidecar="${PROJECT_ROOT}/src-tauri/binaries/codex-turn-chime-hook-${target}"

  if sidecar_needs_rebuild "${sidecar}"; then
    log "Building and staging the debug Hook sidecar..."
    cargo build --manifest-path "${TAURI_MANIFEST}" --bin codex-turn-chime-hook --target "${target}"
    node "${PROJECT_ROOT}/scripts/stage-sidecar.mjs" "${target}" debug
    chmod +x "${sidecar}"
  else
    log "Debug Hook sidecar ready"
  fi

  if ! codesign --verify --strict "${sidecar}" >/dev/null 2>&1; then
    log "Applying an ad-hoc signature to the debug Hook sidecar..."
    codesign --force --sign - "${sidecar}"
  fi
}

main() {
  local skip_debug_sidecar=false
  if [[ "${1:-}" == "--skip-debug-sidecar" ]]; then
    skip_debug_sidecar=true
    shift
  fi
  if (( $# > 0 )); then
    fail "Unknown option: $1"
    return
  fi

  cd "${PROJECT_ROOT}"
  require_macos_apple_silicon
  ensure_xcode_tools
  ensure_node
  ensure_rust
  ensure_npm_dependencies
  if [[ "${skip_debug_sidecar}" == false ]]; then
    ensure_debug_sidecar
  fi
  log "Environment is ready"
}

main "$@"
