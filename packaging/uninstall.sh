#!/bin/sh
set -eu
prefix="${PREFIX:-$HOME/.local}"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
rm -f "$prefix/bin/Majelinux" "$data/applications/Majelinux.desktop" \
  "$data/icons/hicolor/scalable/apps/Majelinux.svg" "$data/icons/hicolor/256x256/apps/Majelinux.png"
