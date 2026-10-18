// Run the browser compiler (rustc.wasm) under Node's WASI, the way the
// playground's worker runs it, to prebuild the libraries boards link against.
//
// usage: node rustc.mjs <rustc.wasm> <sysroot dir> <work dir> -- <rustc args...>
// The sysroot is mounted at /sysroot and the work directory at /work.
import { readFile, mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { WASI } from "node:wasi";

// Node 22 can segfault while this WASI compiler builds wit-bindgen.
if (Number(process.versions.node.split(".")[0]) < 24) {
  console.error(`The browser compiler requires Node.js 24 or newer; found ${process.version}.`);
  process.exit(2);
}

const [, , rustcPath, sysroot, work, separator, ...args] = process.argv;
if (separator !== "--") {
  console.error("usage: node rustc.mjs <rustc.wasm> <sysroot> <work> -- <rustc args...>");
  process.exit(2);
}
process.removeAllListeners("warning"); // node:wasi is "experimental"
const wasi = new WASI({
  version: "preview1",
  args: ["rustc", ...args],
  env: { CLIF2WASM_OBJECT: "1" },
  preopens: { "/sysroot": sysroot, "/work": work, "/tmp": await mkdtemp(join(tmpdir(), "bb-rustc-")) },
  returnOnExit: true,
});
const module = await WebAssembly.compile(await readFile(rustcPath));
const instance = await WebAssembly.instantiate(module, { wasi_snapshot_preview1: wasi.wasiImport });
process.exit(wasi.start(instance));
