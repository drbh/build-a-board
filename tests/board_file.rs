//! Board components build, run sandboxed, and resolve into the expected
//! design. Needs the `wasm32-wasip2` Rust target; skipped without it.

use std::path::{Path, PathBuf};

use build_a_board::board::runtime::wit;
use build_a_board::board::{BoardFile, resolve};
use build_a_board::design::{Layer, Source};
use build_a_board::kicad::Libraries;
use build_a_board::units::{Nm, Pt};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn footprints() -> PathBuf {
    root().join("tests/fixtures/footprints")
}

fn wasm_target() -> bool {
    let sysroot = std::process::Command::new("rustc").args(["--print", "sysroot"]).output();
    let found = sysroot.is_ok_and(|out| {
        Path::new(String::from_utf8_lossy(&out.stdout).trim()).join("lib/rustlib/wasm32-wasip2").is_dir()
    });
    if !found {
        eprintln!("skipping: the wasm32-wasip2 Rust target is not installed");
    }
    found
}

fn fixture() -> Option<BoardFile> {
    wasm_target()
        .then(|| BoardFile::load(&root().join("tests/fixtures/board-rs"), &footprints()).expect("fixture board"))
}

#[test]
fn records_project_and_parts() {
    let Some(board) = fixture() else { return };
    assert_eq!(board.project.name, "fixture");
    assert_eq!(board.project.lcsc, [("MPN".to_owned(), "C1".to_owned())]);
    assert_eq!(board.design.footprints.len(), 3);
    assert!(board.design.footprint("J1").unwrap().hide_reference);
    let r2 = board.design.footprint("R2").unwrap();
    assert_eq!(r2.angle, 270.0);
    let Source::Library(lib) = &r2.source else { panic!("R2 is a library part") };
    assert_eq!((lib.library.as_str(), lib.name.as_str()), ("Test", "R_Test"));
    assert!(board.warnings.is_empty(), "{:?}", board.warnings);
    assert!(board.inputs.contains(&board.component));
}

#[test]
fn groups_place_their_contents() {
    let Some(board) = fixture() else { return };
    let design = board.design;
    // The divider group sits at (10, 10); R2 is at (0, 4) inside it.
    assert_eq!(design.footprint("R1").unwrap().at, Pt::mm(10.0, 10.0));
    assert_eq!(design.footprint("R2").unwrap().at, Pt::mm(10.0, 14.0));
    // The bend point (1.5, 2) is in the group's frame too.
    let mid: Vec<_> = design.tracks.iter().filter(|t| t.net == "MID").collect();
    assert_eq!(mid[0].end, Pt::mm(11.5, 12.0));
}

#[test]
fn pins_resolve_regardless_of_order() {
    let Some(board) = fixture() else { return };
    // The GND track names R2 before the divider places it. R2 is rotated
    // 270°, so its pad 2 (0.825 mm right, unrotated) lands 0.825 mm down.
    let gnd = board.design.tracks.iter().find(|t| t.net == "GND").unwrap();
    assert_eq!(gnd.start, Pt::new(Nm(10_000_000), Nm(14_825_000)));
    assert_eq!(gnd.width, Nm(300_000));
    assert_eq!(gnd.layer, Layer::FCu);
}

#[test]
fn legends_follow_pads() {
    let Some(board) = fixture() else { return };
    let legends: Vec<_> = board.design.texts.iter().map(|t| (t.text.as_str(), t.at)).collect();
    assert_eq!(legends, [("+", Pt::mm(22.0, 13.175)), ("-", Pt::mm(22.0, 14.825))]);
}

#[test]
fn route_width_runs_in_the_component() {
    let Some(board) = fixture() else { return };
    let widths = board.route_widths(&board.design.tracks).unwrap();
    let expected: Vec<_> = board.design.tracks.iter().map(|t| (t.net == "GND").then_some(Nm(500_000))).collect();
    assert_eq!(widths, expected);
}

#[test]
fn the_sandbox_hides_files_environment_and_time() {
    if !wasm_target() {
        return;
    }
    let board = BoardFile::load(&root().join("tests/fixtures/sandbox"), &footprints()).unwrap();
    assert_eq!(board.project.name, "file=false env=0 clock=0");
}

