//! A small board exercising typed nets, part handles, frames, anchors used
//! before their parts' items, and legends.

use bb_sdk::*;

bb_sdk::nets! {
    VIN = "VIN",
    MID = "MID",
    GND = "GND",
}

fn ten_k(p1: Net, p2: Net) -> Resistor {
    resistor(kohm(10), p1, p2).footprint("Test:R_Test")
}

/// A two-resistor module laid out in its own frame.
struct Divider {
    top: Placed<Resistor>,
    bottom: Placed<Resistor>,
}

impl Divider {
    fn new(top: Net, mid: Net, bottom: Net) -> Self {
        Self { top: ten_k(top, mid).place("R1", (0, 0)), bottom: ten_k(mid, bottom).place("R2", (0, 4)).rotate(270) }
    }
}

impl Module for Divider {
    fn layout(&self) -> Group {
        let mid = self.top.part.nets.p2;
        group(items![&self.top, &self.bottom, track(mid, [self.top.p2(), at(1.5, 2), self.bottom.p1()]).width(0.25)])
            .name("divider")
    }
}

fn build() -> Board {
    let divider = Divider::new(VIN, MID, GND);
    let input = connector("IN", "Test:R_Test").numbered([VIN, GND]).place("J1", (0, 10));
    Board::new(Project::new("fixture").lcsc([("MPN", "C1")]))
        .hide_references(["J"])
        // Items deliberately use R2's pads before the divider's items.
        .items(items![
            track(GND, [divider.bottom.p2(), at(15, 20)]).width(0.3),
            via(GND, (15, 20)).size(0.6, 0.3),
            divider.bottom.legends_column(22, [(1, "+"), (2, "-")]),
            divider.at(10, 10),
            input,
        ])
}

fn route_width(track: &RoutedTrack) -> Option<f64> {
    track.is_on(GND).then_some(0.5)
}

bb_sdk::export_board!(build, route_width);
