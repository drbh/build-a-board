#!/usr/bin/env bash
# Upload the R2 files the live site doesn't have yet, then deploy the worker
# and site/. Needs CLOUDFLARE_API_TOKEN (Workers Scripts:Edit, R2:Edit) or
# `wrangler login`. Run stage.sh first.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
site="${BB_SITE_URL:-https://bb.dholtz.com}"
bucket="bb-playground-compiler"
cd "$here"
[[ -f site/compiler/r2.json ]] || { echo "deploy: run stage.sh first" >&2; exit 1; }
for file in r2/*; do
  key="$(basename "$file")"
  if curl -sfI "$site/compiler/$key" >/dev/null; then
    echo "R2 already has $key"
  else
    npx wrangler r2 object put "$bucket/$key" --file "$file" --remote
  fi
done
npx wrangler deploy
