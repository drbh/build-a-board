//! Power: battery entry, the two AP64352 bucks, and bulk capacitance.

use bb_sdk::*;

use crate::nets::*;
use crate::parts::*;

/// A buck's feedback divider: VOUT = VREF x (1 + top / bottom).
pub struct Feedback {
    /// From the output down to FB.
    pub top: Vec<Placed<Resistor>>,
    /// From FB down to ground.
    pub bottom: Vec<Placed<Resistor>>,
}

impl Feedback {
    pub fn vout(&self) -> Volts {
        let total = |chain: &[Placed<Resistor>]| chain.iter().map(|r| r.part.value).sum::<Ohms>().ohms();
        Volts(AP64352_VREF.0 * (1.0 + total(&self.top) / total(&self.bottom)))
    }
}

/// 1% feedback resistors in 0603.
fn precise(value: Ohms, p1: Net, p2: Net) -> Resistor {
    resistor(value, p1, p2).tolerance(1)
}

/// 10 uF / 50 V input decoupling, in 0805.
fn input_cap(p1: Net, p2: Net) -> Capacitor {
    capacitor(uf(10), p1, p2).rated(v(50)).footprint(C0805)
}

/// 22 uF / 25 V output capacitance, in 1206.
fn output_cap(rail: Net) -> Capacitor {
    capacitor(uf(22), rail, GND).rated(v(25)).footprint(C1206)
}

/// 100 nF / 50 V, in 0603.
fn small_cap(p1: Net, p2: Net) -> Capacitor {
    capacitor(nf(100), p1, p2).rated(v(50))
}

/// 6 V / 3.5 A servo supply, laid out around U1.
pub struct ServoBuck {
    pub u1: Placed<Ap64352>,
    pub l1: Placed<Cya0850>,
    /// C1, C2, C16.
    pub input: [Placed<Capacitor>; 3],
    /// C3, C4.
    pub output: [Placed<Capacitor>; 2],
    pub bootstrap: Placed<Capacitor>,
    pub soft_start: Placed<Capacitor>,
    pub timing: Placed<Resistor>,
    pub feedback: Feedback,
    /// TP4, the regulator-output probe the output trunk passes through.
    probe: At,
}

impl ServoBuck {
    pub fn new(probe: &Placed<TestPoint>) -> Self {
        let u1 = Ap64352 { bst: BST, vin: VBAT, en: VBAT, rt: RT, fb: FB, ss: SS, gnd: GND, sw: SW, ep: GND };
        Self {
            u1: u1.place("U1", (0.0, 0.0)),
            l1: inductor(SW, V6).place("L1", (10.5, -1.905)),
            input: [("C1", (-10.0, -1.5)), ("C2", (-6.5, 2.0)), ("C16", (-10.0, 4.5))]
                .map(|(reference, at)| input_cap(VBAT, GND).place(reference, at).rotate(90)),
            output: [("C3", (18.5, -4.5)), ("C4", (18.5, 0.7))]
                .map(|(reference, at)| output_cap(V6).place(reference, at)),
            bootstrap: small_cap(BST, SW).place("C5", (0, -6.5)),
            soft_start: small_cap(SS, GND).place("C9", (6, 6)),
            timing: precise(kohm(200), RT, GND).place("R4", (-2.5, 6)).rotate(270),
            feedback: Feedback {
                top: vec![
                    precise(kohm(120), V6, FB_TOP).place("R1", (14, 6)).rotate(180),
                    precise(kohm(10), FB_TOP, FB).place("R2", (11, 6)).rotate(180),
                ],
                bottom: vec![precise(kohm(20), FB, GND).place("R3", (8, 6)).rotate(180)],
            },
            probe: probe.point(),
        }
    }
}

