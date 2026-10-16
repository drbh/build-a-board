#!/usr/bin/env bash
# Prepare what the app loads but does not build itself, under playground/.cache:
#   1. fetch the browser compiler (pinned, verified);
#   2. prebuild wit-bindgen and bb-sdk with that same compiler, so boards
#      compiled in the browser can link against them        -> public/compiler/
#   3. build bb-core for the browser (wasm-pack)            -> core/
# Rerun after changing the SDK, wit/, or bb-core.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(dirname "$here")"
compiler="$here/.cache/compiler"
public="$here/.cache/public"

"$here/tools/fetch-compiler.sh"

echo "Building bb-sdk with the browser compiler..."
work="$here/.cache/sdk-build"
rm -rf "${work:?}"
mkdir -p "$work/deps"
bindgen_src="$(cargo metadata --format-version 1 --manifest-path "$repo/sdk/Cargo.toml" \
  | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const p=JSON.parse(s).packages.find(p=>p.name==="wit-bindgen");console.log(require("path").dirname(p.manifest_path))})')"
cp -R "$bindgen_src/src" "$work/wit-bindgen"
cp -R "$repo/sdk/src" "$work/bb-sdk"
rustc() { node "$here/tools/rustc.mjs" "$compiler/rustc.wasm" "$compiler/sysroot-wasip1" "$work" -- "$@"; }
common=(--sysroot /sysroot -Zunstable-options --target wasm32-wasip1 -O -Cpanic=abort --out-dir /work/deps)
# wit-bindgen's build script only links a C allocator used on wasip2; wasip1 needs none.
rustc /work/wit-bindgen/lib.rs --crate-name wit_bindgen --crate-type rlib --edition 2024 \
  --cfg 'feature="std"' --cfg 'feature="realloc"' "${common[@]}"
rustc /work/bb-sdk/lib.rs --crate-name bb_sdk --crate-type rlib --edition 2024 \
  --extern wit_bindgen=/work/deps/libwit_bindgen.rlib -L dependency=/work/deps "${common[@]}"
rm -rf "${public:?}/compiler"
mkdir -p "$public/compiler"
cp "$compiler/rustc.wasm" "$compiler/sysroot-wasip1.bundle" "$work/deps/libbb_sdk.rlib" \
  "$work/deps/libwit_bindgen.rlib" "$public/compiler/"

echo "Building bb-core for the browser..."
wasm-pack build "$here/core-wasm" --release --target web --no-typescript --no-pack \
  --out-dir "$here/.cache/core" --out-name bb_core_wasm
rm -f "$here/.cache/core/.gitignore"
