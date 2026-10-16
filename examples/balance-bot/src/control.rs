//! Control: the ESP32 sockets, wheel drives, encoders, I2C, and probes.

use bb_sdk::*;

use crate::nets::*;
use crate::parts::*;

/// The ESP32 dev module plugs into two 15-pin rows. Its antenna points at the
/// top edge, over a matching notch in the ground plane.
pub struct Esp32 {
    /// J7, pin 1 = 5V.
    pub left: Placed<Connector>,
    /// J8, pin 1 = 3V3.
    pub right: Placed<Connector>,
}

impl Esp32 {
    pub fn new() -> Self {
        let socket = "Connector_PinSocket_2.54mm:PinSocket_1x15_P2.54mm_Vertical";
        Self {
            left: connector("ESP32 LEFT", socket).numbered(ESP_LEFT).place("J7", (61, 37)),
            right: connector("ESP32 RIGHT", socket).numbered(ESP_RIGHT).place("J8", (86.4, 37)),
        }
    }
}

impl Module for Esp32 {
    fn layout(&self) -> Group {
        group(items![
            &self.left,
            &self.right,
            text("J7 ESP32 LEFT ROW", (54.5, 35)).size(0.65),
            text("PIN 1 = 5V", (56.5, 38)).size(0.6).rotate(90),
            text("J8 ESP32 RIGHT ROW", (93, 35)).size(0.65),
            text("PIN 1 = 3V3", (91, 38)).size(0.6).rotate(90),
        ])
    }
}

/// JST XH encoder input. Both sides share pin order and orientation so one
/// hand-built cable fits either; the right side's A/B/C therefore cross on
/// their way from J8 (see the fixed fan-out).
pub struct Encoder {
    pub header: Placed<Connector>,
    legends_y: f64,
}

impl Encoder {
    pub fn new(reference: &str, wheel: &Wheel, at: (f64, f64)) -> Self {
        let value = format!("ENCODER {} / JST XH2.5", wheel.letter);
        Self {
            header: connector(value, "Connector_JST:JST_XH_B5B-XH-A_1x05_P2.50mm_Vertical")
                .numbered([GND, wheel.a, wheel.b, wheel.c, V3_3])
                .place(reference, at),
            legends_y: at.1 - 3.6,
        }
    }
}

impl Module for Encoder {
    fn layout(&self) -> Group {
        let legends = [(1, "GND"), (2, "A"), (3, "B"), (4, "C"), (5, "3V3")];
        group(items![&self.header, self.header.legends_row(self.legends_y, legends)])
    }
}

/// IMU header, I2C pull-ups, and pull-ups on the SimpleFOC nFAULT lines
/// (open-drain, so they must be valid while the ESP32 is in reset).
pub struct I2cAndFaults {
    pub imu: Placed<Connector>,
    pub scl_pullup: Placed<Resistor>,
    pub sda_pullup: Placed<Resistor>,
    pub fault_pullups: [Placed<Resistor>; 2],
}

impl I2cAndFaults {
    pub fn new() -> Self {
        let i2c = |net| resistor(kohm(4.7), V3_3, net).footprint(R0603_HAND);
        let fault = |net| resistor(kohm(10), V3_3, net).tolerance(1);
        Self {
            imu: connector("IMU / MALE", "Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Vertical")
                .numbered([GND, SCL, SDA, V3_3])
                .place("J11", (48.19, 78))
                .rotate(90),
            scl_pullup: i2c(SCL).place("R10", (63, 78)),
            sda_pullup: i2c(SDA).place("R11", (72, 78)),
            fault_pullups: [
                fault(NFAULT_L).place("R12", (74.5, 78.192)).rotate(270),
                fault(NFAULT_R).place("R13", (77, 78.192)).rotate(270),
            ],
        }
    }
}

