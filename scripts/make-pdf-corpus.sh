#!/usr/bin/env bash
# Rebuilds tests/pdf-corpus/*.pdf from tests/pdf-corpus/src (needs Chrome and
# python3-reportlab). The sources are the ground truth for the PDF tests.
set -euo pipefail
cd "$(dirname "$0")/../tests/pdf-corpus/src"
chrome="${CHROME:-google-chrome}"
for f in *.html; do
  "$chrome" --headless=new --no-sandbox --disable-gpu --no-pdf-header-footer \
    --print-to-pdf="../${f%.html}.pdf" "file://$PWD/$f"
done
python3 notes_reportlab.py
python3 footnotes_reportlab.py
