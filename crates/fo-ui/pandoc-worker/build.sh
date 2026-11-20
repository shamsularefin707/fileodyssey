#!/usr/bin/env bash
# Rebuilds ../assets/pandoc/pandoc-worker.js (committed) from worker.js and
# pandoc-core.js with the pinned packages in package-lock.json.
set -euo pipefail
cd "$(dirname "$0")"
npm ci --silent
npm run -s build
