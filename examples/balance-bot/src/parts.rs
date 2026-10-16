//! Parts and layout patterns for the Balance Bot devboard.
//!
//! Each part is a type: its struct names the nets on its pins, and placing
//! it returns a handle whose methods are those pins (`u1.sw()`).

use bb_sdk::*;

use crate::nets::{GND, Wheel};

pub const LIB: &str = "BalanceBot_Devboard";
pub const MODELS: &str = "${KICAD10_3DMODEL_DIR}";
pub const R0603_HAND: &str = "Resistor_SMD:R_0603_1608Metric_Pad0.98x0.95mm_HandSolder";

// --- Regulators ------------------------------------------------------------------

/// Reference voltage of the AP64352's feedback comparator.
pub const AP64352_VREF: Volts = Volts(0.8);

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

    /// KiCad 10 ships the exact footprint without its named model; the
    /// standard 3.9 x 4.9 mm SO-8 body is mechanically representative.
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

/// SHOU HAN CYA0850 10 uH / 5 A inductor: 9.2 x 8 mm body on JLC/LCSC's
/// 2.5 x 3.8 mm pads.
#[derive(Clone, Copy, Debug)]
pub struct Cya0850 {
    pub nets: TwoPin,
}

impl Component for Cya0850 {
    type Pins = TwoPinPins;

    fn footprint(&self, reference: &str) -> Footprint {
        generated(reference, "10uH 5A / C19268645", format!("{LIB}:CYA0850_10uH"))
            .pads([smd(1, (-3.7, 0.0), (2.5, 3.8)).net(self.nets.p1), smd(2, (3.7, 0.0), (2.5, 3.8)).net(self.nets.p2)])
            .body(9.2, 8.0)
            .models([model(format!("{MODELS}/Inductor_SMD.3dshapes/L_Coilcraft_XAL8050-223.step"))])
            .into()
    }
}

pub fn inductor(input: Net, output: Net) -> Cya0850 {
    Cya0850 { nets: TwoPin { p1: input, p2: output } }
}

part! {
    /// A polarised capacitor's terminals.
    pub struct Polarised => PolarisedPins {
        positive: 1,
        negative: 2,
    }
}

/// 10 mm SMD electrolytic (SamYoung BXJ) on JLC's exact V-chip land pattern.
///
/// Pad 1 is the positive terminal, matching JLC's own footprints. The
/// polarity silk mirrors them too: chamfered corners on the positive side,
/// '+' beside pad 1 and '-' beside pad 2, checkable before and after reflow.
#[derive(Clone, Copy, Debug)]
pub struct VchipBulk {
    pub value: Farads,
    pub rating: Volts,
    pub nets: Polarised,
}

/// The board's bulk capacitor: 470 uF / 25 V from `rail` to ground.
pub fn bulk(rail: Net) -> VchipBulk {
    VchipBulk { value: uf(470), rating: v(25), nets: Polarised { positive: rail, negative: GND } }
}

impl Component for VchipBulk {
    type Pins = PolarisedPins;

    fn footprint(&self, reference: &str) -> Footprint {
        let outline = [
            ((5.23, -0.85), (5.23, -5.23)),
            ((5.23, -5.23), (-3.09, -5.23)),
            ((-3.09, -5.23), (-5.23, -3.09)),
            ((-5.23, -3.09), (-5.23, -0.85)),
            ((5.23, 0.85), (5.23, 5.23)),
            ((5.23, 5.23), (-3.09, 5.23)),
            ((-3.09, 5.23), (-5.23, 3.09)),
            ((-5.23, 3.09), (-5.23, 0.85)),
        ];
        let marks = [((-4.84, 2.37), (-3.61, 2.37)), ((-4.22, 1.34), (-4.22, 3.40)), ((3.60, 2.37), (4.84, 2.37))];
        generated(reference, format!("{} {}", self.value, self.rating), format!("{LIB}:SamYoung_BXJ_J10_C3002464"))
            .pads([
                smd(1, (-4.5, 0.0), (4.5, 1.65)).net(self.nets.positive),
                smd(2, (4.5, 0.0), (4.5, 1.65)).net(self.nets.negative),
            ])
            .body(10.0, 10.0)
            .graphics(outline.map(|(a, b)| fp_line(Layer::FSilk, a, b, 0.15)))
            .graphics(marks.map(|(a, b)| fp_line(Layer::FSilk, a, b, 0.31)))
            .models([model(format!("{MODELS}/Capacitor_SMD.3dshapes/CP_Elec_10x10.step"))])
            .into()
    }
}

