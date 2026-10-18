# playground

edit a board on the left, see it drawn on the right, with nothing installed but a browser. the board's Rust is compiled in the tab, about half a second after you stop typing, and drawn with the same code as `bb preview`.

```bash
cd playground
npm install
npm run assets     # once, and after changing the SDK, wit/, or bb-core
npm run dev        # http://127.0.0.1:7820/
```

`./build.sh` does all of it and writes a static build to `dist/`; `npm run preview` serves that.

it needs Node 24 or newer, `zstd`, `wasm-pack` (with the `wasm32-unknown-unknown` target), and Cargo. Node 22 can crash while running the WebAssembly compiler used to prebuild the SDK. the dev and preview servers also serve KiCad's footprints under `/footprints/`, found in the usual places or `KICAD_FOOTPRINT_DIR`; a static host has to do the same. Chrome and Firefox work; Safari is not a target.

the address bar is the share link: it names the example, and once you edit it carries the source too, deflated into `?src=`. **share** copies it. ⌘⏎ rebuilds right away; drag the bar between the panes to resize them.

new visitors start on the **tour**: the indicator board built up in seven steps, from an outline to the finished [`examples/indicator`](../examples/indicator), each with a note on what it adds. every step is a complete board in [`tour/`](tour), `<n>-<name>.rs`, whose `//!` header gives its title and note; `?example=tour&step=4` links to one. keep `7-values.rs` in step with `examples/indicator/src/lib.rs` when that changes. what each edit or step changed is drawn only when the **Changes** layer is on.

## hosting

[bb.dholtz.com](https://bb.dholtz.com) serves the playground at `/` and the book at `/book/` from a Cloudflare Worker, redeployed by CI on every green push to `main`. `make site` builds both and stages them in `cloud/`; `make deploy` publishes. the two compiler files over Cloudflare's 25 MiB static-asset limit live in R2, under their sha256; everything else, KiCad's footprints included, is a static asset. see [`cloud/wrangler.toml`](cloud/wrangler.toml).

## limits

- single-file boards only: every example whose `lib.rs` has no `mod name;` is offered (indicator, buck-converter, led-ring, usb-uart). balance-bot needs modules the playground does not have yet
- no routing, zone fills, renders, or artwork images; those stay in `bb`
- the first visit downloads about 160 MB (the compiler and its standard library), which the browser then caches
- the compiler is a young, single-maintainer project; a few rare operations (128-bit integer arithmetic) are not implemented by its WebAssembly backend

## how it works

1. **compile.** a Web Worker runs `rustc` compiled to WebAssembly, from [weblings](https://github.com/AngelOnFira/weblings) and pinned by hash in `tools/fetch-compiler.sh`. it compiles the board as a `wasm32-wasip1` module against `bb-sdk`, which `tools/assets.sh` prebuilds with that same compiler. (the SDK uses no procedural macros, which a compiler running as WebAssembly cannot load; see `sdk/bindgen`)
2. **run.** the worker calls the board's `bb-design-json` export (added by `export_board!` on `wasip1` only), which returns the design as JSON
3. **draw.** `bb-core`, built for the browser by `core-wasm/`, reads the design, fetches the footprints it places, resolves it, and draws the SVG, highlighting what changed since the last drawing

compiler errors appear inline in the editor and in the problems list; design problems and panics appear there too, while the last good drawing stays on screen, dimmed.

## layout

the app is [Svelte 5](https://svelte.dev) built with [Vite](https://vite.dev), in TypeScript (`npm run check` type-checks it).

| | |
|---|---|
| `src/App.svelte` | the page: example picker, status, layers, editor, drawing, problems |
| `src/components/` | `Editor` (CodeMirror), `BoardView` (pan, zoom, net highlight), `Layers`, `Problems` |
| `src/lib/session.svelte.ts` | one editing session: when to compile, the worker's replies, drawing with bb-core |
| `src/lib/compiler.worker.ts` | the Web Worker that runs rustc and then the board |
| `src/lib/protocol.ts` | the messages between the page and the worker |
| `src/lib/examples.ts` | the example boards, read from `../examples` at build time |
| `src/lib/drawing.css` | styles for bb-core's SVG (the classes `bb preview` uses) |
| `tools/assets.sh` | fetches the compiler, prebuilds bb-sdk with it, builds `core-wasm/`; everything lands under `.cache/` |
| `vite.config.ts` | the footprint and self-test routes, and where the prepared assets live |

`?selftest` runs a scripted session (draw, move a part, a typo, the fix) and posts its results to the dev or preview server, which appends them to `.cache/selftest.jsonl`.
