//! A part, wired by its nets.
//!
//! `part!` makes a type whose fields are its pins, so wiring an LED means
//! naming every pin. A resistor feeds it through a net of its own. Nothing is
//! routed yet: the dashed ratsnest shows the connections still to make.

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
    ])
}

bb_sdk::export_board!(build);
