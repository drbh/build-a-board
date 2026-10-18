//! Tracks.
//!
//! A track runs through points: `at(x, y)` on the board, or a pad through its
//! part's handle, like `r1.p1()` or `d1.anode()`. A pad that doesn't exist
//! won't compile. With three tracks the ratsnest is gone: 0 unrouted.

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

fn build() -> Board {
    let power = connector("5V IN", "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical")
        .numbered([V5, GND])
        .place("J1", (24, 28.73));
    let anode = Net::named("LED1_A");
    let r1 = resistor(ohm(620), V5, anode).place("R1", (34, 25));
    let d1 = Led { anode, cathode: GND }.place("D1", (42, 25)).rotate(180);
    Board::new(Project::new("indicator")).items(items![
        outline(Layer::EdgeCuts, [at(20, 20), at(50, 20), at(50, 40), at(20, 40)], 0.1),
        &power,
        power.legends_column(26.5, [(1, "5V"), (2, "GND")]),
        text("INDICATORS", (37, 22.3)).size(1.2),
        &r1,
        &d1,
        track(V5, [power.on(V5), at(30, 28.73), at(30, 25), r1.p1()]).width(0.4),
        track(anode, [r1.p2(), d1.anode()]).width(0.4),
        track(GND, [d1.cathode(), at(46, 25), at(46, 38), at(24, 38), power.on(GND)]).width(0.4),
    ])
}

bb_sdk::export_board!(build);
