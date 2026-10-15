//! Board files evaluate into the expected design, without KiCad installed.

use std::path::Path;

use build_a_board::design::{Layer, Source};
use build_a_board::script::BoardFile;
use build_a_board::units::{Nm, Pt};

fn fixture() -> BoardFile {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    BoardFile::load(&root.join("board"), &root.join("footprints")).expect("fixture board evaluates")
}

#[test]
fn records_project_and_parts() {
    let board = fixture();
    assert_eq!(board.project.name, "fixture");
    assert_eq!(board.project.lcsc, [("MPN".to_owned(), "C1".to_owned())]);
    let design = &board.design;
    assert_eq!(design.footprints.len(), 2);
    let r2 = design.footprint("R2").unwrap();
    assert_eq!(r2.angle, 270.0);
    let Source::Library(lib) = &r2.source else { panic!("R2 is a library part") };
    assert_eq!((lib.library.as_str(), lib.name.as_str()), ("Test", "R_Test"));
}

#[test]
fn pins_anchor_tracks_exactly() {
    let design = fixture().design;
    assert_eq!(design.tracks.len(), 2);
    // R1's pad 2 is 0.825 mm right of its origin; R2 is rotated 270°, which
    // turns the same offset into 0.825 mm down.
    assert_eq!(design.tracks[0].start, Pt::mm(10.825, 10.0));
    assert_eq!(design.tracks[1].end, Pt::new(Nm(20_000_000), Nm(10_825_000)));
    assert_eq!(design.tracks[0].width, Nm(300_000));
    assert_eq!(design.tracks[0].layer, Layer::FCu);
}

#[test]
fn legends_follow_pads() {
    let design = fixture().design;
    let legends: Vec<_> = design.texts.iter().map(|t| (t.text.as_str(), t.at)).collect();
    assert_eq!(legends, [("+", Pt::mm(22.0, 9.175)), ("-", Pt::mm(22.0, 10.825))]);
}

#[test]
fn route_width_hook_runs() {
    let board = fixture();
    let widths = board.route_widths(&board.design.tracks).unwrap();
    assert_eq!(widths, [Some(Nm(500_000)), Some(Nm(500_000))]);
}

/// The shipped example must evaluate from the repository as-is, inputs
/// included. Needs KiCad's footprint libraries; skipped without them.
#[test]
fn balance_bot_example_evaluates() {
    let tools = build_a_board::process::Tools::from_env();
    if !tools.footprints.is_dir() {
        eprintln!("skipping: no KiCad footprint libraries at {}", tools.footprints.display());
        return;
    }
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/balance-bot");
    let board = BoardFile::load(&example, &tools.footprints).expect("example board evaluates");
    assert_eq!(board.design.footprints.len(), 77);
    assert_eq!(board.artwork.len(), 4);
    assert!(board.artwork.iter().all(|art| art.image.is_file()));
    assert!(example.join(&board.project.bom).is_file());
}
