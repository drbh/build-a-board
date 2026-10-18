# build-a-board

`bb` turns a small Rust program into a manufacturable KiCad board: it writes the `.kicad_pcb`, routes it, checks it, renders it, and packs the files JLCPCB asks for.

[the book](book/README.md) walks through a board from the first net to the release zip. try one in the browser at [bb.dholtz.com](https://bb.dholtz.com) with nothing installed; the book is there too, at [bb.dholtz.com/book](https://bb.dholtz.com/book/).

## install

with Rust installed through rustup:

```bash
git clone https://github.com/drbh/build-a-board.git
cargo install --path build-a-board
rustup target add wasm32-wasip2
bb doctor
```

`bb doctor` lists the tools `bb` calls and where it found them: `kicad-cli` and KiCad's footprint library, Java with the [FreeRouting](https://github.com/freerouting/freerouting) jar, and ImageMagick. set `KICAD_CLI`, `KICAD_FOOTPRINT_DIR`, `JAVA`, `FREEROUTING_JAR` or `MAGICK` to point at another install.

with nix, `nix develop` provides everything but KiCad on macOS, and `nix build` builds `bb`.

## a board

a board is a crate that depends on [`bb-sdk`](sdk) and returns one `Board` value. the smallest one, [examples/indicator](examples/indicator), is four LEDs on a header:

```bash
bb --board examples/indicator preview --watch --open   # draw it, redraw on every save
bb --board examples/indicator                          # generate, route, schematic, render
```

`--board` defaults to the current directory. the KiCad project and everything else `bb` makes land in `output/`.

read [a board, from nothing](book/01-a-board.md) to write your own, and the [reference](book/README.md#reference) for every command and builder.

## develop

```bash
cargo build --release          # target/release/bb
cargo test                     # builds the fixture boards for wasm32-wasip2
```

after editing anything in [`wit/`](wit), run `cargo run -p bb-sdk-bindgen` to regenerate the SDK's bindings; every board build fails with that message until you do.
