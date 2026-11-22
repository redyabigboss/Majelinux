#!/bin/sh
set -eu
prefix="${PREFIX:-$HOME/.local}"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
rm -f "$prefix/bin/majelinux" "$data/applications/majelinux.desktop" \
  "$data/icons/hicolor/scalable/apps/majelinux.svg" "$data/icons/hicolor/256x256/apps/majelinux.png"
