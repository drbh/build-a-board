# bb playground

Edit a board on the left, see it drawn on the right, with nothing installed
but a browser. The board's Rust is compiled **in the tab**, about half a
second after you stop typing, and drawn with the same code as `bb preview`
(the SVG is byte-identical).

```sh
cd playground
npm install
npm run assets     # once, and after changing the SDK, wit/, or bb-core
npm run dev        # http://127.0.0.1:7820/, reloading as you edit the app
```

`./build.sh` does all of it and writes a static build to `dist/`;
`npm run preview` serves that.

Requirements: Node, `zstd`, `wasm-pack` (with the `wasm32-unknown-unknown`
Rust target), and Cargo. The dev and preview servers also serve KiCad's
footprint libraries under `/footprints/` (found in the usual places, or set
`KICAD_FOOTPRINT_DIR`); a static host needs to do the same. Chrome and
Firefox are supported; Safari is not a target.

## Layout

The app is [Svelte 5](https://svelte.dev) built with [Vite](https://vite.dev),
in TypeScript (`npm run check` type-checks it):

| | |
|---|---|
| `src/App.svelte` | the page: example picker, status, layers, editor, drawing, problems |
| `src/components/` | `Editor` (CodeMirror), `BoardView` (pan, zoom, net highlight), `Layers`, `Problems` |
| `src/lib/session.svelte.ts` | one editing session: when to compile, the worker's replies, drawing with bb-core |
| `src/lib/compiler.worker.ts` | the Web Worker that runs rustc and then the board |
| `src/lib/protocol.ts` | the messages between the page and the worker |
| `src/lib/examples.ts` | the example boards, read from `../examples` at build time |
| `src/lib/drawing.css` | styles for bb-core's SVG (the classes `bb preview` uses) |
| `tools/assets.sh` | fetches the compiler, prebuilds bb-sdk with it, builds `core-wasm/` |
| `vite.config.ts` | the footprint and self-test routes, and where the prepared assets live |

`tools/assets.sh` writes everything generated under `.cache/`: the
compiler and prebuilt SDK in `.cache/public/compiler/` (served as-is) and
the wasm-pack build of bb-core in `.cache/core/` (imported as `$core`).

## How it works

1. **Compile.** A Web Worker runs `rustc` compiled to WebAssembly (with the
   Cranelift backend) from [weblings](https://github.com/AngelOnFira/weblings),
   pinned by hash in `tools/fetch-compiler.sh`. It compiles the board as a
   `wasm32-wasip1` module against `bb-sdk` and `wit-bindgen`, which
   `tools/assets.sh` prebuilds with that same compiler. (The SDK uses no procedural
   macros, which a compiler running as WebAssembly cannot load; see
   `sdk/bindgen`.)
2. **Run.** The worker instantiates the board and calls its
   `bb-design-json` export (added by `export_board!` on `wasip1` only), which
   returns the design as JSON in serde's default shape for the WIT types.
3. **Draw.** `bb-core`, built for the browser by `core-wasm/` (wasm-pack),
   reads the design, fetches the KiCad footprints it places, resolves it, and
   draws the preview SVG, highlighting what changed since the last drawing.

Compiler errors appear inline in the editor and in the problems list; design
problems (an unknown pad, a missing footprint) and panics appear there too,
while the last good drawing stays on screen, dimmed.

## Limits

- Single-file boards only: every example whose `lib.rs` has no `mod name;`
  is offered (today indicator, buck-converter, led-ring, and usb-uart);
  multi-file crates such as balance-bot need modules the playground does
  not have yet.
- No routing, zone fills, renders, or artwork images: those stay in `bb`.
- The first visit downloads about 160 MB (the compiler and its standard
  library), which the browser then caches.
- The compiler is a young, single-maintainer project; some rare operations
  (128-bit integer arithmetic) are not implemented by its WebAssembly
  backend.

`?selftest` runs a scripted session (draw, move a part, a typo, the fix) and
posts its results to the dev or preview server, which appends them to
`.cache/selftest.jsonl`.
