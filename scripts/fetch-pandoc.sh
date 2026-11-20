#!/usr/bin/env bash
# Pinned Pandoc: the official wasm build for the app (into the UI assets) and,
# with --native, the matching Linux binary for tests (into ./pandoc/native).
set -euo pipefail
VERSION="3.10"
WASM_SHA="e0865674db6fa2698d29811ca2fcb91ab00a2f8b7d0220eae4ea28405d9cab2b"
NATIVE_SHA="e0f8af62d0f267d22baa5bcefe6d5dda3a097ccc60de794b759fe03159923244"
cd "$(dirname "$0")/.."
base="https://github.com/jgm/pandoc/releases/download/${VERSION}"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
check() { echo "$2  $1" | sha256sum -c --quiet - || { echo "checksum mismatch for $1" >&2; exit 1; }; }
out="crates/fo-ui/assets/pandoc"
if [ ! -f "$out/pandoc.wasm" ]; then
  curl -fsSL -o "$tmp/w.zip" "$base/pandoc-${VERSION}.wasm.zip"
  check "$tmp/w.zip" "$WASM_SHA"
  unzip -q -o "$tmp/w.zip" -d "$tmp/w"
  mkdir -p "$out" && cp "$tmp/w/pandoc/pandoc.wasm" "$out/" && cp "$tmp/w/pandoc/COPYRIGHT" "$out/PANDOC-COPYRIGHT.txt"
  echo "pandoc.wasm $VERSION → $out"
fi
if [ "${1:-}" = "--native" ] && [ ! -x pandoc/native/bin/pandoc ]; then
  curl -fsSL -o "$tmp/n.tgz" "$base/pandoc-${VERSION}-linux-amd64.tar.gz"
  check "$tmp/n.tgz" "$NATIVE_SHA"
  mkdir -p pandoc/native && tar xzf "$tmp/n.tgz" -C pandoc/native --strip-components=1
  echo "pandoc $VERSION (native) → pandoc/native"
fi
