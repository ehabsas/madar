#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ "$(id -u)" -eq 0 ]]; then
  echo "Do not run this script with sudo; makepkg refuses to package as root." >&2
  exit 2
fi
if ! command -v makepkg >/dev/null 2>&1; then
  echo "makepkg was not found. Install base-devel using: sudo pacman -S --needed base-devel" >&2
  exit 2
fi
if ! pkg-config --exists webkit2gtk-4.1; then
  echo "Missing WebKitGTK development files. Run the Arch prerequisites in docs/BUILDING.md." >&2
  exit 2
fi
if ! cargo tauri --version >/dev/null 2>&1; then
  cargo install tauri-cli --locked
fi

# Build the native release executable; makepkg then creates a standard pacman package.
cargo tauri build --no-bundle
VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' src-tauri/Cargo.toml | head -n1)"
[[ -n "$VERSION" ]] || { echo "Could not read package version." >&2; exit 2; }
BIN="$ROOT/src-tauri/target/release/madar"
[[ -x "$BIN" ]] || { echo "Release executable not found: $BIN" >&2; exit 2; }

STAGE="$ROOT/src-tauri/target/release/arch-package"
OUT="$ROOT/src-tauri/target/release/bundle/arch"
rm -rf "$STAGE"
mkdir -p "$STAGE" "$OUT"
cp "$BIN" "$STAGE/madar"
cp "$ROOT/src-tauri/icons/128x128.png" "$STAGE/madar.png"
cp "$ROOT/README.md" "$STAGE/README.md"
cat > "$STAGE/madar.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Madar
Name[ar]=مدار
Comment=Business and inventory management
Exec=madar
Icon=madar
Terminal=false
Categories=Office;Finance;
StartupWMClass=madar
DESKTOP
cat > "$STAGE/PKGBUILD" <<PKGBUILD
pkgname=madar
pkgver=${VERSION}
pkgrel=1
pkgdesc='Madar business and inventory management'
arch=('x86_64')
depends=('webkit2gtk-4.1' 'gtk3' 'openssl' 'libappindicator-gtk3' 'librsvg')
source=('madar' 'madar.desktop' 'madar.png' 'README.md')
sha256sums=('SKIP' 'SKIP' 'SKIP' 'SKIP')

package() {
  install -Dm755 "\$srcdir/madar" "\$pkgdir/usr/bin/madar"
  install -Dm644 "\$srcdir/madar.desktop" "\$pkgdir/usr/share/applications/madar.desktop"
  install -Dm644 "\$srcdir/madar.png" "\$pkgdir/usr/share/icons/hicolor/128x128/apps/madar.png"
  install -Dm644 "\$srcdir/README.md" "\$pkgdir/usr/share/doc/madar/README.md"
}
PKGBUILD

(
  cd "$STAGE"
  makepkg --cleanbuild --force --nodeps
)
find "$STAGE" -maxdepth 1 -type f -name '*.pkg.tar.*' -exec cp -f {} "$OUT/" \;
printf '\nMadar Arch package created in %s\n' "$OUT"
