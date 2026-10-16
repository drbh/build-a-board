//! A USB-C to 5 V UART breakout: CH340C bridge, ESD protection, and an
//! FTDI-style six-pin header (GND, CTS, VCC, TXD, RXD, DTR).
//!
//! The board is a USB 2.0 sink: 5.1k pull-downs on CC1 and CC2 ask the host
//! for default 5 V power.

// Blocks are specific placed things; `new()` reads better than `Default`.
#![allow(clippy::new_without_default)]

use bb_sdk::*;

bb_sdk::nets! {
    VBUS = "VBUS",
    GND = "GND",
    CC1 = "CC1",
    CC2 = "CC2",
    /// USB data, connector to ESD to bridge.
    DP = "D+",
    DM = "D-",
    /// The CH340C's internal 3.3 V regulator output.
    V3 = "V3",
    TXD = "TXD",
    RXD = "RXD",
    CTS = "CTS",
    DTR = "DTR",
}

// --- Parts -----------------------------------------------------------------------

part! {
    /// A 16-pin USB 2.0 Type-C receptacle (HRO TYPE-C-31-M-12). Power,
    /// ground, and each data line sit on pads for both plug orientations;
    /// the shield is four tabs.
    pub struct UsbC => UsbCPins {
        gnd: ["A1", "A12", "B1", "B12"],
        vbus: ["A4", "A9", "B4", "B9"],
        cc1: "A5",
        cc2: "B5",
        dp: ["A6", "B6"],
        dm: ["A7", "B7"],
        sbu1: "A8",
        sbu2: "B8",
        shield: "SH",
    }
    footprint = "Connector_USB:USB_C_Receptacle_HRO_TYPE-C-31-M-12", value = "USB-C 16P"
}

part! {
    /// ST USBLC6-2SC6 two-line ESD protection, SOT-23-6. Each line passes
    /// through the package (pins 1-6 and 3-4) so it can be routed straight;
    /// the two lines are interchangeable.
    pub struct Usblc6 => Usblc6Pins {
        io1: 1,
        gnd: 2,
        io2: 3,
        io2_through: 4,
        vbus: 5,
        io1_through: 6,
    }
    footprint = "Package_TO_SOT_SMD:SOT-23-6", value = "USBLC6-2SC6"
}

part! {
    /// WCH CH340C USB to UART bridge with internal oscillator, SOIC-16.
    pub struct Ch340c => Ch340cPins {
        gnd: 1,
        txd: 2,
        rxd: 3,
        /// 3.3 V regulator output; decouple with 100 nF when VCC is 5 V.
        v3: 4,
        ud_plus: 5,
        ud_minus: 6,
        nc7: 7,
        nc8: 8,
        cts: 9,
        dsr: 10,
        ri: 11,
        dcd: 12,
        dtr: 13,
        rts: 14,
        r232: 15,
        vcc: 16,
    }
    footprint = "Package_SO:SOIC-16_3.9x9.9mm_P1.27mm", value = "CH340C"
}

// --- The board -------------------------------------------------------------------

/// Everything on the breakout, by function.
pub struct Breakout {
    pub usb: Placed<UsbC>,
    /// R1, R2: 5.1k on CC1 and CC2 mark the board as a sink.
    pub cc_pulldowns: [Placed<Resistor>; 2],
    pub esd: Placed<Usblc6>,
    pub bridge: Placed<Ch340c>,
    pub bulk: Placed<Capacitor>,
    pub vcc_decoupling: Placed<Capacitor>,
    pub v3_decoupling: Placed<Capacitor>,
    pub header: Placed<Connector>,
}

impl Breakout {
    pub fn new() -> Self {
        let bridge = Ch340c {
            gnd: GND,
            txd: TXD,
            rxd: RXD,
            v3: V3,
            ud_plus: DP,
            ud_minus: DM,
            nc7: NC,
            nc8: NC,
            cts: CTS,
            dsr: NC,
            ri: NC,
            dcd: NC,
            dtr: DTR,
            rts: NC,
            r232: NC,
            vcc: VBUS,
        };
        // D- on the upper channel, D+ on the lower, matching the connector fan-out.
        let esd = Usblc6 { io1: DM, gnd: GND, io2: DP, io2_through: DP, vbus: VBUS, io1_through: DM };
        let usb = UsbC { gnd: GND, vbus: VBUS, cc1: CC1, cc2: CC2, dp: DP, dm: DM, sbu1: NC, sbu2: NC, shield: GND };
        let cc = |net| resistor(kohm(5.1), net, GND).tolerance(1);
        let decoupling = |net| capacitor(nf(100), net, GND).rated(v(16));
        Self {
            // Rotated so the receptacle opens over the left edge.
            usb: usb.place("J1", (23.6, 31)).rotate(270),
            cc_pulldowns: [cc(CC1).place("R1", (30.5, 25.5)), cc(CC2).place("R2", (30.5, 36.5))],
            esd: esd.place("U2", (31.5, 31)),
            bridge: bridge.place("U1", (40, 31)),
            bulk: capacitor(uf(10), VBUS, GND).rated(v(16)).footprint(C0805).place("C1", (34.5, 25.5)),
            vcc_decoupling: decoupling(VBUS).place("C2", (41.2, 24.2)),
            v3_decoupling: decoupling(V3).place("C3", (35.3, 28.2)).rotate(90),
            header: connector("FTDI 6P", "Connector_PinHeader_2.54mm:PinHeader_1x06_P2.54mm_Vertical")
                .numbered([GND, CTS, VBUS, TXD, RXD, DTR])
                .place("J2", (47.5, 24.65)),
        }
    }
}

