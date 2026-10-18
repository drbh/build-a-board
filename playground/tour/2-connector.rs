//! Nets and a connector.
//!
//! `nets!` declares the board's nets once, as constants, so a misspelt net is
//! a compile error. A two-pin header brings 5 V in; placing it returns a
//! handle. The problems panel warns that each net reaches only one pad so far.

use bb_sdk::*;

bb_sdk::nets! {
    V5 = "5V",
    GND = "GND",
}

fn build() -> Board {
    let power = connector("5V IN", "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical")
        .numbered([V5, GND])
        .place("J1", (24, 28.73));
    Board::new(Project::new("indicator")).items(items![
        outline(Layer::EdgeCuts, [at(20, 20), at(50, 20), at(50, 40), at(20, 40)], 0.1),
        &power,
        power.legends_column(26.5, [(1, "5V"), (2, "GND")]),
        text("INDICATORS", (37, 22.3)).size(1.2),
    ])
}

bb_sdk::export_board!(build);
