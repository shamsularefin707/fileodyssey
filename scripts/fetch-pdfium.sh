#!/usr/bin/env bash
# Downloads the pinned PDFium build into ./pdfium/<platform>/ (default: this machine).
# Usage: scripts/fetch-pdfium.sh [linux-x64|win-x64|android-arm64|android-arm|android-x64|android-x86 ...]
set -euo pipefail
VERSION="8125"
cd "$(dirname "$0")/.."
targets=("$@")
[ ${#targets[@]} -eq 0 ] && targets=("linux-x64")
for t in "${targets[@]}"; do
  dest="pdfium/$t"
  [ -f "$dest/VERSION" ] && grep -q "BUILD=$VERSION" "$dest/VERSION" && continue
  rm -rf "${dest:?}" && mkdir -p "$dest"
  curl -fsSL "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F${VERSION}/pdfium-${t}.tgz" | tar xz -C "$dest"
  echo "PDFium $VERSION ($t) → $dest"
done
