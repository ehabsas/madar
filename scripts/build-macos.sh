#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
if ! cargo tauri --version >/dev/null 2>&1; then
  cargo install tauri-cli --locked
fi
cargo tauri build --bundles app,dmg
printf '\nMadar macOS artifacts are under src-tauri/target/release/bundle/{macos,dmg}.\n'
printf 'Distribution outside the App Store needs Apple code signing and notarization.\n'