impl Module for I2cAndFaults {
    fn layout(&self) -> Group {
        let [r12, _] = &self.fault_pullups;
        group(items![
            &self.imu,
            self.imu.legends_row(80.5, [(1, "GND"), (2, "SCL"), (3, "SDA"), (4, "3V3")]),
            &self.scl_pullup,
            &self.sda_pullup,
            &self.fault_pullups,
            // Local fixed join, ending at R11's 3V3 pad; avoids version-specific
            // autorouter endpoint misses.
            track(V3_3, [r12.p1(), at(74.5, 75.5), at(70.0, 75.5), at(70.0, 78.0), at(71.175, 78.0)]).width(0.25),
        ])
    }
}

/// Physical pull-downs make every actuator command deterministic while the
/// ESP32 is in reset, independent of the attached module. Each returns to the
/// back plane through its own via rather than a shared bus between pads.
pub struct Pulldowns {
    pub resistors: Vec<Placed<Resistor>>,
}

/// A pull-down: its reference, signal, and position and rotation in the
/// array's frame.
pub type Pulldown = (&'static str, Net, (f64, f64), f64);

impl Pulldowns {
    pub fn new(signals: &[Pulldown]) -> Self {
        Self {
            resistors: signals
                .iter()
                .map(|&(reference, net, at, angle)| {
                    resistor(kohm(10), net, GND).tolerance(1).place(reference, at).rotate(angle)
                })
                .collect(),
        }
    }

    /// The pull-down on `net`.
    #[track_caller]
    pub fn on(&self, net: Net) -> &Placed<Resistor> {
        self.resistors.iter().find(|r| r.part.nets.p1 == net).unwrap_or_else(|| panic!("no pull-down on {net}"))
    }
}

impl Module for Pulldowns {
    fn layout(&self) -> Group {
        // Each ground via sits 1.5 mm below its resistor's GND pad.
        let stubs = self.resistors.iter().map(|r| items![r, ground_stub(r.p2(), r.p2() + (0, 1.5), 0.3, (0.6, 0.3))]);
        group(stubs.collect::<Vec<_>>()).name("startup pull-downs")
    }
}

/// An unpopulated probe pad and its legend, declared together so the two
/// cannot drift apart.
pub struct Probe {
    pub pad: Placed<TestPoint>,
    legend: Text,
}

impl std::ops::Deref for Probe {
    type Target = Placed<TestPoint>;

    fn deref(&self) -> &Placed<TestPoint> {
        &self.pad
    }
}

fn probe(reference: &str, net: Net, at: (f64, f64), legend: &str, legend_at: (f64, f64)) -> Probe {
    Probe { pad: TestPoint { point: net }.place(reference, at), legend: text(legend, legend_at).size(0.6) }
}

/// Accessible probe pads. TP4 and TP5 are both 6V on purpose: regulator
/// output and the far end of the servo trunk, so a stall's droop can be split
/// into regulator sag and trunk IR drop.
pub struct Probes {
    pub v5: Probe,
    pub gnd: Probe,
    pub raw: Probe,
    pub battery: Probe,
    pub v3_3: Probe,
    pub fault_left: Probe,
    pub fault_right: Probe,
    pub v6_source: Probe,
    pub v6_load: Probe,
}

impl Probes {
    pub fn new() -> Self {
        Self {
            v5: probe("TP6", V5, (60.6, 82.0), "5V", (60.6, 80.0)),
            gnd: probe("TP1", GND, (68.0, 82.0), "GND", (68.0, 80.0)),
            raw: probe("TP2", VBAT_RAW, (71.2, 82.0), "RAW", (71.2, 80.0)),
            battery: probe("TP3", VBAT, (74.4, 82.0), "BAT", (74.4, 80.0)),
            v3_3: probe("TP7", V3_3, (87.2, 82.0), "3V3", (87.2, 80.0)),
            fault_left: probe("TP8", NFAULT_L, (90.4, 82.0), "FL", (90.4, 80.0)),
            fault_right: probe("TP9", NFAULT_R, (93.6, 82.0), "FR", (93.6, 80.0)),
            v6_source: probe("TP4", V6, (93.2, 89.095), "6V SRC", (92.4, 92.6)),
            v6_load: probe("TP5", V6, (84.0, 107.0), "6V LOAD", (84.0, 105.0)),
        }
    }

    fn all(&self) -> [&Probe; 9] {
        [
            &self.v5,
            &self.gnd,
            &self.raw,
            &self.battery,
            &self.v3_3,
            &self.fault_left,
            &self.fault_right,
            &self.v6_source,
            &self.v6_load,
        ]
    }
}

impl Module for Probes {
    fn layout(&self) -> Group {
        group(self.all().map(|p| items![&p.pad, p.legend.clone()]))
    }
}

/// Wheel drives (SimpleFOC Minis), motor connectors, and servo headers. XH
/// rather than PH on the phases: the Mini is 2 A continuous / 3 A peak per
/// phase and PH is rated 2 A.
pub struct Drives {
    pub left: MiniCarrier,
    pub right: MiniCarrier,
    pub motor_left: Placed<Connector>,
    pub motor_right: Placed<Connector>,
    pub servo_left: Placed<Connector>,
    pub servo_right: Placed<Connector>,
}

impl Drives {
    pub fn new() -> Self {
        let motor = |side, wheel: &Wheel| {
            connector(format!("{side} MOTOR XH2.5"), "Connector_JST:JST_XH_B3B-XH-A_1x03_P2.50mm_Vertical")
                .numbered([wheel.m1, wheel.m2, wheel.m3])
        };
        let servo = |name, signal| {
            connector(name, "Connector_PinHeader_2.54mm:PinHeader_1x03_P2.54mm_Vertical").numbered([GND, V6, signal])
        };
        let supply = MiniSupply { gnd: GND, vbat: VBAT };
        let refs = |control, power, phase| CarrierRefs { control, power, phase };
        Self {
            left: MiniCarrier::new(&LEFT, (34.0, 50.0), -90.0, refs("J9", "J2", "J14"), supply),
            right: MiniCarrier::new(&RIGHT, (106.0, 50.0), -90.0, refs("J12", "J3", "J15"), supply),
            motor_left: motor("LEFT", &LEFT).place("J16", (24, 74)).rotate(270),
            motor_right: motor("RIGHT", &RIGHT).place("J17", (116, 78)).rotate(90),
            servo_left: servo("SERVO L", SERVO_L).place("J4", (103, 113)).rotate(90),
            servo_right: servo("SERVO R", SERVO_R).place("J5", (111, 113)).rotate(90),
        }
    }
}

impl Module for Drives {
    fn layout(&self) -> Group {
        let uvw = [(1, "U"), (2, "V"), (3, "W")];
        group(items![
            self.left.layout(),
            self.right.layout(),
            [(33.81, VBAT), (38.76, GND), (105.81, VBAT), (110.76, GND)]
                .map(|(x, net)| text(net.name(), (x, 62.0)).size(0.60)),
            [("M1", 45.49), ("M2", 48.03), ("M3", 50.57)]
                .map(|(name, y)| items![text(name, (28.0, y)).size(0.60), text(name, (100.0, y)).size(0.60)]),
            &self.motor_left,
            &self.motor_right,
            text("J16 MOTOR L", (25.6, 70.8)).size(0.52),
            text("J17 MOTOR R", (114.4, 70.8)).size(0.52),
            self.motor_left.legends_column(28.0, uvw).size(0.62),
            self.motor_right.legends_column(112.0, uvw).size(0.62),
            &self.servo_left,
            &self.servo_right,
            self.servo_left.legends_row(110, [(1, "GND"), (2, "6V"), (3, "PWM-L")]),
            self.servo_right.legends_row(110, [(1, "GND"), (2, "6V"), (3, "PWM-R")]),
        ])
    }
}