impl Module for Breakout {
    fn layout(&self) -> Group {
        let (usb, esd, header) = (&self.usb, &self.esd, &self.header);
        // The receptacle's pads form one column; the loops that join each
        // data line's two pads run just behind it (under the body) or in
        // front of it, in line with the pads.
        let (a6, b6, a7, b7) = (usb.dp().pad("A6"), usb.dp().pad("B6"), usb.dm().pad("A7"), usb.dm().pad("B7"));
        let (a9, b9) = (usb.vbus().pad("A9"), usb.vbus().pad("B9"));
        let (behind, ahead, out) = ((-1.195, 0), (1.255, 0), (1.555, 0));
        group(items![
            // USB-C fan-out, by hand: D+ joins in a loop under the receptacle
            // body, D- just beside it, and each line runs straight through its
            // ESD channel.
            track(DP, [a6.clone(), a6 + behind, b6.clone() + behind, b6.clone()]).width(0.2),
            track(DP, [b6, esd.io2(), esd.io2_through()]).width(0.2),
            track(DM, [b7.clone(), b7.clone() + ahead, a7.clone() + ahead, a7]).width(0.2),
            track(DM, [b7 + ahead, esd.io1(), esd.io1_through()]).width(0.2),
            // VBUS: both pad pairs drop to vias and join on B.Cu, which runs
            // around the bottom and right edges (leaving the back free for
            // signals) to the header's 5V pin; the router taps it.
            route(VBUS).width(0.4).via_size(0.6, 0.3).to(a9.clone()).to(a9 + out).via(Layer::BCu),
            route(VBUS)
                .width(0.4)
                .via_size(0.6, 0.3)
                .to(b9.clone())
                .to(b9 + out)
                .via(Layer::BCu)
                .to(at(29.2, 38.9))
                .to(at(49.0, 38.9))
                .to(header.on(VBUS) + (1.5, 0))
                .to(header.on(VBUS)),
            // The data lines box in U2's ground pin; it drops to the plane
            // through a via under the package, between the two channels.
            route(GND).width(0.3).via_size(0.6, 0.3).to(esd.gnd()).to(esd.gnd() + (0.9375, 0)).via(Layer::BCu),
            usb,
            &self.cc_pulldowns,
            esd,
            &self.bridge,
            &self.bulk,
            &self.vcc_decoupling,
            &self.v3_decoupling,
            header,
            header
                .legends_column(44.6, [(1, "GND"), (2, "CTS"), (3, "5V"), (4, "TXD"), (5, "RXD"), (6, "DTR")])
                .size(0.7),
        ])
    }
}

fn build() -> Board {
    let corners = [(20, 22), (50, 22), (50, 40), (20, 40)];
    Board::new(Project::new("usb-uart").release_files(["Cargo.toml", "Cargo.lock", "src"]))
        // The receptacle's pads sit 0.2 mm apart, so the board runs at a
        // 0.15 mm clearance throughout.
        .rules(rules(0.2, 0.15).via(0.6, 0.3))
        // FRICTION: there is no differential-pair rule; D+/D- are routed by
        // hand above and get no coupled spacing or length matching.
        .netclasses([netclass("Power", 0.4, [VBUS, GND]).clearance(0.15)])
        .severities([("courtyards_overlap", "warning")])
        // The receptacle's 0.5 mm pitch cannot escape the router's default 0.7 mm.
        .router_clearance(0.15)
        .hide_references(["J"])
        .items(items![
            outline(Layer::EdgeCuts, corners.map(|(x, y)| at(x, y)), 0.1),
            Breakout::new().layout(),
            zone(GND, Layer::BCu, [at(20.5, 22.5), at(49.5, 22.5), at(49.5, 39.5), at(20.5, 39.5)]),
            text("USB-UART CH340C", (34, 38.6)).size(0.8),
        ])
}

bb_sdk::export_board!(build);

/// Design checks: `cargo test` in this directory.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cc_pulldowns_identify_a_sink() {
        let board = Breakout::new();
        for (r, cc) in board.cc_pulldowns.iter().zip([CC1, CC2]) {
            assert_eq!(r.part.value, kohm(5.1));
            assert_eq!((r.part.nets.p1, r.part.nets.p2), (cc, GND));
        }
    }

    #[test]
    fn both_plug_orientations_carry_data() {
        let usb = Breakout::new().usb.part;
        let on = |net| usb.pads().into_iter().filter(|(_, n)| *n == net).count();
        assert_eq!((on(DP), on(DM), on(VBUS), on(GND)), (2, 2, 4, 5));
    }

    #[test]
    fn data_lines_pass_through_the_esd_part() {
        let esd = Breakout::new().esd.part;
        assert_eq!((esd.io1, esd.io1_through), (DM, DM));
        assert_eq!((esd.io2, esd.io2_through), (DP, DP));
    }

    #[test]
    fn unused_bridge_pins_are_left_unwired() {
        let bridge = Breakout::new().bridge.part;
        assert!(
            [bridge.dsr, bridge.ri, bridge.dcd, bridge.rts, bridge.r232, bridge.nc7, bridge.nc8]
                .iter()
                .all(|n| n.is_nc())
        );
        let wired: Vec<String> = bridge.pads().into_iter().map(|(pad, _)| pad).collect();
        assert_eq!(wired, ["1", "2", "3", "4", "5", "6", "9", "13", "16"]);
    }
}
