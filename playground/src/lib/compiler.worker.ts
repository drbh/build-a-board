// The compiler engine, off the main thread: rustc.wasm compiles a board
// against the prebuilt bb-sdk, then the board runs and hands over its design
// as JSON through its `bb-design-json` export.
import { ConsoleStdout, Directory, type Fd, File, type Inode, OpenFile, PreopenDirectory, WASI } from "@bjorn3/browser_wasi_shim";
import type { Compiled, Problem, Reply, Request } from "./protocol";

let rustc: WebAssembly.Module;
let sysroot: Map<string, File>;
let deps: Map<string, Inode>;

const reply = (message: Reply) => postMessage(message);
const local = (text: string) => text.replaceAll("/work/board.rs", "board.rs");

/// The sysroot bundle: "RIWB1\n", a u32le index length, a JSON index
/// {files: [{p, o, l}]}, then the files' bytes.
function parseBundle(bytes: Uint8Array): Map<string, File> {
  const indexLength = new DataView(bytes.buffer, bytes.byteOffset + 6, 4).getUint32(0, true);
  const index: { files: { p: string; o: number; l: number }[] } = JSON.parse(
    new TextDecoder().decode(bytes.subarray(10, 10 + indexLength)),
  );
  const base = 10 + indexLength;
  const files = new Map<string, File>();
  for (const f of index.files) {
    if (f.p !== "manifest.json") files.set(f.p, new File(bytes.slice(base + f.o, base + f.o + f.l)));
  }
  return files;
}

/// The sysroot as a directory tree. Built fresh for each compile: the shim's
/// directories are mutable, and rustc must not see the last run's leftovers.
function sysrootDir(): PreopenDirectory {
  type Tree = Map<string, Tree | File>;
  const root: Tree = new Map();
  for (const [path, file] of sysroot) {
    const segments = path.split("/");
    const name = segments.pop()!;
    let dir = root;
    for (const s of segments) {
      if (!dir.has(s)) dir.set(s, new Map());
      dir = dir.get(s) as Tree;
    }
    dir.set(name, file);
  }
  const entries = (tree: Tree): Map<string, Inode> =>
    new Map<string, Inode>([...tree].map(([n, v]) => [n, v instanceof Map ? new Directory(entries(v)) : v]));
  return new PreopenDirectory("/sysroot", entries(root));
}

async function init(base: string) {
  const fetchBytes = async (name: string) => {
    const response = await fetch(new URL(name, base));
    if (!response.ok) throw new Error(`${name}: ${response.status}`);
    return new Uint8Array(await response.arrayBuffer());
  };
  const [module, bundle, sdk, bindgen] = await Promise.all([
    WebAssembly.compileStreaming(fetch(new URL("rustc.wasm", base))),
    fetchBytes("sysroot-wasip1.bundle"),
    fetchBytes("libbb_sdk.rlib"),
    fetchBytes("libwit_bindgen.rlib"),
  ]);
  rustc = module;
  sysroot = parseBundle(bundle);
  deps = new Map([
    ["libbb_sdk.rlib", new File(sdk)],
    ["libwit_bindgen.rlib", new File(bindgen)],
  ]);
}

/// rustc's JSON diagnostics (one per line), and anything else it printed.
function diagnostics(log: string): { diagnostics: Problem[]; residue: string } {
  const found: Problem[] = [];
  let residue = "";
  for (const line of log.split("\n")) {
    if (!line.startsWith("{")) {
      if (line && !line.startsWith("riwl-timing")) residue += line + "\n";
      continue;
    }
    try {
      const d = JSON.parse(line);
      if (d.level !== "error" && d.level !== "warning") continue;
      const spans: any[] = d.spans ?? [];
      const span = spans.find((s) => s.is_primary) ?? spans[0];
      found.push({
        level: d.level,
        message: d.message,
        rendered: local(d.rendered ?? d.message).trim(),
        span: span && { line: span.line_start, col: span.column_start, endLine: span.line_end, endCol: span.column_end },
      });
    } catch {
      residue += line + "\n";
    }
  }
  return { diagnostics: found, residue: residue.trim() };
}