// --- Power entry -------------------------------------------------------------------

/// Keystone 3568 holder for one MINI/ATM blade fuse.
///
/// Each terminal lands two 1.78 mm pins 3.4 mm apart (both numbered for the
/// terminal, so `p1()` is the first and `p1() + (0, 3.4)` the second) and
/// the terminals sit on a 9.92 mm pitch. MINI rather than ATO: both
/// interrupt 1 kA at 32 V, which is what matters against a 4S pack, but the
/// MINI holder is a quarter of the price, has four holes instead of nine,
/// and has a curated KiCad land pattern. The original hand-drawn clip
/// footprint was wrong by the width of the fuse body.
#[derive(Clone, Copy, Debug)]
pub struct FuseHolder {
    pub value: &'static str,
    /// Supply side on pad 1, load side on pad 2.
    pub nets: TwoPin,
}

impl FuseHolder {
    /// The second pin of a terminal, below the first.
    pub const PIN_PITCH: f64 = 3.4;

    /// Place by the middle of the four-hole pattern, not the footprint
    /// origin (the terminal 1 pin nearest it).
    pub fn place_centred(self, reference: &str, (x, y): (f64, f64)) -> Placed<Self> {
        self.place(reference, (x - 4.96, y - 1.7))
    }
}

impl Component for FuseHolder {
    type Pins = TwoPinPins;

    fn footprint(&self, reference: &str) -> Footprint {
        part(reference, self.value, "Fuse:Fuseholder_Blade_Mini_Keystone_3568").pins(self.nets.pads()).into()
    }
}

part! {
    pub struct Diode => DiodePins {
        /// KiCad's banded pad.
        cathode: 1,
        anode: 2,
    }
}

/// A through-hole TVS diode in DO-15.
#[derive(Clone, Copy, Debug)]
pub struct Tvs {
    pub part: &'static str,
    pub nets: Diode,
}

impl Component for Tvs {
    type Pins = DiodePins;

    fn footprint(&self, reference: &str) -> Footprint {
        part(reference, self.part, "Diode_THT:D_DO-15_P10.16mm_Horizontal").pins(self.nets.pads()).into()
    }
}

// --- Mechanical and test ---------------------------------------------------------

part! {
    /// An unpopulated probe pad; its value is the net it probes.
    pub struct TestPoint => TestPointPins {
        point: 1,
    }
}

impl Component for TestPoint {
    type Pins = TestPointPins;

    fn footprint(&self, reference: &str) -> Footprint {
        part(reference, self.point.name(), "TestPoint:TestPoint_Pad_D1.5mm").pins(self.pads()).into()
    }
}

part! {
    pub struct Fiducial => FiducialPins {}
    footprint = "Fiducial:Fiducial_1mm_Mask2mm", value = "FIDUCIAL"
}

part! {
    pub struct MountingHole => MountingHolePins {}
    footprint = "MountingHole:MountingHole_3.2mm_M3", value = "M3"
}

// --- SimpleFOC Mini carrier --------------------------------------------------------

part! {
    /// The Mini's direct control header (2x05; unlisted pads are unused).
    pub struct MiniControl => MiniControlPins {
        gnd: 1,
        in1: 3,
        gnd2: 4,
        in2: 5,
        in3: 7,
        en: 9,
        nfault: 10,
    }
}

part! {
    /// The Mini's phase header: official pin 1=M3, 2=M2, 3=M1.
    pub struct MiniPhase => MiniPhasePins {
        m3: 1,
        m2: 2,
        m1: 3,
    }
}