impl Module for ServoBuck {
    fn layout(&self) -> Group {
        let Self { u1, l1, input: [c1, c2, c16], output: [c3, c4], bootstrap: c5, soft_start: c9, timing: r4, .. } =
            self;
        let (r1, r2, r3) = (&self.feedback.top[0], &self.feedback.top[1], &self.feedback.bottom[0]);
        let tp4 = &self.probe;
        group(items![
            u1,
            l1,
            [c1, c2, c16, c3, c4, c5, c9],
            [r1, r2, r3, r4],
            track(SW, [u1.sw(), at(4.3, -1.905), l1.p1()]).width(1.0),
            track(VBAT, [u1.vin(), at(-3.5, -0.635), at(-6.5, -0.55)]).width(1.0),
            track(VBAT, [u1.en(), at(-4.5, 0.635), at(-4.5, 2.95), c2.p1()]).width(1.0),
            // The router may thin or drop the probe's segment of the trunk; it is put back.
            track(V6, [l1.p2(), tp4.clone()]).width(1.5),
            track(V6, [tp4.clone(), at(15.2, -4.5)]).width(1.5).keep(),
            track(V6, [at(15.2, -4.5), c3.p1()]).width(1.5),
            track(V6, [tp4.clone(), at(15.2, 0.7), c4.p1()]).width(1.5),
            track(BST, [u1.bst(), at(-2.5, -6.5)]).width(0.35).keep(),
            track(BST, [at(-2.5, -6.5), c5.p1()]).width(0.35),
            track(SW, [c5.p2(), at(2.5, -6.5), at(2.5, -1.905)]).width(0.35),
            track(RT, [u1.rt(), r4.p1()]).width(0.3),
            track(SS, [u1.ss(), at(5.225, 0.635), c9.p1()]).width(0.3),
            track(FB, [u1.fb(), at(3.5, 1.905), at(3.5, 8.2), at(8.825, 8.2), r3.p1()]).width(0.25),
            track(FB, [r3.p1(), r2.p2()]).width(0.25),
            track(FB_TOP, [r2.p1(), r1.p2()]).width(0.25),
            track(V6, [r1.p1(), at(15.2, 6), at(15.2, 0.7)]).width(0.25),
            ground_stubs([
                (u1.ep(), at(0.0, 3.0), 0.8),
                (u1.gnd(), at(4.3, -0.635), 0.5),
                (c2.p2(), at(-8.2, 1.05), 1.0),
                (c3.p2(), at(21.3, -4.5), 1.0),
                (c4.p2(), at(21.3, 0.7), 1.0),
                (c9.p2(), at(6.775, 7.3), 0.5),
                (r4.p2(), at(-2.5, 8.0), 0.5),
            ]),
            thermal_vias((0.0, 0.0), 2.25),
            labelled_block((-11.2, -10.6), (22.7, 9.4), "6V SERVO REGULATOR", (5.75, 8.7)),
        ])
        .name("6V servo buck")
    }
}

/// 5 V control supply (same AP64352 as the servo buck), laid out around U2.
pub struct ControlBuck {
    pub u2: Placed<Ap64352>,
    pub l2: Placed<Cya0850>,
    /// C10, C11, C17.
    pub input: [Placed<Capacitor>; 3],
    /// C12, C13.
    pub output: [Placed<Capacitor>; 2],
    pub bootstrap: Placed<Capacitor>,
    pub soft_start: Placed<Capacitor>,
    pub timing: Placed<Resistor>,
    pub feedback: Feedback,
}

