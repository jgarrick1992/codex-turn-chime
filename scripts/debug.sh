#!/usr/bin/env bash

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

# Source the initializer so an NVM/Homebrew PATH change remains active here.
# shellcheck source=./init_env.sh
source "${SCRIPT_DIR}/init_env.sh"

cd "${PROJECT_ROOT}"
exec npm run tauri dev
