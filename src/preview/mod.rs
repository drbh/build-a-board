//! `bb preview`: draw a board with [`bb_core::preview`] and write or serve
//! the page. The drawing itself lives in bb-core; this side reads the inputs
//! (library footprints, artwork, the routed KiCad board) from disk.

mod serve;

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use bb_core::preview::Scene;
use bb_core::preview::svg;

use crate::artwork::ArtworkDir;
use crate::board::BoardFile;
use crate::kicad::Libraries;
use crate::sexpr::{self, Sexp};

pub use serve::{WatchOptions, watch};

/// The scene for a loaded board; with `routed`, copper comes from that
/// KiCad board file when it exists.
pub fn scene(board: &BoardFile, libraries: &Libraries, routed: Option<&Path>) -> Result<Scene> {
    let pcb = match routed {
        Some(path) if path.is_file() => Some(read_pcb(path)?),
        _ => None,
    };
    Scene::new(&board.project.name, &board.design, &board.artwork, libraries, &ArtworkDir(&board.dir), pcb.as_ref())
}

fn read_pcb(path: &Path) -> Result<Sexp> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    sexpr::parse(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Options for a one-off preview.
pub struct Options {
    pub routed: bool,
    pub open: bool,
}

/// Write `output/preview.html` for the board and print its path.
pub fn write(board: &BoardFile, footprints: &Path, kicad_pcb: &Path, options: &Options) -> Result<PathBuf> {
    let started = std::time::Instant::now();
    let libraries = Libraries::new(footprints);
    let scene = scene(board, &libraries, options.routed.then_some(kicad_pcb))?;
    let status = svg::Status { message: format!("drawn in {} ms", started.elapsed().as_millis()) };
    let page = svg::page(&scene, None, &status, false);
    let path = board.dir.join("output").join("preview.html");
    std::fs::create_dir_all(path.parent().expect("output dir"))?;
    std::fs::write(&path, page)?;
    println!("Preview: {} ({} footprints, {} unrouted)", path.display(), scene.footprints.len(), scene.ratsnest.len());
    report(&scene);
    if options.open {
        open(&path.display().to_string());
    }
    Ok(path)
}

/// Print what the placement checks found.
pub fn report(scene: &Scene) {
    for finding in &scene.findings {
        eprintln!("{}: {}", finding.severity.name(), finding.message);
    }
}

/// Open a file or URL with the desktop's default handler.
fn open(target: &str) {
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = std::process::Command::new(opener).arg(target).spawn();
}