impl ControlBuck {
    pub fn new() -> Self {
        let u2 = Ap64352 { bst: BST5, vin: VBAT, en: VBAT, rt: RT5, fb: FB5, ss: SS5, gnd: GND, sw: SW5, ep: GND };
        Self {
            u2: u2.place("U2", (0.0, 0.0)),
            l2: inductor(SW5, V5).place("L2", (11.5, -1.905)),
            input: [("C10", -2.5), ("C11", 1.2), ("C17", 4.5)]
                .map(|(reference, y)| input_cap(VBAT, GND).place(reference, (-6.0, y)).rotate(180)),
            output: [("C12", (19, -3.8)), ("C13", (19, 0.2))]
                .map(|(reference, at)| output_cap(V5).place(reference, at)),
            bootstrap: small_cap(BST5, SW5).place("C14", (0, -5.5)),
            soft_start: small_cap(GND, SS5).place("C15", (4, 5.5)).rotate(90),
            timing: precise(kohm(200), RT5, GND).place("R9", (-3, 5.5)).rotate(270),
            feedback: Feedback {
                top: vec![
                    precise(kohm(200), FB5_TOP, V5).place("R5", (17, 5.5)),
                    precise(kohm(10), FB5, FB5_TOP).place("R6", (14, 5.5)),
                ],
                bottom: vec![
                    precise(kohm(20), FB5_BOTTOM, FB5).place("R7", (11, 5.5)),
                    precise(kohm(20), GND, FB5_BOTTOM).place("R8", (8, 5.5)),
                ],
            },
        }
    }
}

impl Module for ControlBuck {
    fn layout(&self) -> Group {
        let Self {
            u2,
            l2,
            input: [c10, c11, c17],
            output: [c12, c13],
            bootstrap: c14,
            soft_start: c15,
            timing: r9,
            ..
        } = self;
        let (r5, r6) = (&self.feedback.top[0], &self.feedback.top[1]);
        let (r7, r8) = (&self.feedback.bottom[0], &self.feedback.bottom[1]);
        group(items![
            u2,
            l2,
            [c10, c11, c17, c12, c13, c14, c15],
            [r5, r6, r7, r8, r9],
            track(VBAT, [c17.p1(), c11.p1()]).width(1.0),
            track(GND, [c17.p2(), c11.p2()]).width(1.0),
            track(SW5, [u2.sw(), l2.p1()]).width(1.0),
            track(VBAT, [u2.vin(), at(-4.3, -0.635), c10.p1()]).width(1.0),
            track(VBAT, [at(-4.3, -0.635), at(-4.3, 1.2), c11.p1()]).width(1.0),
            track(VBAT, [u2.en(), u2.vin()]).width(0.35),
            track(V5, [l2.p2(), at(16.6, -1.905), at(16.6, -3.8), c12.p1()]).width(1.0),
            track(V5, [at(16.6, -1.905), at(16.6, 0.2), c13.p1()]).width(1.0),
            track(BST5, [u2.bst(), at(-2.5, -5.5), c14.p1()]).width(0.35),
            track(SW5, [c14.p2(), at(2.5, -5.5)]).width(0.35).keep(),
            track(SW5, [at(2.5, -5.5), at(2.5, -1.905)]).width(0.35),
            track(RT5, [u2.rt(), at(-3, 1.905), r9.p1()]).width(0.3),
            track(SS5, [u2.ss(), at(5, 0.635), at(5, 4.725), c15.p2()]).width(0.3),
            track(FB5_BOTTOM, [r8.p2(), r7.p1()]).width(0.25),
            track(FB5, [r7.p2(), r6.p1()]).width(0.25),
            track(FB5_TOP, [r6.p2(), r5.p1()]).width(0.25),
            track(V5, [r5.p2(), at(17.825, 4), at(15, 4), at(15, 0.2), c13.p1()]).width(0.4),
            ground_stubs([
                (u2.ep(), at(0.0, 3.0), 0.8),
                (u2.gnd(), at(4.2, -0.635), 0.5),
                (c10.p2(), at(-8.2, -2.5), 0.8),
                (c11.p2(), at(-8.2, 1.2), 0.8),
                // C12's and C13's ground pads, written as the points they sit at:
                // the reviewed board has them a nanometre short of the pads.
                (at(20.475, -3.8), at(21.5, -3.8), 0.8),
                (at(20.475, 0.2), at(21.5, 0.2), 0.8),
                (c15.p1(), at(4.0, 7.2), 0.4),
                (r8.p1(), at(7.175, 6.8), 0.4),
                (r9.p2(), at(-3.0, 7.2), 0.4),
            ]),
            // FB5 hops under the inductor on B.Cu.
            track(FB5, [u2.fb(), at(3.5, 2.5)]).width(0.25),
            route(FB5)
                .width(0.25)
                .via_size(0.7, 0.3)
                .to(at(3.5, 2.5))
                .via(Layer::BCu)
                .to(at(12.0, 3.5))
                .via(Layer::FCu)
                .to(r7.p2()),
            thermal_vias((0.0, 0.0), 2.25),
            labelled_block((-10.5, -7.6), (22.5, 12.4), "5V CONTROL REGULATOR", (6.0, 11.7)),
        ])
        .name("5V control buck")
    }
}

