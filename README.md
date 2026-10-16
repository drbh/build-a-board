# build-a-board (`bb`)

`bb` turns a **board program** into a manufacturable KiCad board. You
describe the board in Rust with [`bb-sdk`](sdk), and `bb` handles the rest:
it builds your crate to a WebAssembly component, runs it in a sandbox,
writes the `.kicad_pcb`, routes it with FreeRouting, checks it, renders it,
and builds the JLCPCB release. The board lives in its own crate, outside
`bb`'s source.

```
my-board/
├── Cargo.toml            # a cdylib depending on bb-sdk
├── src/lib.rs            # the design (plus any modules, e.g. src/parts.rs)
├── bom/approved-parts.csv
├── logo.png              # inputs the design names
└── output/               # everything bb generates, including the KiCad
                          # project (<name>.kicad_pcb/.kicad_pro/.kicad_sch)
```

A board directory holds only inputs. If your repository commits the routed
board for review (as `bb lock-board` expects), set
`Project::new(..).kicad_dir(".")` to keep the KiCad project beside the sources
instead.

```sh
bb --board my-board                 # build: generate → route → schematic → render, skipping what's up to date
bb --board my-board --no-route      # stop at the unrouted board (fast iteration)
bb --board my-board --quick         # render only top and bottom, not the perspective sheets and GLB
bb --board my-board --no-schematic  # skip the connectivity schematic
bb --board my-board --no-render     # skip images
bb --board my-board --force         # redo every enabled stage
bb --board my-board schematic       # rewrite the schematic and its images unconditionally
bb --board my-board lock-board   # pin the committed, reviewed revision
bb --board my-board check        # lock, BOM, connectivity, DRC, ERC, CPL, gates
bb --board my-board prep         # full release: fab, assembly, renders, zips
bb --board my-board clean        # remove generated output, keeping the KiCad project
bb --board my-board clean --all  # …the KiCad project too (git-tracked files are always kept)
```

`--board` defaults to the current directory (or `BB_BOARD`).

`build` is idempotent. Each stage records what it was built from in
`<name>.bb.json` beside the KiCad project and reruns only when that changes:
generation when the board component, artwork, placed footprints, `bb`, or KiCad
change; routing (slow and not deterministic) when the unrouted board
changes; the schematic when the board's connectivity changes (an existing
schematic that already matches is kept); renders when the board or
schematic changes. Renders cover the top and bottom wiring views, nine
perspective views with a 3x3 sheet, a three-view sheet, a GLB model, and the
schematic as SVG and PNG.

A committed board that `bb` has no record of (for example a reviewed,
routed board in a repository using `kicad_dir(".")`) is never rebuilt
implicitly; `bb` asks for `--force` instead of discarding its routing.

## Board programs

A board crate is a `cdylib` that depends on `bb-sdk` and has an empty
`[workspace]` table, so it builds on its own:

```toml
[package]
name = "my-board"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
bb-sdk = { path = "../build-a-board/sdk" }

[workspace]
```

`bb` runs `cargo build --release --target wasm32-wasip2` whenever the board
is built. Cargo's incremental build makes an edit take well under a second.
A directory may also hold a prebuilt `board.wasm`. Any language that
targets the `bb:board` [WIT world](wit/board.wit) works, but the SDK is
Rust.

The design is declarative: `build()` returns a single `Board` value, and
`bb` resolves it as a whole, so declaration order never matters. The SDK
leans on the type system so that most mistakes are compile errors:

- **Nets are constants.** `nets! { VIN = "VIN", ... }` declares them once;
  every builder takes a `Net`, so a misspelt net doesn't compile. Generated
  names come from `Net::named(format!("D{i}"))`, a block's private nets from
  `Scope::new("buck").net("SW")` (the net `buck.SW`), and `NC` wires a pin
  to nothing. The board's nets are whatever its parts and copper use.
