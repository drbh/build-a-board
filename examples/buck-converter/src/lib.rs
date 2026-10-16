//! A buck converter module: 12-36 V in, a chosen voltage out at up to 3.5 A.
//!
//! The regulator is a parametric block. `Buck::new(v(5))` picks the
//! feedback resistor from the E96 series to hit the requested output, so
//! the same layout serves any output from about 1 V up; the tests at the
//! bottom check the choice. Change `OUTPUT` and rebuild.

use bb_sdk::*;

bb_sdk::nets! {
    VIN = "VIN",
    VOUT = "VOUT",
    GND = "GND",
}

/// The output this board is built for.
pub const OUTPUT: Volts = Volts(5.0);
/// The highest input the module is specified for (the AP64352 allows 40 V).
pub const VIN_MAX: Volts = Volts(36.0);

const MODELS: &str = "${KICAD10_3DMODEL_DIR}";
const TERMINAL: &str = "TerminalBlock_Phoenix:TerminalBlock_Phoenix_MKDS-1,5-2_1x02_P5.00mm_Horizontal";

// --- Parts -----------------------------------------------------------------------

/// Reference voltage of the AP64352's feedback comparator.
const VREF: Volts = Volts(0.8);

part! {
    /// Diodes AP64352 40 V / 3.5 A synchronous buck in SO-8EP.
    pub struct Ap64352 => Ap64352Pins {
        /// Bootstrap capacitor to SW.
        bst: 1,
        vin: 2,
        /// Enable; tied to VIN so the regulator runs whenever powered.
        en: 3,
        /// Switching-frequency resistor to GND.
        rt: 4,
        fb: 5,
        /// Soft-start capacitor to GND.
        ss: 6,
        gnd: 7,
        sw: 8,
        /// Exposed thermal pad, also GND.
        ep: 9,
    }
}

impl Component for Ap64352 {
    type Pins = Ap64352Pins;

    /// KiCad ships the exact footprint without its named model; the standard
    /// 3.9 x 4.9 mm SO-8 body is mechanically representative.
    fn footprint(&self, reference: &str) -> Footprint {
        part(reference, "AP64352SP-13", "Package_SO:Diodes_SO-8EP")
            .pins(self.pads())
            .swap_model(format!("{MODELS}/Package_SO.3dshapes/SO-8_3.9x4.9mm_P1.27mm.step"))
            // JLCPCB's library turns this package 270° from KiCad's; without the
            // correction its pick-and-place file rotates U1/U2 wrongly.
            .assembly_rotation(270)
            .into()
    }
}

part! {
    /// Bourns SRP1050WA 10 uH shielded power inductor, 10 x 10 mm.
    pub struct Inductor => InductorPins {
        p1: 1,
        p2: 2,
    }
    footprint = "Inductor_SMD:L_Bourns_SRP1050WA", value = "10uH"
}

part! {
    pub struct MountingHole => MountingHolePins {}
    footprint = "MountingHole:MountingHole_3.2mm_M3", value = "M3"
}

/// The E96 (1%) series, one decade.
const E96: [f64; 96] = [
    1.00, 1.02, 1.05, 1.07, 1.10, 1.13, 1.15, 1.18, 1.21, 1.24, 1.27, 1.30, 1.33, 1.37, 1.40, 1.43, 1.47, 1.50, 1.54,
    1.58, 1.62, 1.65, 1.69, 1.74, 1.78, 1.82, 1.87, 1.91, 1.96, 2.00, 2.05, 2.10, 2.15, 2.21, 2.26, 2.32, 2.37, 2.43,
    2.49, 2.55, 2.61, 2.67, 2.74, 2.80, 2.87, 2.94, 3.01, 3.09, 3.16, 3.24, 3.32, 3.40, 3.48, 3.57, 3.65, 3.74, 3.83,
    3.92, 4.02, 4.12, 4.22, 4.32, 4.42, 4.53, 4.64, 4.75, 4.87, 4.99, 5.11, 5.23, 5.36, 5.49, 5.62, 5.76, 5.90, 6.04,
    6.19, 6.34, 6.49, 6.65, 6.81, 6.98, 7.15, 7.32, 7.50, 7.68, 7.87, 8.06, 8.25, 8.45, 8.66, 8.87, 9.09, 9.31, 9.53,
    9.76,
];

