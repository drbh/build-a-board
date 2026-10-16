#!/usr/bin/env bash
# Build the playground into playground/dist: prepare the compiler and bb-core
# (tools/assets.sh), then type-check and bundle the app with Vite.
# `npm run preview` (or any static server, plus KiCad's footprints under
# /footprints/) serves the result.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
"$here/tools/assets.sh"
cd "$here"
npm ci --no-audit --no-fund --silent
npm run build
echo "Playground built in $here/dist; serve it with: (cd $here && npm run preview)"