- **Parts are types with named pins.** `part!` declares a struct with one
  `Net` field per pin, so wiring a part means writing a struct literal that
  names every pin. A pin may span several pads (`vbus: ["A4", "A9"]`). With
  `footprint = ..., value = ...` the part is a `Component`; otherwise
  implement `Component::footprint` yourself.
- **Placing a part returns a handle.** Routes name pads through it:
  `u1.sw()` instead of `pin("U1", 8)`, so a missing part or pin can't
  compile; `connector(...)` handles take `.on(net)`. A point near a pad is
  `u1.sw() + (1.5, 0)`: it stays with the pad, and turns with the frame.
- **Values have units.** `resistor(kohm(120), V6, FB).tolerance(1)` prints
  `120k 1%` in the BOM; `capacitor(nf(100), BST, SW).rated(v(50))` prints
  `100n 50V`. Because values are numbers, the board can compute with them.
- **Blocks are structs.** A block's fields are its placed parts (other
  blocks reach them through those fields), and `Module::layout` returns
  its group in its own frame.
- **Design rules are tests.** `cargo test` in the board crate runs on the
  host, e.g. "the servo buck's divider sets 6 V ± 1%".

```rust
use bb_sdk::*;

bb_sdk::nets! { VIN = "VIN", MID = "MID", GND = "GND" }

part! {
    /// A two-pin input header.
    pub struct Input => InputPins { supply: 1, ground: 2 }
    footprint = "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical", value = "IN"
}

/// A block: its parts are fields, its layout is a group in its own frame.
pub struct Divider {
    top: Placed<Resistor>,
    bottom: Placed<Resistor>,
}

impl Divider {
    fn new(top: Net, mid: Net, bottom: Net) -> Self {
        Self {
            top: resistor(kohm(10), top, mid).place("R1", (0, 0)),
            bottom: resistor(kohm(10), mid, bottom).place("R2", (0, 4)).rotate(270),
        }
    }

    pub fn ratio(&self) -> f64 {
        let (top, bottom) = (self.top.part.value.ohms(), self.bottom.part.value.ohms());
        bottom / (top + bottom)
    }
}

impl Module for Divider {
    fn layout(&self) -> Group {
        let mid = self.top.part.nets.p2;
        // The bend sits 1.5 mm right of and 2 mm below R1's pad, wherever the block goes.
        group(items![
            &self.top,
            &self.bottom,
            track(mid, [self.top.p2(), self.top.p2() + (1.5, 2), self.bottom.p1()]).width(0.25),
        ])
    }
}

fn build() -> Board {
    let input = Input { supply: VIN, ground: GND }.place("J1", (5, 10));
    let divider = Divider::new(VIN, MID, GND);
    Board::new(Project::new("my-board").lcsc([("0603WAF1002T5E", "C25804")]))
        .rules(rules(0.25, 0.2))
        .netclasses([netclass("Power", 1.0, [VIN])])
        .severities([("courtyards_overlap", "warning")])
        .hide_references(["J"])
        .finish("Black", "White", "HASL lead-free")
        .items(items![
            outline(Layer::EdgeCuts, [at(0, 0), at(50, 0), at(50, 30), at(0, 30)], 0.1),
            track(VIN, [input.supply(), divider.top.p1()]).width(0.5),
            // Ground drops to the back plane through a via 2 mm below the header pin.
            route(GND).width(0.5).to(input.ground()).to(input.ground() + (0, 2)).via(Layer::BCu),
            divider.at(20, 10), // move the block, its routes follow
            &input,
            zone(GND, Layer::BCu, [at(0.5, 0.5), at(49.5, 0.5), at(49.5, 29.5), at(0.5, 29.5)]),
            input.legends_column(2, [(1, "+"), (2, "-")]),
            artwork("LOGO", "logo.png", (40, 20), 12),
        ])
}

/// Applied to every track after autorouting; `None` keeps its width.
fn route_width(t: &RoutedTrack) -> Option<f64> {
    t.is_on(VIN).then_some(1.0)
}

bb_sdk::export_board!(build, route_width);

#[test]
fn divider_halves_the_input() {
    assert_eq!(Divider::new(VIN, MID, GND).ratio(), 0.5);
}
```

