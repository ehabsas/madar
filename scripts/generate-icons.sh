#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
if ! cargo tauri --version >/dev/null 2>&1; then
  cargo install tauri-cli --locked
fi
cargo tauri icon src-tauri/icons/madar.svg --output src-tauri/icons
printf 'Application icons regenerated under src-tauri/icons.\n'