// --- The resolver, fed designs directly --------------------------------------------

fn design(no_connect: &[&str], items: Vec<wit::Item>) -> wit::Design {
    wit::Design {
        project: wit::Project {
            name: "probe".into(),
            bom: "bom.csv".into(),
            lcsc: vec![],
            order_quantity: 1,
            gerber_layers: String::new(),
            unsourced: None,
            checks: vec![],
            release_check: None,
            contract: None,
            release_files: vec![],
            kicad_dir: "output".into(),
            schematic: wit::Schematic { title: None, company: None, revision: None, comment: None, library: None },
            copper_weight_oz: 1,
            decisions: vec![],
        },
        no_connect: no_connect.iter().map(|n| n.to_string()).collect(),
        rules: None,
        netclasses: vec![],
        severities: vec![],
        copper_layers: 2,
        hide_references: vec![],
        finish: None,
        unlink_after_route: None,
        router_clearance: None,
        frames: vec![wit::Frame { parent: None, at: wit::Point { x: 10.0, y: 10.0 }, rotate: 90.0, name: None }],
        items,
    }
}

/// An item in the board frame, or in the design's one frame: at (10, 10),
/// turned a quarter counter-clockwise.
fn item(frame: Option<u32>, shape: wit::Shape) -> wit::Item {
    wit::Item { frame, shape }
}

fn resistor(reference: &str, pins: &[(&str, &str)]) -> wit::Shape {
    wit::Shape::Placement(wit::Placement {
        footprint: wit::Footprint::Library(wit::Part {
            reference: reference.into(),
            value: "1k".into(),
            footprint: "Test:R_Test".into(),
            pins: pins.iter().map(|(p, n)| (p.to_string(), n.to_string())).collect(),
            models: wit::Models::Keep,
        }),
        at: wit::Point { x: 0.0, y: 0.0 },
        rotate: 0.0,
        hide_reference: false,
    })
}

fn track(net: &str, points: Vec<wit::Anchor>, keep: wit::Keep) -> wit::Shape {
    wit::Shape::Track(wit::Track { net: net.into(), points, width: 0.2, layer: "F.Cu".into(), keep })
}

fn mm(x: f64, y: f64) -> wit::Anchor {
    wit::Anchor::Mm(wit::Point { x, y })
}

fn pin_ref(reference: &str, pad: &str) -> wit::PinRef {
    wit::PinRef { reference: reference.into(), pad: pad.into() }
}

fn pin(reference: &str, pad: &str) -> wit::Anchor {
    wit::Anchor::Pin(pin_ref(reference, pad))
}

fn offset(reference: &str, pad: &str, dx: f64, dy: f64) -> wit::Anchor {
    wit::Anchor::Offset(wit::PadOffset { pad: pin_ref(reference, pad), by: wit::Point { x: dx, y: dy } })
}

#[test]
fn reports_every_problem_at_once() {
    let mut broken = design(
        &[],
        vec![
            item(None, resistor("R1", &[("1", "A"), ("2", "B")])),
            item(None, track("A", vec![pin("R1", "1"), pin("R9", "1")], wit::Keep::No)),
            item(None, track("B", vec![offset("R1", "3", 1.0, 0.0), mm(5.0, 5.0)], wit::Keep::No)),
        ],
    );
    let rules = wit::Rules { track_width: 0.2, clearance: 0.2, via_diameter: 0.6, via_drill: 0.3 };
    broken.netclasses.push(wit::Netclass { name: "Power".into(), nets: vec!["TYPO".into()], rules });
    let err = format!("{:#}", resolve(broken, &Libraries::new(footprints()), root()).err().expect("invalid"));
    assert!(err.starts_with("3 problem(s):"), "{err}");
    assert!(err.contains("there is no part R9"), "{err}");
    assert!(err.contains("R1 has no pad 3"), "{err}");
    assert!(err.contains("net class Power names nets the board never uses: TYPO"), "{err}");
}

#[test]
fn single_connection_nets_warn() {
    let lonely = design(
        &["NC"],
        vec![
            item(None, resistor("R1", &[("1", "A"), ("2", "NC")])),
            item(None, track("B", vec![mm(0.0, 0.0), mm(1.0, 0.0)], wit::Keep::No)),
        ],
    );
    let resolved = resolve(lonely, &Libraries::new(footprints()), root()).unwrap();
    assert_eq!(resolved.warnings, ["net A connects only one pad", "net B connects no pads"]);
}