/// XT60 pigtail, fuse, TVS, and the bulk capacitors.
pub struct PowerEntry {
    pub battery: Placed<Connector>,
    pub fuse: Placed<FuseHolder>,
    pub tvs: Placed<Tvs>,
    /// C7, on VBAT.
    pub battery_bulk: Placed<VchipBulk>,
    /// C6 and C8 give the servo rail 940 uF at ~40 mohm.
    pub servo_bulk: [Placed<VchipBulk>; 2],
}

impl PowerEntry {
    pub fn new() -> Self {
        Self {
            // A male pigtail solders into the AMASS holes (rotated for
            // clearance); pad 1 is the keyed negative contact. No upright
            // XT60 body is fitted.
            battery: connector("XT60-M PIGTAIL / 4S INPUT", "Connector_AMASS:AMASS_XT60-M_1x02_P7.20mm_Vertical")
                .numbered([GND, VBAT_RAW])
                .models([])
                .place("J1", (25, 94))
                .rotate(90),
            fuse: FuseHolder { value: "MINI holder / F1 10A", nets: TwoPin { p1: VBAT_RAW, p2: VBAT } }
                .place_centred("FH1", (43.0, 111.0)),
            tvs: Tvs { part: "P6KE22A", nets: Diode { cathode: VBAT, anode: GND } }
                .place("D1", (54.16, 103))
                .rotate(180),
            battery_bulk: bulk(VBAT).place("C7", (38.0, 76.0)),
            servo_bulk: [bulk(V6).place("C6", (91.0, 111.0)), bulk(V6).place("C8", (108.5, 91.0))],
        }
    }
}

impl Module for PowerEntry {
    fn layout(&self) -> Group {
        let Self { battery: j1, fuse: fh1, .. } = self;
        group(items![
            j1,
            fh1,
            &self.tvs,
            &self.battery_bulk,
            &self.servo_bulk,
            text("J1 BATTERY PIGTAIL", (32.2, 89.0)).size(0.50).rotate(90),
            text("XT60-M PIGTAIL / 12.0-16.8 VDC", (35.4, 83.0)).size(0.50),
            text("GND(-)  |  VBAT_RAW(+)", (35.4, 84.3)).size(0.50),
            // Polarity right at the pads: a 4S pack, no reverse protection, and
            // no fuse until F1 is fitted make this the one connector not to guess.
            j1.legends_column(30.0, [(1, "-"), (2, "+")]).size(1.1),
            text("F1  10A MINI   VBAT_RAW -> VBAT", (43.0, 115.5)).size(0.6),
            // FH1 carries the whole battery current: bond both pins of each
            // terminal and leave on a 2 mm exit.
            track(VBAT_RAW, [fh1.p1(), fh1.p1() + (0, FuseHolder::PIN_PITCH)]).width(2.0),
            track(VBAT, [fh1.p2(), fh1.p2() + (0, FuseHolder::PIN_PITCH)]).width(2.0),
            track(VBAT, [fh1.p2(), at(49.4, 109.3), at(49.4, 106), at(52.0, 103.0)]).width(2.0),
        ])
    }
}
