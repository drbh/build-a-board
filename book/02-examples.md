# examples

one board per idea. each is a crate under [examples/](../examples): `bb --board examples/<name> preview --watch --open` draws it, `bb --board examples/<name>` builds it, and `cargo test` in its directory runs its design checks. the single-file ones also open in the [playground](../playground).

- [indicator](../examples/indicator): four LEDs on a 5 V header, each resistor sized from the LED's forward voltage, every track routed by hand. the board [the book](01-a-board.md) builds
- [buck-converter](../examples/buck-converter): one parametric block. `Buck::new(v(5))` picks its feedback resistor from the E96 series; change `OUTPUT` and rebuild. tests check the output is within 1% and the capacitors are rated with margin
- [led-ring](../examples/led-ring): twelve WS2812Bs placed in a loop with `polar`, with a data chain of generated nets
- [usb-uart](../examples/usb-uart): USB-C receptacle, ESD protection and a CH340C. a pin on several pads (`vbus: ["A4", "A9", "B4", "B9"]`), a hand-routed fan-out written as offsets from pads, and a smaller `router_clearance` for the 0.5 mm pitch
- [balance-bot](../examples/balance-bot): a complete devboard across several modules: `nets.rs`, `parts.rs`, `power.rs` and `control.rs` (blocks), `routes.rs` (fixed routes between blocks, written through their handles), and `lib.rs`. the only example with an approved parts list, so `bb check` and `bb prep` run on it

[FRICTION.md](../examples/FRICTION.md) records what was awkward to express while writing them, what the SDK now does about it, and what is still open.
