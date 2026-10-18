#!/usr/bin/env bash
# Assemble what the hosted site serves, from playground/dist (build.sh and
# `npm run book`) and KiCad's footprint libraries:
#   site/   static assets: the playground, the book, footprints/<Lib>.pretty/*
#   r2/     compiler files over the 25 MiB static-asset limit, named by sha256,
#           with site/compiler/r2.json mapping each file name to its key
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
dist="$here/../dist"
footprints="${KICAD_FOOTPRINT_DIR:-}"
for dir in "$footprints" /Applications/KiCad/KiCad.app/Contents/SharedSupport/footprints /usr/share/kicad/footprints; do
  if [[ -n "$dir" && -d "$dir" ]]; then footprints="$dir"; break; fi
done
[[ -d "$dist/compiler" && -d "$dist/book" ]] || { echo "stage: build the playground and the book first (make site)" >&2; exit 1; }
[[ -d "$footprints" ]] || { echo "stage: no KiCad footprint libraries found; set KICAD_FOOTPRINT_DIR" >&2; exit 1; }

rm -rf "$here/site" "$here/r2"
mkdir -p "$here/r2"
cp -R "$dist" "$here/site"
entries=()
for name in rustc.wasm sysroot-wasip1.bundle; do
  sha="$(shasum -a 256 "$here/site/compiler/$name" | cut -d' ' -f1)"
  mv "$here/site/compiler/$name" "$here/r2/$sha"
  entries+=("\"$name\":\"$sha\"")
done
(IFS=,; echo "{${entries[*]}}") > "$here/site/compiler/r2.json"

mkdir -p "$here/site/footprints"
for lib in "$footprints"/*.pretty; do
  mkdir -p "$here/site/footprints/$(basename "$lib")"
  cp "$lib"/*.kicad_mod "$here/site/footprints/$(basename "$lib")/" 2>/dev/null || true
done

large="$(find "$here/site" -type f -size +25M)"
[[ -z "$large" ]] || { echo "stage: files over the 25 MiB static-asset limit:" >&2; echo "$large" >&2; exit 1; }
echo "Staged $(find "$here/site" -type f | wc -l | tr -d ' ') files in site/ and $(ls "$here/r2" | wc -l | tr -d ' ') in r2/"