Mistakes surface in three places:

- **The compiler** catches misspelt nets, pins, and parts, wrong value
  types, and unwired pins, as ordinary `rustc` errors at the line.
- **The board itself** can panic with a message, e.g. `.on(net)` on a
  connector with no pad on that net, or `.pad("B9")` on a pin that has no
  such pad; `bb` shows the message and the board-source line.
- **`bb`** checks the resolved netlist and reports **every** problem at
  once: duplicate references, pads that do not exist, net classes naming
  nets the board never uses, and nets that connect fewer than two pads (a
  warning, unless listed in `no_connect`).

| Builder | Returns |
| --- | --- |
| `nets! { NAME = "net", ... }`, `Net::named(name)`, `Scope::new(block).net(name)`, `NC` | Nets: constants, generated, block-private, and the unwired pin |
| `part! { struct Part => PartPins { pin: pad, multi: [pad, pad] } footprint = .., value = .. }` | A part type with named pins; without the clause, add `impl Component` |
| `resistor`, `capacitor` (`kohm`, `nf`, `uf`, `v`, …), `connector(...).numbered()/.pins()` | Built-in components |
| `component.place(ref, at).rotate(deg).hide_reference()` | A `Placed` handle: an item, plus its pins (`.p1()`, `.on(net)`, `.pad(n)`, `.vbus().pad("B9")`) |
| `impl Module { fn layout(&self) -> Group }`, `.at(x, y)` | A block in its own frame |
| `Board::new(project).rules().netclasses().no_connect().severities().hide_references().finish().unlink_after_route().items()` | The board `build()` returns |
| `Project::new(name)` + `.lcsc() .order_quantity() .checks() .release_files() .schematic() .decisions() .kicad_dir() …` | Release settings |
| `group(items).at().rotate().name()` | A frame: contents are laid out relative to it and move with it; groups nest |
| `part(ref, value, footprint)`, `generated(...)` with `smd`, `tht`, `fp_line`, `model` | Raw footprints, for `Component` impls |
| `at(x, y)`, `u1.sw()`, `u1.sw() + (dx, dy)` | Anchors: a point in the current frame, a pad, or a point beside a pad |
| `track(..).keep()/.keep_routed()`, `via`, `zone`, `outline`, `text` | Copper and graphics; kept tracks survive routing at least as wide |
| `route(net).width().via_size().to(a).to(b).via(Layer::BCu).to(c)` | A fixed run that changes layers through vias |
| `placed.legends_row(y, ..)` / `.legends_column(x, ..)` | Silkscreen legends placed from a part's pads |
| `artwork(ref, image, center, width).layer()` | A PNG traced into silkscreen |
| `rules(track_width, clearance).via(d, drill)`, `netclass(name, width, nets)` | Board-wide rules (KiCad's Default class, and what the router works to) and per-net classes |
| `Board::router_clearance(mm)` | The clearance FreeRouting routes with (default 0.7; fine-pitch boards need less) |
| `rotate(center, p, deg)`, `polar(center, r, deg)` | Geometry helpers, counter-clockwise like footprint rotation |

`items![...]` accepts parts, handles (`&placed`), groups, vectors, arrays,
and `Option`s, so conditional and generated content composes naturally.
[`examples/buck-converter`](examples/buck-converter) is the smallest
complete board: one parametric block, `Buck::new(v(5))`, that picks its
feedback resistor from the E96 series for the requested output, a pair of
screw terminals, and design tests (`cargo test` there) that check the
output is within 1% for common rails and that capacitors are rated with
margin. Change `OUTPUT` and run `bb --board examples/buck-converter`.

[`examples/led-ring`](examples/led-ring) (twelve WS2812Bs placed in a
loop, with a data chain) and [`examples/usb-uart`](examples/usb-uart)
(USB-C receptacle, ESD protection, CH340C, and a hand-routed USB-C fan-out)
cover generated layout and fine-pitch connectors.
[`examples/FRICTION.md`](examples/FRICTION.md) records what was awkward to
express while writing them.

[`examples/balance-bot`](examples/balance-bot) is a complete board:
`nets.rs` (the netlist, and per-wheel net sets), `parts.rs` (part types
such as the AP64352 and the SimpleFOC Mini carrier), `power.rs` and
`control.rs` (blocks), `routes.rs` (fixed routes between blocks, written
through the blocks' handles), and `lib.rs` (the board, plus design tests).

### The sandbox

The component runs in Wasmtime with no filesystem, network, or environment
access. Its clock is frozen, its randomness is seeded, NaNs are
canonicalized, and execution is fuel-limited. The same component therefore
always produces the same design, which is what lets `bb` hash the
`.wasm` as the board's input. The only way information leaves the
component is through the `Design` it returns. Images and parts lists are
read by `bb` itself, from the paths the design names.

## How it works

Everything that used to need KiCad's Python (`pcbnew`) is native Rust:

- **Board writer:** stock footprints are read from KiCad's libraries and
  placed. Geometry is held in integer nanometres and follows pcbnew's own
  rounding rules. KiCad (`kicad-cli`) fills the zones and saves the board,
  and `bb` then canonicalizes item order and UUIDs so the output is
  reproducible.
- **Artwork tracing:** a pure-Rust port of the OpenCV pipeline that matches
  OpenCV 4.8.1 exactly. Polygons with holes go through KiCad's own
  simplify-and-fracture step, using the vendored Clipper2 1.3.0 that KiCad
  ships.
- **Specctra:** DSN export and SES import for FreeRouting, byte-identical
  to pcbnew's.
- **Schematic:** the connectivity schematic is written directly.
- **Board programs:** components are built with cargo and run with
  Wasmtime (Cranelift, component model, WASI preview 2).

External tools: `cargo` with the `wasm32-wasip2` target
(`rustup target add wasm32-wasip2`), `kicad-cli` (fill, DRC/ERC, Gerbers, renders), Java with
FreeRouting, and ImageMagick. `bb doctor` checks them. Overrides:
`KICAD_CLI`, `KICAD_FOOTPRINT_DIR`, `JAVA`, `FREEROUTING_JAR`, `MAGICK`,
`BB_PYTHON` (for a project's own check scripts).

## Nix

`nix develop` provides the Rust toolchain (with the `wasm32-wasip2`
target), JDK 21, ImageMagick, Python, and
the exact FreeRouting 2.0.1 jar, and sets the variables above. `nix build`
builds `bb`. KiCad comes from nixpkgs on Linux. nixpkgs marks KiCad broken
on macOS, so there the shell uses `/Applications/KiCad`.

## Fidelity to the original Python flow

Measured against the Balance Bot devboard's original `bb`/`board.py`, with
the board described by [`examples/balance-bot`](examples/balance-bot):

- **`generate`:** the board and project are byte-identical to the Python
  pipeline once the zones are filled the same way. KiCad's fill depends on
  the order items appear in the file, and pcbnew saved them in an
  arbitrary order, so `bb` fills from canonical order. That makes the
  output reproducible.
- **Release artifacts:** BOMs, CPL, shopping list, netlist, lock file, and
  zips are byte-identical. Files written by `kicad-cli` differ only in the
  timestamps KiCad embeds.
- **KiCad quirk kept on purpose:** pcbnew saves only the first outline of a
  silkscreen polygon. One traced logo shape splits in two when fractured,
  and its second piece is dropped exactly as it was before.