part! {
    /// The Mini's two power holes.
    pub struct MiniSupply => MiniSupplyPins {
        gnd: 1,
        vbat: 2,
    }
}

/// The control header, wired and named for one side.
#[derive(Clone, Debug)]
pub struct ControlHeader {
    side: &'static str,
    nets: MiniControl,
}

impl Component for ControlHeader {
    type Pins = MiniControlPins;

    fn footprint(&self, reference: &str) -> Footprint {
        let value = format!("SIMPLEFOC MINI V1.0 {} DIRECT CONTROL PINS", self.side);
        part(reference, value, "Connector_PinHeader_2.54mm:PinHeader_2x05_P2.54mm_Vertical")
            .pins(self.nets.pads())
            .into()
    }
}

/// The phase header: a socket footprint carrying a pin-header model.
#[derive(Clone, Debug)]
pub struct PhaseHeader {
    side: &'static str,
    nets: MiniPhase,
}

impl Component for PhaseHeader {
    type Pins = MiniPhasePins;

    fn footprint(&self, reference: &str) -> Footprint {
        let value = format!("SIMPLEFOC MINI V1.0 {} DIRECT PHASE PINS", self.side);
        part(reference, value, "Connector_PinSocket_2.54mm:PinSocket_1x03_P2.54mm_Vertical")
            .pins(self.nets.pads())
            .models([model(format!(
                "{MODELS}/Connector_PinHeader_2.54mm.3dshapes/PinHeader_1x03_P2.54mm_Vertical.step"
            ))])
            .into()
    }
}

/// Two separately cut 1x1 pins at the Mini's exact power-hole centres: the
/// holes sit on a non-standard 4.953 mm pitch.
#[derive(Clone, Debug)]
pub struct PowerPins {
    side: &'static str,
    nets: MiniSupply,
    /// Hole centres relative to the part origin, midway between them.
    offsets: [(f64, f64); 2],
    /// Whether the holes run along X.
    horizontal: bool,
}

impl Component for PowerPins {
    type Pins = MiniSupplyPins;

    fn footprint(&self, reference: &str) -> Footprint {
        let value = format!("SIMPLEFOC MINI V1.0 {} DIRECT POWER PINS", self.side);
        let pin = format!("{MODELS}/Connector_PinHeader_2.54mm.3dshapes/PinHeader_1x01_P2.54mm_Vertical.step");
        let (w, h) = if self.horizontal { (6.5, 3.0) } else { (3.0, 6.5) };
        generated(reference, value, format!("{LIB}:Generated_{reference}"))
            .pads([
                tht(1, self.offsets[0], (2.0, 2.0), 1.0).net(self.nets.gnd),
                tht(2, self.offsets[1], (2.0, 2.0), 1.0).net(self.nets.vbat).shape(PadShape::Circle),
            ])
            .body(w, h)
            .models(self.offsets.map(|(x, y)| model(pin.clone()).offset(x, y, 0.0)))
            .into()
    }
}

/// Direct-solder carrier for an official SimpleFOC Mini v1.0.
///
/// Geometry is transcribed from the tagged v1.0 EasyEDA source. A 3 A male
/// breakaway strip is cut into control, phase, and individual power sections;
/// the short posts solder into this carrier and the long pins pass through
/// the Mini, soldered permanently after alignment.
pub struct MiniCarrier {
    pub center: (f64, f64),
    /// The module's rotation, degrees counter-clockwise.
    pub angle: f64,
    pub control: Placed<ControlHeader>,
    pub phase: Placed<PhaseHeader>,
    pub power: Placed<PowerPins>,
}

/// References for one carrier's three headers.
pub struct CarrierRefs {
    pub control: &'static str,
    pub power: &'static str,
    pub phase: &'static str,
}

