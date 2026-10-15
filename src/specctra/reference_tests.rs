//! Comparisons against pcbnew-generated references. These need KiCad output
//! on disk, so they are ignored by default; point `SPECCTRA_REF_DIR` at a
//! directory holding `<name>.kicad_pcb`, `<name>.kicad_pro`, `<name>.dsn`
//! and run `cargo test specctra -- --ignored`.

use std::path::PathBuf;

use crate::sexpr;
use crate::specctra::{ProjectRules, export_dsn};

fn reference_dir() -> Option<PathBuf> {
    std::env::var_os("SPECCTRA_REF_DIR").map(PathBuf::from)
}

fn first_difference(expected: &str, actual: &str) -> Option<String> {
    let (e, a): (Vec<&str>, Vec<&str>) = (expected.lines().collect(), actual.lines().collect());
    let mismatches: Vec<usize> = (0..e.len().max(a.len())).filter(|&i| e.get(i) != a.get(i)).collect();
    let i = *mismatches.first()?;
    Some(format!(
        "{} differing lines (of {} / {}); first at line {}:\n  pcbnew: {:?}\n  rust:   {:?}",
        mismatches.len(),
        e.len(),
        a.len(),
        i + 1,
        e.get(i).unwrap_or(&""),
        a.get(i).unwrap_or(&"")
    ))
}

#[test]
#[ignore]
fn dsn_matches_pcbnew() {
    let Some(dir) = reference_dir() else { return };
    for name in ["unrouted", "routed"] {
        let board = std::fs::read_to_string(dir.join(format!("{name}.kicad_pcb"))).unwrap();
        let project: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{name}.kicad_pro"))).unwrap()).unwrap();
        let expected = std::fs::read_to_string(dir.join(format!("{name}.dsn"))).unwrap();
        let rules = ProjectRules::from_project(&project).unwrap();
        let actual = export_dsn(&sexpr::parse(&board).unwrap(), &rules, &format!("{name}.dsn")).unwrap();
        std::fs::write(dir.join(format!("{name}.rust.dsn")), &actual).unwrap();
        if let Some(diff) = first_difference(&expected, &actual) {
            panic!("{name}: {diff}");
        }
    }
}

/// Courtyards match pcbnew's `GetCourtyard( F_CrtYd )` to the nanometre
/// (as vertex sets: the cached polygon and the exported one differ only in
/// start vertex). Needs `<name>.courtyards.json` dumped from pcbnew.
#[test]
#[ignore]
fn courtyards_match_pcbnew() {
    let Some(dir) = reference_dir() else { return };
    for name in ["unrouted", "routed"] {
        let board = std::fs::read_to_string(dir.join(format!("{name}.kicad_pcb"))).unwrap();
        let expected: std::collections::HashMap<String, Vec<Vec<[i64; 2]>>> =
            serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{name}.courtyards.json"))).unwrap())
                .unwrap();
        let board = crate::specctra::Board::parse(&sexpr::parse(&board).unwrap()).unwrap();
        let mut checked = 0;
        for fp in &board.footprints {
            let mut ours: Vec<Vec<[i64; 2]>> = crate::specctra::export::courtyard(fp)
                .into_iter()
                .map(|poly| {
                    let mut v: Vec<[i64; 2]> = poly.iter().map(|p| [p.x, p.y]).collect();
                    v.sort();
                    v
                })
                .collect();
            let mut theirs: Vec<Vec<[i64; 2]>> = expected[&fp.reference]
                .iter()
                .map(|poly| {
                    let mut v = poly.clone();
                    v.sort();
                    v
                })
                .collect();
            ours.sort();
            theirs.sort();
            assert_eq!(ours, theirs, "{name} {}", fp.reference);
            checked += 1;
        }
        assert_eq!(checked, expected.len());
    }
}

// ------------------------------------------------------------- fixtures --
// `fixtures/fixture.kicad_pcb` is a trimmed Balance Bot board (round-rect,
// oval, circle, rect and keepout pads; rect, polygon and circle courtyards;
// 0/90/-90/180° footprints; tracks, vias, a zone, and the board outline).
// `fixture.dsn` is pcbnew 10.0.5's `ExportSpecctraDSN` of it.

const FIXTURE_PCB: &str = include_str!("fixtures/fixture.kicad_pcb");
const FIXTURE_PRO: &str = include_str!("fixtures/fixture.kicad_pro");
const FIXTURE_DSN: &str = include_str!("fixtures/fixture.dsn");
const FIXTURE_SES: &str = include_str!("fixtures/fixture.ses");

fn fixture_rules() -> ProjectRules {
    ProjectRules::from_project(&serde_json::from_str(FIXTURE_PRO).unwrap()).unwrap()
}

#[test]
fn fixture_dsn_is_byte_identical_to_pcbnew() {
    let board = sexpr::parse(FIXTURE_PCB).unwrap();
    let actual = export_dsn(&board, &fixture_rules(), "fixture.dsn").unwrap();
    if let Some(diff) = first_difference(FIXTURE_DSN, &actual) {
        panic!("{diff}");
    }
    assert_eq!(actual, FIXTURE_DSN);
}

/// Expected values are pcbnew's `ImportSpecctraSES` of the same session.
#[test]
fn fixture_session_import_matches_pcbnew() {
    let mut board = sexpr::parse(FIXTURE_PCB).unwrap();
    let stats = crate::specctra::import_ses(&mut board, FIXTURE_SES, &fixture_rules()).unwrap();
    assert_eq!(stats, crate::specctra::ImportStats { removed: 18, segments: 3, vias: 2, moved: 1 });

    let tracks: Vec<&sexpr::Sexp> =
        board.args().iter().filter(|i| matches!(i.head(), Some("segment" | "via"))).collect();
    let summary: Vec<String> = tracks
        .iter()
        .map(|t| {
            let xy = |head: &str| {
                let n = t.child(head).unwrap();
                format!("{} {}", n.arg(0).unwrap(), n.arg(1).unwrap())
            };
            let net = t.value("net").unwrap();
            if t.is("via") {
                format!("via {net} {} {} {}", xy("at"), t.value("size").unwrap(), t.value("drill").unwrap())
            } else {
                format!(
                    "seg {net} {} {} {} {} {}",
                    t.value("layer").unwrap(),
                    xy("start"),
                    xy("end"),
                    t.value("width").unwrap(),
                    t.value("locked") == Some("yes")
                )
            }
        })
        .collect();
    assert_eq!(
        summary,
        [
            "seg VBAT F.Cu 50 75 52 75 1.5 false",
            "seg VBAT F.Cu 52 75 53 76.0123 1.5 false",
            // Default-drill vias take their net class drill (Battery: 0.5 mm).
            "via VBAT 53 76.0123 0.6 0.5",
            "seg M1_L B.Cu 24 74 25 74 0.5 true",
            "via M1_L 25 74 0.8 0.4",
        ]
    );
    // FreeRouting rounds J9 to the session resolution; pcbnew moves it 1 nm.
    let j9 = board
        .children("footprint")
        .find(|f| f.children("property").any(|p| p.arg(1).and_then(sexpr::Sexp::text) == Some("J9")))
        .unwrap();
    let at = j9.child("at").unwrap();
    assert_eq!(at.args().iter().map(|a| a.to_string()).collect::<Vec<_>>(), ["42.5735", "52.0955", "180"]);
}
