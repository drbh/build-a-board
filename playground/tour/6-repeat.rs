//! Four of them.
//!
//! Because a block is a value, a bar of four is a `map` over the rows. Each
//! row's tracks hang off two trunks: 5 V down the left, GND down the right.
//! Change `ROWS` (add a fifth, or space them out) and watch the board follow.

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

/// Rows of the indicator bar.
const ROWS: [f64; 4] = [25.0, 28.5, 32.0, 35.5];

fn build() -> Board {
    let power = connector("5V IN", "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical")
        .numbered([V5, GND])
        .place("J1", (24, 28.73));
    let bar: Vec<Indicator> = ROWS.iter().enumerate().map(|(i, &y)| Indicator::new(i + 1, y)).collect();
    let (top, bottom) = (ROWS[0], ROWS[ROWS.len() - 1]);
    Board::new(Project::new("indicator")).items(items![
        outline(Layer::EdgeCuts, [at(20, 20), at(50, 20), at(50, 40), at(20, 40)], 0.1),
        &power,
        power.legends_column(26.5, [(1, "5V"), (2, "GND")]),
        text("INDICATORS", (37, 22.3)).size(1.2),
        // 5V trunk down the left of the resistors, GND trunk right of the LEDs.
        track(V5, [power.on(V5), at(30, 28.73)]).width(0.4),
        track(V5, [at(30, top), at(30, bottom)]).width(0.4),
        track(GND, [at(46, top), at(46, 38), at(24, 38), power.on(GND)]).width(0.4),
        bar.iter()
            .zip(ROWS)
            .map(|(row, y)| {
                let (r, d) = (&row.resistor, &row.led);
                items![
                    r,
                    d,
                    track(V5, [at(30, y), r.p1()]).width(0.4),
                    track(d.part.anode, [r.p2(), d.anode()]).width(0.4),
                    track(GND, [d.cathode(), at(46, y)]).width(0.4),
                ]
            })
            .collect::<Vec<_>>(),
    ])
}

bb_sdk::export_board!(build);