/// The E96 value nearest `ohms`, as kilohms.
fn nearest_e96(ohms: f64) -> f64 {
    let decade = 10f64.powf(ohms.log10().floor());
    let candidates = E96.iter().map(|m| m * decade).chain([10.0 * decade]);
    let best = candidates.min_by(|a, b| (a - ohms).abs().total_cmp(&(b - ohms).abs())).expect("E96 is not empty");
    // Round away float noise from the decade scaling: 105000.00000000001 → 105.
    (best / 10.0).round() / 100.0
}

// --- The regulator -------------------------------------------------------------------

/// The AP64352 regulator, laid out around U1 in its own frame: input on the
/// left, switch node and inductor across the top, output on the right. Its
/// internal nets live in the block's own scope (`buck.SW`, ...), so nothing
/// outside can touch them and a second regulator would get its own.
pub struct Buck {
    pub u1: Placed<Ap64352>,
    pub l1: Placed<Inductor>,
    /// C1, C2: 10 uF / 50 V beside VIN.
    pub input: [Placed<Capacitor>; 2],
    /// C5, C6: 22 uF / 25 V.
    pub output: [Placed<Capacitor>; 2],
    pub bootstrap: Placed<Capacitor>,
    pub soft_start: Placed<Capacitor>,
    pub timing: Placed<Resistor>,
    /// R2, from VOUT to FB.
    pub feedback_top: Placed<Resistor>,
    /// R3, from FB to GND.
    pub feedback_bottom: Placed<Resistor>,
}

impl Buck {
    /// The divider's bottom leg; the top leg is chosen to suit the output.
    const FEEDBACK_BOTTOM_KOHM: f64 = 20.0;

    pub fn new(vout: Volts) -> Self {
        let nets = Scope::new("buck");
        let (sw, bst, rt, ss, fb) = (nets.net("SW"), nets.net("BST"), nets.net("RT"), nets.net("SS"), nets.net("FB"));
        let top = nearest_e96(Self::FEEDBACK_BOTTOM_KOHM * 1e3 * (vout.0 / VREF.0 - 1.0));
        let precise = |value, p1, p2| resistor(value, p1, p2).tolerance(1);
        let u1 = Ap64352 { bst, vin: VIN, en: VIN, rt, fb, ss, gnd: GND, sw, ep: GND };
        Self {
            u1: u1.place("U1", (0, 0)),
            l1: Inductor { p1: sw, p2: VOUT }.place("L1", (11.5, -1.905)),
            input: [("C1", -0.635), ("C2", 2.6)].map(|(reference, y)| {
                capacitor(uf(10), VIN, GND).rated(v(50)).footprint(C0805).place(reference, (-6, y)).rotate(180)
            }),
            output: [("C5", -5.5), ("C6", 1.5)].map(|(reference, y)| {
                capacitor(uf(22), VOUT, GND).rated(v(25)).footprint(C1206).place(reference, (20.5, y))
            }),
            bootstrap: capacitor(nf(100), bst, sw).rated(v(50)).place("C3", (0, -4.5)),
            soft_start: capacitor(nf(100), ss, GND).rated(v(50)).place("C4", (4, 4.5)).rotate(270),
            timing: precise(kohm(200), rt, GND).place("R1", (-2.5, 4.5)).rotate(270),
            feedback_top: precise(kohm(top), VOUT, fb).place("R2", (10, 6.5)).rotate(180),
            feedback_bottom: precise(kohm(Self::FEEDBACK_BOTTOM_KOHM), fb, GND).place("R3", (7, 6.5)).rotate(180),
        }
    }

