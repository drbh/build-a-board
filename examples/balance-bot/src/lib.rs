//! Balance Bot devboard: unified 100 x 100 mm, two-layer power and control board.
//!
//! The board is a value built from typed blocks. Each block is a struct
//! whose fields are its placed parts; its layout is a group in its own
//! frame. Routes reach pads through those parts' handles (`u1.sw()`,
//! `esp.right.on(A_R)`), so nothing can name a part or pin that does not
//! exist, and nothing depends on declaration order.

// Blocks are placed, specific things; `Block::new()` reads better than a
// `Default` that suggests a neutral value.
#![allow(clippy::new_without_default)]

pub mod control;
pub mod nets;
pub mod parts;
pub mod power;
mod routes;

use bb_sdk::*;

use control::*;
use nets::*;
use parts::*;
use power::*;

/// Every block on the board, each holding its placed parts.
pub struct Devboard {
    esp: Esp32,
    /// J10 (left) and J13 (right).
    encoders: [Encoder; 2],
    i2c: I2cAndFaults,
    pulldowns: Pulldowns,
    servo_buck: ServoBuck,
    control_buck: ControlBuck,
    probes: Probes,
    power: PowerEntry,
    drives: Drives,
}

impl Devboard {
    pub fn new() -> Self {
        let probes = Probes::new();
        Self {
            esp: Esp32::new(),
            encoders: [Encoder::new("J10", &LEFT, (32.0, 67.4)), Encoder::new("J13", &RIGHT, (94.0, 67.4))],
            i2c: I2cAndFaults::new(),
            pulldowns: Pulldowns::new(&[
                ("R14", SERVO_L, (0.0, 0.0), 0.0),
                ("R15", IN1_L, (4.0, 0.0), 0.0),
                ("R16", IN2_L, (8.0, 0.0), 0.0),
                ("R17", IN3_L, (12.0, 0.0), 0.0),
                ("R18", EN, (16.0, 0.0), 180.0), // GND pad faces the bus, not EN
                ("R19", IN1_R, (2.0, 4.0), 0.0),
                ("R20", IN2_R, (6.0, 4.0), 0.0),
                ("R21", IN3_R, (10.0, 4.0), 0.0),
                ("R22", EN_R, (14.0, 4.0), 180.0),
            ]),
            servo_buck: ServoBuck::new(&probes.v6_source),
            control_buck: ControlBuck::new(),
            probes,
            power: PowerEntry::new(),
            drives: Drives::new(),
        }
    }

    pub fn board(&self) -> Board {
        Board::new(project())
            .no_connect(NO_CONNECT)
            .netclasses([netclass("Battery", 1.5, BATTERY).via(1.0, 0.5), netclass("MotorPhase", 0.5, MOTOR_PHASES)])
            // Courtyard clashes are reviewed in the assembly render; electrical,
            // spacing, outline, and connectivity checks still gate release.
            .severities(
                ["courtyards_overlap", "pth_inside_courtyard", "npth_inside_courtyard"].map(|rule| (rule, "warning")),
            )
            // Designators give way to purpose-written legends on connectors,
            // mechanical parts, and probes (at a test pad the useful legend is its net).
            .hide_references(["J", "H", "FID", "FC", "FH", "TP"])
            .finish("Black", "White", "HASL lead-free")
            // Once routed, the board-local generated parts drop their virtual
            // library link and designators, as on the reviewed release.
            .unlink_after_route(LIB)
            .items(items![
                mechanical(),
                self.esp.layout(),
                self.encoders.each_ref().map(|e| e.layout()),
                text("<- J10 ENC L", (47.0, 67.4)).size(0.52), // C7 hides the area below J10
                text("J13 ENC R", (100.0, 70.8)).size(0.52),
                self.i2c.layout(),
                self.pulldowns.at(66, 62),
                self.servo_buck.at(78, 91),
                self.control_buck.at(44, 88),
                self.probes.layout(),
                self.power.layout(),
                self.drives.layout(),
                planes(),
                self.power_routes(),
                self.ground_routes(),
                self.signal_routes(),
                branding(),
            ])
    }
}

/// Board edge, mounting holes, and fiducials.
fn mechanical() -> Vec<Item> {
    items![
        outline(Layer::EdgeCuts, [at(20, 20), at(120, 20), at(120, 120), at(20, 120)], 0.1),
        [("H1", (24, 24)), ("H2", (116, 24)), ("H3", (24, 116)), ("H4", (116, 116))]
            .map(|(reference, at)| MountingHole.place(reference, at)),
        [("FID1", (30, 26)), ("FID2", (110, 26)), ("FID3", (115, 98))]
            .map(|(reference, at)| Fiducial.place(reference, at)),
    ]
}

fn planes() -> Vec<Item> {
    items![
        // Back ground plane, notched under the ESP32 antenna.
        zone(
            GND,
            Layer::BCu,
            [
                at(20.5, 20.5),
                at(52, 20.5),
                at(52, 37),
                at(95, 37),
                at(95, 20.5),
                at(119.5, 20.5),
                at(119.5, 119.5),
                at(20.5, 119.5)
            ],
        ),
        zone(VBAT, Layer::FCu, [at(50, 75), at(118, 75), at(118, 105), at(50, 105)]).priority(1),
    ]
}

