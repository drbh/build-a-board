# build-a-board (`bb`)

`bb` turns a **board file** into a manufacturable KiCad board. You describe
the board in [Starlark](https://github.com/bazelbuild/starlark) (Python
syntax) with `bb`'s primitives, and `bb` handles the rest: it writes the
`.kicad_pcb`, routes it with FreeRouting, checks it, renders it, and builds
the JLCPCB release. Board files are read at run time, so iterating on a
board needs no recompile.

```
my-board/
├── board.star            # the design
├── parts.star            # optional shared components (load("parts.star", ...))
├── bom/approved-parts.csv
├── logo.png              # inputs the board file names
└── output/               # everything bb generates, including the KiCad
                          # project (<name>.kicad_pcb/.kicad_pro/.kicad_sch)
```

A board directory holds only inputs. If your repository commits the routed
board for review (as `bb lock-board` expects), set
`project(..., kicad_dir = ".")` to keep the KiCad project beside the sources
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
generation when the board files, artwork, placed footprints, `bb`, or KiCad
change; routing (slow and not deterministic) when the unrouted board
changes; the schematic when the board's connectivity changes (an existing
schematic that already matches is kept); renders when the board or
schematic changes. Renders cover the top and bottom wiring views, nine
perspective views with a 3x3 sheet, a three-view sheet, a GLB model, and the
schematic as SVG and PNG.

A committed board that `bb` has no record of (for example a reviewed,
routed board in a repository using `kicad_dir = "."`) is never rebuilt
implicitly; `bb` asks for `--force` instead of discarding its routing.

## Board files

Coordinates are millimetres in KiCad's frame (Y down); angles are degrees.

```python
load("parts.star", "two_pin")

project("my-board", lcsc = {"AP64352SP-13": "C2071665"}, order_quantity = 5)
board(hide_references = ["J", "TP"], finish = {"mask": "Black"})
netclass("Power", track_width = 1.0, nets = ["VIN"], via_diameter = 1.0, via_drill = 0.5)
outline("Edge.Cuts", [(0, 0), (50, 0), (50, 30), (0, 30)], width = 0.1)

part("R1", "10k", "Resistor_SMD:R_0603_1608Metric", at = (10, 10), angle = 90,
     pins = {"1": "VIN", "2": "GND"})
generated("L1", "10uH", "MyLib:L_8x8", at = (20, 10),
          pads = [smd(1, (-3.7, 0), (2.5, 3.8), net = "VIN"), smd(2, (3.7, 0), (2.5, 3.8), net = "5V")],
          body = (9.2, 8.0))

track("VIN", [pin("R1", 1), (15, 8), pin("L1", 1)], width = 0.5)   # pin() lands exactly on a pad
via("GND", (12, 12), diameter = 0.6, drill = 0.3)
zone("GND", "B.Cu", [(0.5, 0.5), (49.5, 0.5), (49.5, 29.5), (0.5, 29.5)])
text("HELLO", (25, 2), size = 1.0)
pin_legends("J1", {"1": "GND", "2": "VIN"}, row = 5)           # one legend per pad, from the pad
artwork("LOGO", "logo.png", center = (40, 20), width = 12, layer = "F.SilkS")

def route_width(track):            # optional: widen routed tracks after FreeRouting
    return 1.0 if track.net == "VIN" else None
```

| Primitive | Purpose |
| --- | --- |
| `project(name, ...)` | KiCad project name and release settings: BOM path, LCSC map, gates, release files, schematic title block, review decisions |
| `board(copper_layers, hide_references, finish, unlink_after_route)` | Board-wide settings |
| `nets`, `netclass`, `rule_severity` | Nets, routing classes, `.kicad_pro` DRC severities |
| `part` | A stock KiCad library footprint (`models=[...]` replaces 3D models, `swap_model=` changes the file) |
| `generated` + `smd`, `tht`, `fp_line`, `model` | A footprint built from pads and graphics |
| `track`, `via`, `zone`, `line`, `outline`, `text` | Copper and graphics |
| `pin(ref, pad)` | An exact pad-centre anchor, accepted anywhere a point is |
| `pin_legends` | Silkscreen legends positioned from a footprint's pads |
| `rotate(center, point, angle)` | Place points in a rotated local frame |
| `artwork` | Trace a PNG into silkscreen polygons |
| `keep_track`, `def route_width(track)` | Post-route rules |

[`examples/balance-bot`](examples/balance-bot) is a complete board. Its
`parts.star` holds the reusable components (the AP64352 buck IC and its
CYA0850 inductor, the SimpleFOC Mini carrier, JLC V-chip bulk caps, the
blade-fuse holder, thermal-via fields, labelled blocks); `board.star`
composes them and anchors 136 routes to pads with `pin()`.

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

External tools: `kicad-cli` (fill, DRC/ERC, Gerbers, renders), Java with
FreeRouting, and ImageMagick. `bb doctor` checks them. Overrides:
`KICAD_CLI`, `KICAD_FOOTPRINT_DIR`, `JAVA`, `FREEROUTING_JAR`, `MAGICK`,
`BB_PYTHON` (for a project's own check scripts).

## Nix

`nix develop` provides the Rust toolchain, JDK 21, ImageMagick, Python, and
the exact FreeRouting 2.0.1 jar, and sets the variables above. `nix build`
builds `bb`. KiCad comes from nixpkgs on Linux. nixpkgs marks KiCad broken
on macOS, so there the shell uses `/Applications/KiCad`.

## Fidelity to the original Python flow

Measured against the Balance Bot devboard's original `bb`/`board.py`:

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
