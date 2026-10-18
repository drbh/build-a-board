# under the hood

what a board program is to `bb`, and the pieces that matter when you change `bb` itself. [back to the book](../README.md)

## the component

a board crate compiles to a WebAssembly component for the `bb:board` world in [wit/board.wit](../../wit/board.wit): one export, `build`, that returns the design, and one, `route-width`, that `bb` asks about each routed track. the SDK is Rust, but any language that targets that world works; a directory holding a prebuilt `board.wasm` instead of a `Cargo.toml` is also a board.

`bb` runs the component in Wasmtime with no filesystem, network, or environment access, a frozen clock, seeded randomness, and a fuel limit. the same component therefore always produces the same design, which is what lets `bb` hash the `.wasm` as the board's input and skip stages that are up to date. the only way out of the component is the design it returns; images and parts lists are read by `bb` from the paths the design names.

## the bindings

the SDK's bindings for `wit/` are generated ahead of time into `sdk/src/bindings.rs`, so board crates compile without procedural macros: a cold board build takes about a second, and the SDK compiles under a compiler running in the browser. after editing anything in `wit/`:

```bash
cargo run -p bb-sdk-bindgen
```

the generated file records a hash of `wit/`, and the SDK's build script refuses to build when it no longer matches, so forgetting shows up as a failed board build with that command in the message. `cargo test -p bb-sdk-bindgen` checks the committed file against a fresh generation.

## bb-core

[core/](../../core) is the part of `bb` that needs nothing but the design and the footprint library: resolving frames and anchors, reading footprints, and drawing the preview SVG. `bb preview` and the [playground](../../playground) both draw with it, so the two pictures are identical.

## what is native

nothing in `bb` calls KiCad's Python. the board writer reads stock footprints from KiCad's libraries and writes the `.kicad_pcb` itself, in integer nanometres with pcbnew's rounding; `kicad-cli` fills the zones and saves, and `bb` canonicalizes item order and UUIDs so the file is reproducible. the Specctra DSN and SES for FreeRouting, the connectivity schematic, and the artwork tracing (a port of the OpenCV contour pipeline, with KiCad's vendored Clipper2 for polygons with holes) are Rust too.
