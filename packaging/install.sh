#!/bin/sh
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
bin="$root/target/release/Majelinux"
[ -x "$bin" ] || { echo "build first: cargo build --release" >&2; exit 1; }
prefix="${PREFIX:-$HOME/.local}"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
install -Dm755 "$bin" "$prefix/bin/Majelinux"
install -Dm644 "$root/packaging/Majelinux.desktop" "$data/applications/Majelinux.desktop"
install -Dm644 "$root/assets/icon.svg" "$data/icons/hicolor/scalable/apps/Majelinux.svg"
install -Dm644 "$root/assets/icon.png" "$data/icons/hicolor/256x256/apps/Majelinux.png"
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$data/applications" || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q "$data/icons/hicolor" || true
