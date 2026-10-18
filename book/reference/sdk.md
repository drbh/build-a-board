# the sdk

every builder a board is written with, by what it makes. `use bb_sdk::*` brings them all in. lengths are millimetres in KiCad's frame (Y down); angles are degrees, counter-clockwise, as KiCad rotates footprints. [back to the book](../README.md)

## nets

| builder | makes |
| --- | --- |
| `nets! { VIN = "VIN", GND = "GND" }` | one `Net` constant per line |
| `Net::named(format!("D{i}"))` | a net named at run time; equal names are the same net |
| `Scope::new("buck").net("SW")` | the net `buck.SW`, so a block instanced twice has its own internal nets |
| `NC` | the net of a pin left unwired on purpose |

a board's nets are whatever its parts and copper use; there is no list to maintain.

## parts

| builder | makes |
| --- | --- |
| `part! { struct P => PPins { a: 1, b: ["A4", "B4"] } footprint = .., value = .. }` | a part type with one `Net` field per pin, that is a `Component` |
| `part! { struct P => PPins { .. } }` + `impl Component for P { type Pins = PPins; fn footprint(&self, reference) -> Footprint }` | a part whose footprint or value depends on its fields |
| `resistor(kohm(10), a, b).tolerance(1).footprint(R0603)` | a resistor; prints `10k 1%` in the BOM |
| `capacitor(nf(100), a, b).rated(v(50)).footprint(C0603)` | a capacitor; prints `100n 50V` |
| `connector(value, footprint).numbered([a, b, c])` or `.pins([(1, a), (3, c)])` | a library connector, wired by pad number |
| `ohm`, `kohm`, `mohm`, `pf`, `nf`, `uf`, `v` | values with units; `.ohms()` and `.farads()` read them back as `f64` |
| `R0603`, `C0603`, `C0805`, `C1206` | footprint names for the common chip sizes |

for a `Component::footprint` you write yourself:

| builder | makes |
| --- | --- |
| `part(reference, value, footprint).pins(pads).models([..]).swap_model(path).assembly_rotation(deg)` | a library footprint with its pads wired |
| `generated(reference, value, footprint).pads([smd(..), tht(..)]).body(w, h).graphics([fp_line(..)]).models([..])` | a footprint drawn from scratch |
| `smd(number, at, size)`, `tht(number, at, size, drill)`, `.net(n)`, `.shape(PadShape::..)` | pads |
| `fp_line(layer, start, end, width)`, `model(path).offset(x, y, z)` | footprint graphics and 3D models |

## placing

| builder | makes |
| --- | --- |
| `component.place("U1", (x, y)).rotate(deg).hide_reference()` | a `Placed<C>`: an item, plus a handle to its pins |
| `u1.sw()`, `j1.vbus().pad("B9")`, `u1.pad(8)` | the centre of a pad, as an anchor |
| `header.on(GND)` | a connector's one pad on that net |
| `u1.sw() + (1.5, 0)` | a point beside a pad, that stays with it and turns with its frame |
| `at(x, y)` | a point in the current frame |
| `u1.part` | the part the handle was placed from, for its values and nets |
| `placed.legends_row(y, [(1, "+"), (2, "-")])`, `.legends_column(x, ..)`, `.size(mm)`, `.rotate(deg)` | silkscreen labels placed from a part's pads |

a `Placed` goes into `items![..]` by reference, `&u1`, so the handle stays usable.

## copper and graphics

| builder | makes |
| --- | --- |
| `track(net, [a, b, c]).width(mm).layer(Layer::BCu)` | a track through anchors |
| `track(..).keep()` / `.keep_routed()` | a track the router keeps, at least as wide; `keep_routed` for a segment the router draws |
| `via(net, at).size(diameter, drill)` | a via |
| `route(net).width(mm).via_size(d, drill).layer(l).to(a).to(b).via(Layer::BCu).to(c)` | a run of tracks that changes layers through vias |
| `zone(net, layer, outline).priority(n).clearance(mm)` | a filled zone |
| `outline(Layer::EdgeCuts, points, width).open()` | the board edge, or any polyline |
| `text("label", at).size(mm).rotate(deg).layer(l)` | silkscreen text |
| `artwork("LOGO", "logo.png", center, width).layer(l).tone(Tone::..)` | a PNG traced into silkscreen, read from the board directory |
| `Layer::{FCu, BCu, FSilk, BSilk, FMask, BMask, FFab, BFab, FCourtyard, BCourtyard, EdgeCuts}` | layers |

## composition

| builder | makes |
| --- | --- |
| `items![a, &b, vec, [c, d], option]` | a list of items: parts, handles, groups, vectors, arrays and `Option`s flatten |
| `group(items).at(x, y).rotate(deg).name("buck")` | a frame: its contents are laid out relative to it and move with it. groups nest |
| `impl Module for Block { fn layout(&self) -> Group }`, then `block.at(x, y)` | a block in its own frame |
| `rotate(center, point, deg)`, `polar(center, radius, deg)` | geometry helpers |

## the board

| builder | makes |
| --- | --- |
| `Board::new(project)` | the board `build()` returns |
| `.items(items![..])` | its contents |
| `.rules(rules(track_width, clearance).via(d, drill))` | KiCad's Default class, and what the router works to |
| `.netclasses([netclass("Power", 1.0, [VIN]).via(d, drill).clearance(mm)])` | per-net classes |
| `.router_clearance(mm)` | the clearance FreeRouting routes with (default 0.7; fine-pitch boards need less) |
| `.no_connect([net])` | nets deliberately left with one connection, so they are not warned about |
| `.severities([("courtyards_overlap", "warning")])` | DRC rule severities |
| `.copper_layers(n)` | layer count (default 2) |
| `.hide_references(["J"])` | no silkscreen designator for these prefixes |
| `.finish("Black", "White", "HASL lead-free")` | mask, silk and copper finish |
| `.unlink_after_route(library)` | detach this library's generated footprints after routing |

## the project

| builder | makes |
| --- | --- |
| `Project::new("name")` | the KiCad project name, and the names of everything in `output/` |
| `.kicad_dir(".")` | keep the KiCad project beside the sources instead of in `output/`, to commit and review it |
| `.lcsc([("mpn", "C25804")])` | LCSC part numbers for the JLCPCB BOM |
| `.order_quantity(n)` | boards, for the shopping list quantities |
| `.release_files(["docs/notes.md"])` | extra files for the complete release zip |
| `.schematic(Schematic { title, company, revision, comment, library })` | the schematic's title block |
| `.decisions([("why", "because")])` | recorded in the review lock |

the approved parts list is read from `bom/approved-parts.csv`; see [releasing a board](release.md).

## exporting

```rust
bb_sdk::export_board!(build);
bb_sdk::export_board!(build, route_width);
```

`route_width` is a `fn(&RoutedTrack) -> Option<f64>`, applied to every track after autorouting; `None` keeps its width. `RoutedTrack` has `net`, `layer`, `width`, `start`, `end` and `.is_on(net)`.