#[test]
fn offsets_follow_their_pad_and_turn_with_the_frame() {
    // R1 sits in the turned frame, so its pad 1 (0.825 mm left) lands 0.825 mm down.
    let board = design(
        &[],
        vec![
            item(Some(0), resistor("R1", &[("1", "A"), ("2", "A")])),
            item(None, track("A", vec![offset("R1", "1", 0.1, 0.2), pin("R1", "1")], wit::Keep::No)),
            item(Some(0), track("A", vec![offset("R1", "1", 1.0, 0.5), pin("R1", "1")], wit::Keep::No)),
        ],
    );
    let resolved = resolve(board, &Libraries::new(footprints()), root()).unwrap();
    let starts: Vec<Pt> = resolved.design.tracks.iter().map(|t| t.start).collect();
    assert_eq!(resolved.design.tracks[0].end, Pt::mm(10.0, 10.825));
    // In the board frame the offset is as written; in the frame it is turned
    // with it, (1, 0.5) becoming (0.5, -1), and added in whole nanometres.
    assert_eq!(starts, [Pt::new(Nm(10_100_000), Nm(11_025_000)), Pt::new(Nm(10_500_000), Nm(9_825_000))]);
}

#[test]
fn kept_tracks_are_restored_where_their_frame_puts_them() {
    let board = design(
        &[],
        vec![
            item(None, resistor("R1", &[("1", "A"), ("2", "A")])),
            item(Some(0), track("A", vec![mm(0.0, 0.0), mm(1.0, 0.0)], wit::Keep::Laid)),
            item(Some(0), track("A", vec![mm(1.0, 0.0), mm(2.0, 0.0)], wit::Keep::Routed)),
        ],
    );
    let resolved = resolve(board, &Libraries::new(footprints()), root()).unwrap();
    // Only the kept-as-laid track is on the board; both are restored after routing.
    assert_eq!(resolved.design.tracks.len(), 1);
    let restored: Vec<(Pt, Pt)> = resolved.restore.iter().map(|r| (r.start, r.end)).collect();
    assert_eq!(restored, [(Pt::mm(10.0, 10.0), Pt::mm(10.0, 9.0)), (Pt::mm(10.0, 9.0), Pt::mm(10.0, 8.0))]);
    assert_eq!(resolved.restore[0].width, Nm(200_000));
}

/// The shipped example must build and resolve from the repository as-is,
/// inputs included. Needs KiCad's footprint libraries and the wasm target.
#[test]
fn balance_bot_example_builds() {
    let tools = build_a_board::process::Tools::from_env();
    if !tools.footprints.is_dir() || !wasm_target() || !root().join("examples/balance-bot/Cargo.toml").is_file() {
        eprintln!("skipping: needs KiCad footprint libraries, wasm32-wasip2, and the Rust example board");
        return;
    }
    let example = root().join("examples/balance-bot");
    let board = BoardFile::load(&example, &tools.footprints).expect("example board builds");
    assert_eq!(board.design.footprints.len(), 77);
    assert_eq!(board.artwork.len(), 4);
    assert!(board.warnings.is_empty(), "{:?}", board.warnings);
    assert!(board.artwork.iter().all(|art| art.image.is_file()));
    assert!(example.join(&board.project.bom).is_file());
}

/// The small examples resolve without netlist warnings; the buck converter
/// carries the divider it computes.
#[test]
fn small_examples_build() {
    let tools = build_a_board::process::Tools::from_env();
    if !tools.footprints.is_dir() || !wasm_target() {
        eprintln!("skipping: needs KiCad footprint libraries and wasm32-wasip2");
        return;
    }
    for name in ["indicator", "buck-converter", "led-ring", "usb-uart"] {
        let board = BoardFile::load(&root().join("examples").join(name), &tools.footprints).expect(name);
        assert!(board.warnings.is_empty(), "{name}: {:?}", board.warnings);
        if name == "buck-converter" {
            assert_eq!(board.design.footprint("R2").unwrap().value, "105k 1%");
        }
    }
}
