#!/usr/bin/env bash
# Fetch the browser compiler (rustc compiled to WebAssembly, with Cranelift)
# that weblings publishes, verify it against pinned hashes, and unpack it into
# playground/.cache/compiler: rustc.wasm, the wasm32-wasip1 standard library,
# and the single-file sysroot bundle the browser loads.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cache="$here/.cache"
out="$cache/compiler"

# Pinned to weblings' artifacts.lock (github.com/AngelOnFira/weblings).
release="https://github.com/AngelOnFira/wasm-rustc/releases/download/artifacts-test-7"
assets=(
  "rustc-wasm.tar.zst a96f6d53afff3c95d6387def27f6ddb53a02575679dc6c981d60797c32dcd022"
  "wasip1-sysroot.tar.zst 4eedff7b0cd4330bfe226734b67751a97fac5572ac55392e93f9d3e4886a277d"
)

if [[ -f "$out/rustc.wasm" && -f "$out/sysroot-wasip1.bundle" && -d "$out/sysroot-wasip1/lib" ]]; then
  exit 0
fi
command -v zstd >/dev/null || { echo "fetch-compiler: zstd is required" >&2; exit 1; }
mkdir -p "$cache" "$out"
for entry in "${assets[@]}"; do
  read -r name sha <<<"$entry"
  if [[ ! -f "$cache/$name" ]]; then
    echo "Downloading $name..."
    curl -fL --retry 3 -o "$cache/$name.part" "$release/$name"
    mv "$cache/$name.part" "$cache/$name"
  fi
  actual="$(shasum -a 256 "$cache/$name" | cut -d' ' -f1)"
  if [[ "$actual" != "$sha" ]]; then
    echo "fetch-compiler: $name has sha256 $actual, expected $sha" >&2
    exit 1
  fi
  zstd -dc "$cache/$name" | tar -xf - -C "$out" --strip-components 1
done
echo "Compiler ready in $out"
