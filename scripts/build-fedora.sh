#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
if ! pkg-config --exists webkit2gtk-4.1; then
  echo "Missing WebKitGTK development files. Run the Fedora prerequisites in docs/BUILDING.md." >&2
  exit 2
fi
if ! cargo tauri --version >/dev/null 2>&1; then
  cargo install tauri-cli --locked
fi
cargo tauri build --bundles rpm,appimage
printf '\nMadar Fedora artifacts are under src-tauri/target/release/bundle/{rpm,appimage}.\n'