impl MiniCarrier {
    pub fn new(wheel: &Wheel, center: (f64, f64), angle: f64, refs: CarrierRefs, supply: MiniSupply) -> Self {
        // The headers run across the module.
        let header_angle = (angle - 90.0).rem_euclid(360.0);
        let world = |local: (f64, f64)| rotate(center, local, angle);

        // Header locations are relative to the module's mechanical centre
        // (outline ~23.0 x 20.955 mm), numbering intact. The male-header
        // footprint itself is used: KiCad's 2xN socket puts its second column
        // on the other side of pad 1, which would put one hole column outside
        // the Mini.
        let (cx, cy) = (-2.9845, -7.3035);
        let (px, py) = (-1.9685, 9.0805);
        let gnd = world((9.3345, -4.7625));
        let vbat = world((9.3345, 0.1905));
        let mid = ((gnd.x + vbat.x) / 2.0, (gnd.y + vbat.y) / 2.0);

        let control = MiniControl {
            gnd: supply.gnd,
            in1: wheel.in1,
            gnd2: supply.gnd,
            in2: wheel.in2,
            in3: wheel.in3,
            en: wheel.enable,
            nfault: wheel.nfault,
        };
        let phase = MiniPhase { m3: wheel.m3, m2: wheel.m2, m1: wheel.m1 };
        Self {
            center,
            angle,
            control: ControlHeader { side: wheel.name, nets: control }
                .place(refs.control, world((cx + 5.08, cy - 1.27)))
                .rotate(header_angle),
            phase: PhaseHeader { side: wheel.name, nets: phase }
                .place(refs.phase, world((px + 2.54, py)))
                .rotate(header_angle),
            power: PowerPins {
                side: wheel.name,
                nets: supply,
                offsets: [gnd, vbat].map(|p| (p.x - mid.0, p.y - mid.1)),
                horizontal: angle == 0.0 || angle == 180.0,
            }
            .place(refs.power, mid),
        }
    }
}

impl Module for MiniCarrier {
    fn layout(&self) -> Group {
        let corners = [(-11.5, -10.4775), (11.5, -10.4775), (11.5, 10.4775), (-11.5, 10.4775)];
        group(items![
            &self.control,
            &self.phase,
            &self.power,
            // The permanently mounted module's envelope, with the same caption
            // on both modules so their rotated outlines read identically; the
            // caption reads up the module.
            outline(Layer::FSilk, corners.map(|c| At::from(rotate(self.center, c, self.angle))), 0.25),
            text("SIMPLEFOC MINI v1.0 / SOLDER", self.center).size(0.62).rotate(self.angle + 180.0),
        ])
    }
}

// --- Layout patterns ---------------------------------------------------------------

/// A short ground track from a pad to its own via into the back plane.
pub fn ground_stub(start: At, end: At, width: f64, (diameter, drill): (f64, f64)) -> Route {
    route(GND).width(width).via_size(diameter, drill).to(start).to(end).via(Layer::BCu)
}

/// Ground stubs with 0.7/0.3 mm vias: from each return pad (or point) to
/// its own via at `end`, `width` wide.
pub fn ground_stubs<const N: usize>(stubs: [(At, At, f64); N]) -> [Route; N] {
    stubs.map(|(start, end, width)| ground_stub(start, end, width, (0.7, 0.3)))
}

/// Six small vias above and below an exposed pad at `center`, each tied back
/// to it by a short spoke into the B.Cu ground plane.
pub fn thermal_vias(center: (f64, f64), reach: f64) -> Vec<Route> {
    let (cx, cy) = center;
    let mut out = Vec::new();
    for x in [cx - 0.65, cx, cx + 0.65] {
        for y in [cy - reach, cy + reach] {
            out.push(ground_stub(at(cx, cy), at(x, y), 0.4, (0.6, 0.3)));
        }
    }
    out
}

/// A durable silkscreen box around a functional block, captioned inside.
pub fn labelled_block(
    top_left: (f64, f64),
    bottom_right: (f64, f64),
    caption: &str,
    caption_at: (f64, f64),
) -> Vec<Item> {
    let ((x1, y1), (x2, y2)) = (top_left, bottom_right);
    items![
        outline(Layer::FSilk, [at(x1, y1), at(x2, y1), at(x2, y2), at(x1, y2)], 0.3),
        text(caption, caption_at).size(0.64),
    ]
}