/// A console that collects what is written to it.
function capture(): { fd: Fd; text: () => string } {
  let text = "";
  const decoder = new TextDecoder();
  return { fd: new ConsoleStdout((data) => (text += decoder.decode(data, { stream: true }))), text: () => text };
}

/// An empty stdin, or a stdout nobody reads.
const nothing = (): Fd => new OpenFile(new File([]));

async function compile(source: string) {
  const out = capture();
  const work = new PreopenDirectory(
    "/work",
    new Map<string, Inode>([
      ["board.rs", new File(new TextEncoder().encode(source))],
      ["deps", new Directory(deps)],
    ]),
  );
  // the same flags tools/assets.sh builds the SDK with
  const args = ["rustc", "/work/board.rs", "--crate-name", "board", "--crate-type", "cdylib", "--edition", "2024",
    "--sysroot", "/sysroot", "-Zunstable-options", "--target", "wasm32-wasip1", "-O", "-Cpanic=abort",
    "--extern", "bb_sdk=/work/deps/libbb_sdk.rlib", "-L", "dependency=/work/deps",
    "--error-format=json", "-o", "/work/board.wasm"];
  const fds = [nothing(), out.fd, out.fd, new PreopenDirectory("/tmp", new Map()), sysrootDir(), work];
  const wasi = new WASI(args, ["CLIF2WASM_OBJECT=1"], fds);
  const instance = await WebAssembly.instantiate(rustc, { wasi_snapshot_preview1: wasi.wasiImport });
  let trap = "";
  try {
    wasi.start(instance as any);
  } catch (e: any) {
    trap = String(e?.message ?? e);
  }
  const wasm = (work.dir.contents.get("board.wasm") as File | undefined)?.data;
  return { wasm, ...diagnostics(out.text() + trap) };
}

/// Run the board and read its design. A panic surfaces as its message.
async function design(wasm: Uint8Array): Promise<{ json: string } | { panic: string }> {
  const stderr = capture();
  const wasi = new WASI(["board"], [], [nothing(), nothing(), stderr.fd]);
  let exports: any;
  // The linker leaves wit-bindgen's allocator as an import; forward it to the versioned export.
  const env = { cabi_realloc: (...a: number[]) => exports.cabi_realloc_wit_bindgen_0_51_0(...a) };
  const { instance } = await WebAssembly.instantiate(wasm as BufferSource, {
    wasi_snapshot_preview1: wasi.wasiImport,
    env,
  });
  exports = instance.exports;
  wasi.initialize(instance as any);
  try {
    const packed: bigint = exports["bb-design-json"]();
    const pointer = Number(packed >> 32n);
    const length = Number(packed & 0xffffffffn);
    return { json: new TextDecoder().decode(new Uint8Array(exports.memory.buffer, pointer, length)) };
  } catch (e: any) {
    return { panic: local(stderr.text().trim() || String(e?.message ?? e)) };
  }
}

onmessage = async ({ data }: MessageEvent<Request>) => {
  if (data.type === "init") {
    const t0 = performance.now();
    try {
      await init(data.base);
      reply({ type: "ready", ms: performance.now() - t0 });
    } catch (e: any) {
      reply({ type: "compiled", id: 0, engineError: `loading the compiler: ${e?.stack ?? e}` });
    }
    return;
  }
  try {
    const t0 = performance.now();
    const { wasm, ...compiled } = await compile(data.source);
    const compileMs = performance.now() - t0;
    const result: Compiled = { type: "compiled", id: data.id, compileMs, ...compiled };
    reply(wasm?.length ? { ...result, residue: undefined, ...(await design(wasm)) } : result);
  } catch (e: any) {
    reply({ type: "compiled", id: data.id, engineError: String(e?.stack ?? e) });
  }
};
