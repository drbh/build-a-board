//! Values, computed.
//!
//! 620 Ω was a guess. Values are numbers with units, so the board can work the
//! resistor out from the supply, the LED's forward voltage and a target
//! current, then print what it chose. Try `LED_MA` = 10. This is the finished
//! `examples/indicator`; its tests run with `cargo test`.

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

/// Supply voltage.
const SUPPLY: Volts = Volts(5.0);
/// The LED's forward voltage at its operating current (from its datasheet).
const LED_VF: Volts = Volts(2.0);
/// Indicator brightness: a few milliamps is plenty.
const LED_MA: f64 = 5.0;

/// The E24 (5%) series, one decade.
const E24: [f64; 24] = [
    1.0, 1.1, 1.2, 1.3, 1.5, 1.6, 1.8, 2.0, 2.2, 2.4, 2.7, 3.0, 3.3, 3.6, 3.9, 4.3, 4.7, 5.1, 5.6, 6.2, 6.8, 7.5, 8.2,
    9.1,
];

/// The E24 value nearest `ohms`.
fn nearest_e24(ohms: f64) -> f64 {
    let decade = 10f64.powf(ohms.log10().floor());
    let best =
        E24.iter().chain([&10.0]).map(|m| m * decade).min_by(|a, b| (a - ohms).abs().total_cmp(&(b - ohms).abs()));
    best.expect("E24 is not empty").round()
}

/// The current-limiting resistor for `LED_MA` through the LED.
fn limit() -> Ohms {
    ohm(nearest_e24((SUPPLY.0 - LED_VF.0) / (LED_MA / 1000.0)))
}

/// The LED current a resistor actually gives, in mA.
fn current_ma(r: Ohms) -> f64 {
    (SUPPLY.0 - LED_VF.0) / r.ohms() * 1000.0
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
            resistor: resistor(limit(), V5, anode).place(&format!("R{n}"), (34, y)),
            // Anode toward the resistor.
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
        text(format!("{} OHM / {:.1} mA EACH", limit(), current_ma(limit())), (37, 39.1)).size(0.6),
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

/// Design checks: `cargo test` in this directory.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_led_runs_near_its_target_current() {
        let ma = current_ma(limit());
        assert!((ma - LED_MA).abs() / LED_MA < 0.1, "{ma:.2} mA");
    }

    #[test]
    fn the_resistor_runs_cool() {
        // 0603 resistors are rated 1/10 W; stay under half of that.
        let watts = (SUPPLY.0 - LED_VF.0) * current_ma(limit()) / 1000.0;
        assert!(watts < 0.05, "{watts:.3} W");
    }
}