    /// The output the divider actually sets: VREF x (1 + top / bottom).
    pub fn vout(&self) -> Volts {
        let (top, bottom) = (self.feedback_top.part.value.ohms(), self.feedback_bottom.part.value.ohms());
        Volts(VREF.0 * (1.0 + top / bottom))
    }
}

/// A short ground track from a pad to its own via into the back plane.
fn ground_via(from: At, to: At, width: f64) -> Route {
    route(GND).width(width).via_size(0.7, 0.3).to(from).to(to).via(Layer::BCu)
}

impl Module for Buck {
    fn layout(&self) -> Group {
        let Self { u1, l1, input: [c1, c2], output: [c5, c6], bootstrap: c3, soft_start: c4, timing: r1, .. } = self;
        let (r2, r3) = (&self.feedback_top, &self.feedback_bottom);
        let Ap64352 { sw, bst, ss, fb, .. } = u1.part;
        // The hot loop (VIN caps, switch node, bootstrap) is fixed; the
        // router finishes the quiet signals.
        let mut stitching = Vec::new();
        for x in [-0.65, 0.0, 0.65] {
            for y in [-1.0, 1.0] {
                stitching.push(via(GND, (x, y)).size(0.6, 0.3));
            }
        }
        group(items![
            u1,
            l1,
            [c1, c2, c3, c4, c5, c6],
            [r1, r2, r3],
            track(VIN, [c1.p1(), u1.vin()]).width(0.8),
            track(VIN, [c1.p1(), c2.p1()]).width(0.8),
            track(VIN, [u1.en(), u1.vin()]).width(0.35),
            track(sw, [u1.sw(), at(4.3, -1.905), l1.p1()]).width(1.2),
            track(bst, [u1.bst(), at(-2.4975, -4.5), c3.p1()]).width(0.35),
            track(sw, [c3.p2(), at(2.4975, -4.5), u1.sw()]).width(0.35),
            track(VOUT, [l1.p2(), at(18.1, -1.905), at(18.1, -5.5), c5.p1()]).width(1.2),
            track(VOUT, [at(18.1, -1.905), at(18.1, 1.5), c6.p1()]).width(1.2),
            track(fb, [r3.p1(), r2.p2()]).width(0.25),
            // SS escapes down the clear column between FB's pad and the inductor.
            track(ss, [u1.ss(), at(4.0, 0.635), c4.p1()]).width(0.3),
            // Ground vias just outboard of each return pad.
            ground_via(c1.p2(), c1.p2() + (-1.65, 0), 0.8),
            ground_via(c2.p2(), c2.p2() + (-1.65, 0), 0.8),
            ground_via(c5.p2(), c5.p2() + (1.225, 0), 0.8),
            ground_via(c6.p2(), c6.p2() + (1.225, 0), 0.8),
            ground_via(u1.gnd(), at(4.3, -0.635), 0.5),
            ground_via(c4.p2(), c4.p2() + (0, 1.525), 0.4),
            ground_via(r1.p2(), r1.p2() + (0, 1.525), 0.4),
            ground_via(r3.p2(), at(6.175, 8.3), 0.4),
            // Thermal vias in the exposed pad, into the back ground plane.
            stitching,
            text(format!("AP64352  {OUTPUT} / 3.5A"), (6.0, 9.6)).size(0.8),
        ])
        .name("buck")
    }
}

// --- The board -------------------------------------------------------------------

/// A Phoenix MKDS screw terminal, wire entry facing the board edge.
fn terminal(
    reference: &str,
    value: &str,
    nets: [Net; 2],
    at: impl Into<Point>,
    rotate: impl Into<f64>,
) -> Placed<Connector> {
    connector(value, TERMINAL).numbered(nets).place(reference, at).rotate(rotate)
}

