//! A block.
//!
//! The resistor and LED belong together, so they become a struct: its fields
//! are the placed parts, and `Indicator::new(n, y)` builds one on row `y`.
//! The board looks the same; the next step is where this pays off.

use bb_sdk::*;

bb_sdk::nets! {
    V5 = "5V",
    GND = "GND",
}

part! {
    /// A green 0805 LED.
    pub struct Led => LedPins {
        cathode: 1,
        anode: 2,
    }
    footprint = "LED_SMD:LED_0805_2012Metric", value = "GREEN"
}

/// One indicator: a resistor feeding an LED, on a row at height `y`.
pub struct Indicator {
    pub resistor: Placed<Resistor>,
    pub led: Placed<Led>,
}

impl Indicator {
    fn new(n: usize, y: f64) -> Self {
        // Each indicator's resistor-to-LED node is its own net.
        let anode = Net::named(format!("LED{n}_A"));
        Self {
            resistor: resistor(ohm(620), V5, anode).place(&format!("R{n}"), (34, y)),
            led: Led { anode, cathode: GND }.place(&format!("D{n}"), (42, y)).rotate(180),
        }
    }
}

fn build() -> Board {
    let power = connector("5V IN", "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical")
        .numbered([V5, GND])
        .place("J1", (24, 28.73));
    let one = Indicator::new(1, 25.0);
    let (r, d) = (&one.resistor, &one.led);
    Board::new(Project::new("indicator")).items(items![
        outline(Layer::EdgeCuts, [at(20, 20), at(50, 20), at(50, 40), at(20, 40)], 0.1),
        &power,
        power.legends_column(26.5, [(1, "5V"), (2, "GND")]),
        text("INDICATORS", (37, 22.3)).size(1.2),
        r,
        d,
        track(V5, [power.on(V5), at(30, 28.73), at(30, 25), r.p1()]).width(0.4),
        track(d.part.anode, [r.p2(), d.anode()]).width(0.4),
        track(GND, [d.cathode(), at(46, 25), at(46, 38), at(24, 38), power.on(GND)]).width(0.4),
    ])
}

bb_sdk::export_board!(build);
