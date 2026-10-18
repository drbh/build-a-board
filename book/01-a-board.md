# a board, from nothing

this page builds [examples/indicator](../examples/indicator), four LEDs on a 5 V header, one step at a time. by the end you will have seen every idea in `bb`; the [reference](README.md#reference) has the rest of the details.

## a crate

a board is an ordinary Rust crate: a `cdylib` that depends on `bb-sdk`, with an empty `[workspace]` so it builds on its own.

```toml
[package]
name = "indicator"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
bb-sdk = { path = "../build-a-board/sdk" }

[workspace]
```

`src/lib.rs` defines one function that returns a `Board`, and exports it:

```rust
use bb_sdk::*;

fn build() -> Board {
    Board::new(Project::new("indicator"))
}

bb_sdk::export_board!(build);
```

that is already a board. `bb` compiles the crate to WebAssembly, runs it, and writes a KiCad project from what `build()` returns. nothing is placed as the function runs; `bb` resolves the whole value afterwards, so the order you write things in never matters.

## nets

declare the nets once, as constants. everything that takes a net takes a `Net`, so a misspelt one does not compile.

```rust
bb_sdk::nets! {
    V5 = "5V",
    GND = "GND",
}
```

a net whose name is computed is `Net::named(format!("LED{n}_A"))`; two with the same name are the same net. `NC` is the net of a pin you leave unwired on purpose.

## a part

`part!` declares a part as a struct with one `Net` field per pin. wiring it is writing a struct literal, so no pin can be forgotten.

```rust
part! {
    /// A green 0805 LED.
    pub struct Led => LedPins {
        cathode: 1,
        anode: 2,
    }
    footprint = "LED_SMD:LED_0805_2012Metric", value = "GREEN"
}
```

resistors, capacitors and connectors are built in: `resistor(ohm(560), V5, anode)`, `capacitor(uf(10), VIN, GND).rated(v(50))`, and `connector("5V IN", "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical").numbered([V5, GND])`.

## placing

`.place(reference, (x, y))` puts a part on the board and returns a handle. the handle has one method per pin, so a track can name a pad as `d1.anode()` instead of `("D1", 2)`.

```rust
fn build() -> Board {
    let power = connector("5V IN", "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical")
        .numbered([V5, GND])
        .place("J1", (24, 28.73));
    let r1 = resistor(ohm(560), V5, Net::named("LED1_A")).place("R1", (34, 25));
    let d1 = Led { anode: Net::named("LED1_A"), cathode: GND }.place("D1", (42, 25)).rotate(180);

    Board::new(Project::new("indicator")).items(items![
        outline(Layer::EdgeCuts, [at(20, 20), at(50, 20), at(50, 40), at(20, 40)], 0.1),
        &power,
        &r1,
        &d1,
    ])
}
```

lengths are millimetres, Y down, as in KiCad; rotations are degrees counter-clockwise. `items![...]` takes parts, handles, tracks, groups, vectors and `Option`s, so generated content composes.

see it:

```bash
bb preview --watch --open
```

the preview draws the board straight from the design in a fraction of a second, with a ratsnest of what still needs connecting. it redraws on every save, highlights what moved, and clicking a pad lights up its net.

## tracks

a track is a net and a list of points. points can be coordinates, pads, or a pad plus an offset; an offset stays with the pad when the part moves.

```rust
track(V5, [power.on(V5), at(30, 28.73)]).width(0.4),
track(d1.part.anode, [r1.p2(), d1.anode()]).width(0.4),
track(GND, [d1.cathode(), d1.cathode() + (4, 0)]).width(0.4),
```

`power.on(V5)` finds the connector pad on that net. a run that changes layers is `route(GND).to(pad).to(pad + (0, 2)).via(Layer::BCu)`. anything you do not route by hand, FreeRouting routes later.

## blocks

a block is a struct whose fields are its placed parts. the indicator is one resistor feeding one LED, on a row:

```rust
pub struct Indicator {
    pub resistor: Placed<Resistor>,
    pub led: Placed<Led>,
}

impl Indicator {
    fn new(n: usize, y: f64) -> Self {
        let anode = Net::named(format!("LED{n}_A"));
        Self {
            resistor: resistor(limit(), V5, anode).place(&format!("R{n}"), (34, y)),
            led: Led { anode, cathode: GND }.place(&format!("D{n}"), (42, y)).rotate(180),
        }
    }
}
```

four of them, with their tracks, is a `map` over the rows:

```rust
const ROWS: [f64; 4] = [25.0, 28.5, 32.0, 35.5];

let bar: Vec<Indicator> = ROWS.iter().enumerate().map(|(i, &y)| Indicator::new(i + 1, y)).collect();

bar.iter().zip(ROWS).map(|(row, y)| {
    let (r, d) = (&row.resistor, &row.led);
    items![
        r,
        d,
        track(V5, [at(30, y), r.p1()]).width(0.4),
        track(d.part.anode, [r.p2(), d.anode()]).width(0.4),
        track(GND, [d.cathode(), at(46, y)]).width(0.4),
    ]
}).collect::<Vec<_>>(),
```

a block that should move as one piece implements `Module`: `fn layout(&self) -> Group` returns its parts and tracks in the block's own frame, and `block.at(20, 10)` puts it anywhere, routes and all. [examples/buck-converter](../examples/buck-converter) is one such block.

## values

values have units, and they are numbers. `limit()` above picks the resistor from the LED's forward voltage and target current:

```rust
const SUPPLY: Volts = Volts(5.0);
const LED_VF: Volts = Volts(2.0);
const LED_MA: f64 = 5.0;

fn limit() -> Ohms {
    ohm(nearest_e24((SUPPLY.0 - LED_VF.0) / (LED_MA / 1000.0)))
}
```

`ohm(560)` prints `560` in the BOM, `kohm(120)` prints `120k`, `.tolerance(1)` adds `1%`. because the design is a program, its rules are tests, and `cargo test` in the board crate runs them on your machine:

```rust
#[test]
fn the_led_runs_near_its_target_current() {
    let ma = current_ma(limit());
    assert!((ma - LED_MA).abs() / LED_MA < 0.1, "{ma:.2} mA");
}
```

## mistakes

mistakes show up in three places. a misspelt net, pin or part, or a wrong unit, is a compile error at the line. a panic in the board, such as `.on(net)` on a connector with no pad on that net, is reported with its message and source line. everything else `bb` checks on the resolved design and reports all at once: duplicate references, pads that do not exist, and nets with fewer than two pads (a warning, unless listed in `Board::no_connect`).

## build

```bash
bb
```

with no command, `bb` builds: it generates the KiCad board, routes it with FreeRouting, writes a connectivity schematic, and renders images. each stage remembers what it was built from in `output/indicator.bb.json` and is skipped when that has not changed, so a second `bb` is instant and a one-line edit reruns only what it touched. routing is slow and not deterministic, so a routed board is never redone unless the unrouted board changed.

```
output/
├── indicator.kicad_pcb / .kicad_pro / .kicad_sch
├── indicator.bb.json           # what each stage was built from
├── preview.html
├── router/                     # the unrouted board, the DSN/SES exchange, one DRC report per attempt
└── render/indicator/           # wiring-top, wiring-bottom, 3-view, 9-angle, schematic, .glb
```

`bb --no-route` stops at the unrouted board, `bb --quick` renders only the top and bottom views, and `bb --force` redoes everything. the [command line reference](reference/cli.md) has the rest.

## what next

- [the examples](02-examples.md), for blocks, generated layout, fine-pitch parts, and a multi-file board
- [the sdk reference](reference/sdk.md), for every builder
- [releasing a board](reference/release.md), when it is time to order one