fn build() -> Board {
    let buck = Buck::new(OUTPUT);
    let input = terminal("J1", "VIN 12-36V", [VIN, GND], (27, 40), 90);
    let output = terminal("J2", "VOUT", [VOUT, GND], (73, 35), 270);
    let corners = [(20, 22), (80, 22), (80, 54), (20, 54)];
    Board::new(
        Project::new("buck-converter")
            .release_files(["Cargo.toml", "Cargo.lock", "src"])
            .schematic(Schematic { title: Some(format!("AP64352 {OUTPUT} buck converter")), ..Schematic::default() }),
    )
    .netclasses([netclass("Power", 1.0, [VIN, VOUT]).via(1.0, 0.5)])
    .severities([("courtyards_overlap", "warning")])
    .hide_references(["J", "H"])
    .items(items![
        outline(Layer::EdgeCuts, corners.map(|(x, y)| at(x, y)), 0.1),
        [("H1", (23.5, 25.5)), ("H2", (76.5, 25.5)), ("H3", (23.5, 50.5)), ("H4", (76.5, 50.5))]
            .map(|(reference, at)| MountingHole.place(reference, at)),
        buck.at(45, 37),
        &input,
        &output,
        input.legends_column(34, [(1, "VIN"), (2, "GND")]),
        output.legends_column(66, [(1, "VOUT"), (2, "GND")]),
        // Input and output feed through the board; the router joins them to
        // the regulator. Ground is the back plane.
        zone(GND, Layer::BCu, [at(20.5, 22.5), at(79.5, 22.5), at(79.5, 53.5), at(20.5, 53.5)]),
        text("BUCK CONVERTER", (50, 51)).size(1.2),
    ])
}

/// Routed power tracks keep at least the power class width.
fn route_width(t: &RoutedTrack) -> Option<f64> {
    ((t.is_on(VIN) || t.is_on(VOUT)) && t.width < 1.0).then_some(1.0)
}

bb_sdk::export_board!(build, route_width);

/// Design checks: `cargo test` in this directory.
#[cfg(test)]
mod tests {
    use super::*;

    fn error_percent(actual: Volts, target: Volts) -> f64 {
        (actual.0 - target.0).abs() / target.0 * 100.0
    }

    #[test]
    fn the_divider_sets_the_output() {
        let buck = Buck::new(OUTPUT);
        assert_eq!(buck.feedback_top.part.value, kohm(105));
        assert!(error_percent(buck.vout(), OUTPUT) < 0.1, "{:?}", buck.vout());
    }

    #[test]
    fn any_common_output_is_within_one_percent() {
        for target in [1.2, 1.8, 2.5, 3.3, 5.0, 9.0, 12.0, 15.0] {
            let buck = Buck::new(v(target));
            assert!(error_percent(buck.vout(), v(target)) < 1.0, "{target} V gives {:?}", buck.vout());
        }
    }

    #[test]
    fn capacitors_are_rated_with_margin() {
        let buck = Buck::new(OUTPUT);
        for c in &buck.input {
            assert!(c.part.rating.unwrap().0 >= VIN_MAX.0 * 1.25, "input caps need 25% over VIN max");
        }
        for c in &buck.output {
            assert!(c.part.rating.unwrap().0 >= OUTPUT.0 * 2.0, "output caps need 2x VOUT");
        }
    }

    #[test]
    fn internal_nets_stay_inside_the_block() {
        let buck = Buck::new(OUTPUT);
        assert_eq!(buck.u1.part.sw.name(), "buck.SW");
        assert_eq!(buck.bootstrap.part.nets, TwoPin { p1: buck.u1.part.bst, p2: buck.u1.part.sw });
    }

    #[test]
    fn e96_lookup() {
        assert_eq!(nearest_e96(105_000.0), 105.0);
        assert_eq!(nearest_e96(62_500.0), 61.9);
        assert_eq!(nearest_e96(9_990.0), 10.0);
    }
}
