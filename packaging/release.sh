#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
out="dist"

cargo build --release
rm -rf "$out"
mkdir -p "$out"
cp target/release/Majelinux "$out/Majelinux-linux-x86_64"
strip "$out/Majelinux-linux-x86_64" || true

tar -C "$out" -czf "$out/Majelinux-$version-linux-x86_64.tar.gz" Majelinux-linux-x86_64
(cd "$out" && sha256sum Majelinux-linux-x86_64 "Majelinux-$version-linux-x86_64.tar.gz" > SHA256SUMS)

echo "Tag: v$version"
echo "Create the release:"
echo "gh release create v$version $out/Majelinux-linux-x86_64 $out/Majelinux-$version-linux-x86_64.tar.gz $out/SHA256SUMS --title v$version --generate-notes"
