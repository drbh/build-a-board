//! A 50 mm round board with twelve WS2812B LEDs on a ring.
//!
//! Data enters at the centre header, runs clockwise from LED to LED, and
//! leaves on a pad at the edge so rings can be chained. Every LED has its
//! own 100 nF decoupling capacitor just inside the ring.

// Blocks are specific placed things; `new()` reads better than `Default`.
#![allow(clippy::new_without_default)]

use bb_sdk::*;

bb_sdk::nets! {
    V5 = "5V",
    GND = "GND",
}

/// LEDs on the ring.
pub const COUNT: usize = 12;

/// The data chain: `data(0)` from the header into LED 1, ..., `data(COUNT)`
/// out of the last LED.
pub fn data(i: usize) -> Net {
    Net::named(format!("D{i}"))
}

const CENTER: (f64, f64) = (50.0, 50.0);
const BOARD_RADIUS: f64 = 25.0;
const LED_RADIUS: f64 = 18.0;
const CAP_RADIUS: f64 = 13.0;
/// Worst-case draw per LED, all three colours at full brightness.
const LED_MAX_MA: f64 = 60.0;

part! {
    /// Worldsemi WS2812B addressable RGB LED, 5050 package.
    pub struct Ws2812b => Ws2812bPins {
        vdd: 1,
        dout: 2,
        vss: 3,
        din: 4,
    }
    footprint = "LED_SMD:LED_WS2812B_PLCC4_5.0x5.0mm_P3.2mm", value = "WS2812B"
}

/// Where pixel `i` sits: from the top, a twelfth of a turn clockwise per
/// pixel (so the angle, counter-clockwise from +X, falls).
fn bearing(i: f64) -> f64 {
    90.0 - 360.0 * i / COUNT as f64
}

/// One LED and its decoupling capacitor.
pub struct Pixel {
    pub led: Placed<Ws2812b>,
    pub cap: Placed<Capacitor>,
}

/// The ring.
pub struct Ring {
    pub pixels: Vec<Pixel>,
}

impl Ring {
    pub fn new() -> Self {
        let pixels = (0..COUNT)
            .map(|i| {
                let angle = bearing(i as f64);
                // An LED's data flows DIN (+X) to DOUT (-X); a quarter turn
                // past its bearing points that along the ring, clockwise.
                let tangent = angle + 90.0;
                // FRICTION: references are numbered by hand with format!; the
                // SDK has no annotation (D1, D2, ...) for repeated blocks.
                let n = i + 1;
                let led = Ws2812b { vdd: V5, dout: data(i + 1), vss: GND, din: data(i) };
                Pixel {
                    led: led.place(&format!("D{n}"), polar(CENTER, LED_RADIUS, angle)).rotate(tangent),
                    cap: capacitor(nf(100), V5, GND)
                        .rated(v(16))
                        .place(&format!("C{n}"), polar(CENTER, CAP_RADIUS, angle))
                        .rotate(tangent),
                }
            })
            .collect();
        Self { pixels }
    }

    /// Worst-case current, all LEDs full white.
    pub fn max_current_ma(&self) -> f64 {
        self.pixels.len() as f64 * LED_MAX_MA
    }
}

impl Module for Ring {
    fn layout(&self) -> Group {
        group(self.pixels.iter().map(|p| items![&p.led, &p.cap]).collect::<Vec<_>>()).name("ring")
    }
}

/// A closed polygon approximating a circle.
// FRICTION: outlines and zones are straight segments only, so a round board
// is a 72-gon; KiCad itself supports circles and arcs.
fn circle(radius: f64) -> Vec<At> {
    (0..72).map(|i| At::from(polar(CENTER, radius, f64::from(i) * 5.0))).collect()
}

fn build() -> Board {
    let ring = Ring::new();
    let header = connector("5V DIN GND", "Connector_PinHeader_2.54mm:PinHeader_1x03_P2.54mm_Vertical")
        .numbered([V5, data(0), GND])
        .place("J1", (CENTER.0, CENTER.1 - 2.54));
    let bulk =
        capacitor(uf(10), V5, GND).rated(v(16)).footprint(C0805).place("C13", (CENTER.0 + 5.0, CENTER.1)).rotate(90);
    // Data out to the next ring, just outside the last LED.
    let dout = connector("DOUT", "TestPoint:TestPoint_Pad_D1.5mm")
        .numbered([data(COUNT)])
        .place("TP1", polar(CENTER, 22.0, bearing(COUNT as f64 - 0.5)));

    Board::new(Project::new("led-ring").release_files(["Cargo.toml", "Cargo.lock", "src"]))
        .netclasses([netclass("Power", 0.5, [V5, GND])])
        .severities([("courtyards_overlap", "warning")])
        .hide_references(["J", "TP"])
        .items(items![
            outline(Layer::EdgeCuts, circle(BOARD_RADIUS), 0.1),
            ring.layout(),
            &header,
            &bulk,
            &dout,
            header.legends_column(CENTER.0 - 3.0, [(1, "5V"), (2, "DIN"), (3, "GND")]),
            text("DOUT", polar(CENTER, 22.0, bearing(COUNT as f64 - 1.4))).size(0.8),
            zone(GND, Layer::BCu, circle(BOARD_RADIUS - 0.5)),
        ])
}

bb_sdk::export_board!(build);

/// Design checks: `cargo test` in this directory.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_runs_through_every_led_in_order() {
        let ring = Ring::new();
        for (i, pixel) in ring.pixels.iter().enumerate() {
            assert_eq!(pixel.led.part.din, data(i));
            assert_eq!(pixel.led.part.dout, data(i + 1));
        }
    }

    #[test]
    fn every_led_is_decoupled() {
        let ring = Ring::new();
        assert!(ring.pixels.iter().all(|p| p.cap.part.nets.p1 == p.led.part.vdd && p.cap.part.nets.p2 == GND));
    }

    #[test]
    fn full_white_stays_under_one_amp() {
        assert!(Ring::new().max_current_ma() <= 1000.0);
    }

    #[test]
    fn the_ring_fits_the_board() {
        // A 5 mm LED's farthest corner must stay 1 mm inside the edge.
        assert!(LED_RADIUS + 2.5 * 2f64.sqrt() <= BOARD_RADIUS - 1.0);
    }
}