/// Title, credits, and cat portraits: small on the front corners beside the
/// title, large on the otherwise text-free back.
fn branding() -> Vec<Item> {
    let mut cats = Vec::new();
    for (name, image, front_x, back_x) in [("BIGGIE", "biggie.png", 35.0, 45.0), ("JAMEO", "jameo.png", 105.0, 95.0)] {
        for (side, at, width, layer) in
            [("FRONT", (front_x, 29.5), 12.5, Layer::FSilk), ("BACK", (back_x, 70.0), 43.0, Layer::BSilk)]
        {
            let reference = format!("LOGO_{name}_{side}");
            cats.push(
                artwork(&reference, image, at, width)
                    .layer(layer)
                    .value(format!("{image} / WHITE SILK"))
                    .footprint(format!("{LIB}:Cat_Silkscreen_{reference}")),
            );
        }
    }
    items![
        cats,
        text("BALANCE BOT", (70, 22.6)).size(1.45),
        text("DAVID HOLTZ  /  drbh", (70, 25.2)).size(0.68),
        text("ESP32 ANTENNA -> BOARD EDGE / KEEP CLEAR", (73.5, 30.2)).size(0.72),
        text("BALANCE BOT  /  DAVID HOLTZ  /  drbh", (70, 118)).size(0.72),
    ]
}

/// Routed tracks get their net's minimum width, except the VBAT input leads
/// inside the two regulator blocks, which stay 1.0 mm to clear the pads.
fn route_width(t: &RoutedTrack) -> Option<f64> {
    if t.is_on(VBAT) {
        let (xs, ys) = ((t.start.x, t.end.x), (t.start.y, t.end.y));
        let near_6v = xs.0.max(xs.1) > 70.0 && ys.0.min(ys.1) > 84.0 && ys.0.max(ys.1) < 96.0;
        let near_5v = xs.0.min(xs.1) > 35.0 && xs.0.max(xs.1) < 45.0 && ys.0.min(ys.1) > 83.0 && ys.0.max(ys.1) < 92.0;
        if near_6v || near_5v {
            return Some(1.0);
        }
    }
    let wanted = if t.is_on(VBAT_RAW) {
        1.0
    } else if t.is_on(VBAT) {
        1.5
    } else if MOTOR_PHASES.iter().any(|&net| t.is_on(net)) {
        0.5
    } else {
        return None;
    };
    (t.width < wanted).then_some(wanted)
}

fn project() -> Project {
    Project::new("balance-bot-devboard")
        .lcsc([
            ("AP64352SP-13", "C2071665"),
            ("CYA0850-10UH", "C19268645"),
            ("GRM21BR61H106KE43L", "C440198"),
            ("CL31A226KAHNNNE", "C12891"),
            ("CC0603KRX7R9BB104", "C14663"),
            ("0603WAF1203T5E", "C25808"),
            ("0603WAF1002T5E", "C25804"),
            ("0603WAF2002T5E", "C4184"),
            ("0603WAF2003T5E", "C25811"),
            ("0603WAF4701T5E", "C23162"),
            ("BXJ25V470M10*10 10.0TP", "C3002464"),
        ])
        .order_quantity(5)
        .checks(["pincheck", "simcheck", "boardcheck", "jlccheck"])
        .release_check("boardcheck")
        .contract("boardcheck/contract.json")
        .release_files([
            "Cargo.toml",
            "Cargo.lock",
            "src",
            "Makefile",
            "README.md",
            "requirements.txt",
            "biggie.png",
            "jameo.png",
            "bom",
            "docs",
            "tools",
        ])
        .decisions([
            ("dual_servo_stall", "Accepted: protected 6 V rail may droop and servos may brown out"),
            ("module_footprints", "Verified against the intended physical modules"),
        ])
        .schematic(Schematic {
            title: Some("Balance Bot Devboard".into()),
            company: Some("Balance Bot devboard".into()),
            comment: Some("Generated from routed PCB connectivity".into()),
            library: Some(LIB.into()),
            ..Schematic::default()
        })
}

fn build() -> Board {
    Devboard::new().board()
}

bb_sdk::export_board!(build, route_width);

/// Design checks: `cargo test` in this directory.
#[cfg(test)]
mod tests {
    use super::*;

    fn near(actual: Volts, target: f64, percent: f64) -> bool {
        (actual.0 - target).abs() <= target * percent / 100.0
    }

    #[test]
    fn regulators_hit_their_rails() {
        let board = Devboard::new();
        let servo = board.servo_buck.feedback.vout();
        let control = board.control_buck.feedback.vout();
        assert!(near(servo, 6.0, 1.0), "servo rail is {servo:?}");
        assert!(near(control, 5.0, 1.0), "control rail is {control:?}");
    }

    #[test]
    fn regulators_share_a_switching_frequency() {
        let board = Devboard::new();
        assert_eq!(board.servo_buck.timing.part.value, board.control_buck.timing.part.value);
    }

    #[test]
    fn every_actuator_command_has_a_pulldown() {
        let board = Devboard::new();
        for wheel in [LEFT, RIGHT] {
            for net in [wheel.in1, wheel.in2, wheel.in3, wheel.enable] {
                assert_eq!(board.pulldowns.on(net).part.nets.p2, GND);
            }
        }
        board.pulldowns.on(SERVO_L);
    }

    /// Tracks marked to survive routing, at any depth.
    fn kept(items: &[Item]) -> usize {
        items
            .iter()
            .map(|item| match item {
                Item::Track(t) => usize::from(t.keep != Keep::No),
                Item::Group(g) => kept(&g.items),
                _ => 0,
            })
            .sum()
    }

    #[test]
    fn the_board_builds() {
        assert_eq!(kept(&build().items), 4);
    }
}
