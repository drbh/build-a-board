//! Board programs: WebAssembly components that build a design.
//!
//! A board directory holds either a Rust crate (`Cargo.toml`, written with
//! `bb-sdk` and built for `wasm32-wasip2`) or a prebuilt `board.wasm`, plus
//! the inputs its design names (artwork, parts lists). `bb` builds the
//! component when needed, runs it in a sandbox, and resolves the design it
//! returns. Any language that targets the `bb:board` WIT world works.

mod convert;
pub mod runtime;

pub use bb_core::resolve::{Artwork, Project, Resolved, Restore, SchematicConfig, resolve};

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow, bail};

use crate::artwork::ArtworkDir;
use crate::design;
use crate::kicad::Libraries;
use crate::process::Cmd;
use crate::units::{Nm, Pt};
use runtime::{Guest, wit};

/// The board program, built, run, and resolved.
pub struct BoardFile {
    pub dir: PathBuf,
    pub design: design::Board,
    pub project: Project,
    pub artwork: Vec<Artwork>,
    pub restore: Vec<Restore>,
    pub unlink_after_route: Option<String>,
    /// The clearance FreeRouting is given, in mm.
    pub router_clearance: f64,
    /// Netlist warnings, e.g. a net with a single connection.
    pub warnings: Vec<String>,
    /// Every file the design was built from: the component, the library
    /// footprints it placed, and its artwork.
    pub inputs: Vec<PathBuf>,
    /// The compiled component.
    pub component: PathBuf,
    guest: RefCell<Guest>,
}

/// The board component for `dir`: built from its crate, or prebuilt.
pub fn component(dir: &Path) -> Result<PathBuf> {
    let manifest = dir.join("Cargo.toml");
    if manifest.is_file() {
        return build_crate(&manifest);
    }
    let prebuilt = dir.join("board.wasm");
    if prebuilt.is_file() {
        return Ok(prebuilt);
    }
    bail!("{} has no board: expected a Cargo.toml (a bb-sdk crate) or board.wasm", dir.display())
}

/// `cargo build` the board crate for `wasm32-wasip2` and find its component.
/// Compiler diagnostics are printed as they arrive, and a failed build's
/// errors are returned in the error itself (so `bb preview` can show them).
fn build_crate(manifest: &Path) -> Result<PathBuf> {
    let (status, messages) = Cmd::new("cargo")
        .args(["build", "--release", "--target", "wasm32-wasip2", "--message-format=json"])
        .arg("--manifest-path")
        .arg(manifest)
        .stdout_and_status()
        .context("building the board crate")?;
    let mut wasm = None;
    let mut errors = Vec::new();
    for line in String::from_utf8_lossy(&messages).lines() {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        match message["reason"].as_str() {
            Some("compiler-message") => {
                let Some(rendered) = message["message"]["rendered"].as_str() else { continue };
                eprint!("{rendered}");
                if message["message"]["level"] == "error" {
                    errors.push(rendered.trim_end().to_owned());
                }
            }
            Some("compiler-artifact") => {
                let mut files = message["filenames"].as_array().into_iter().flatten().filter_map(|f| f.as_str());
                if let Some(file) = files.rfind(|f| f.ends_with(".wasm")) {
                    wasm = Some(PathBuf::from(file));
                }
            }
            _ => {}
        }
    }
    if !status.success() {
        bail!("the board crate does not compile:\n\n{}", errors.join("\n\n"));
    }
    wasm.ok_or_else(|| {
        anyhow!("{} built no WebAssembly component; is it a cdylib using bb_sdk::export_board!?", manifest.display())
    })
}

impl BoardFile {
    /// Build (if needed), run, and resolve the board at `path`: a board
    /// directory or a `.wasm` component.
    pub fn load(path: &Path, footprint_root: &Path) -> Result<Self> {
        let (dir, component) = if path.is_dir() {
            (path.to_path_buf(), component(path)?)
        } else {
            (path.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf), path.to_path_buf())
        };
        let wasm = std::fs::read(&component).with_context(|| format!("reading {}", component.display()))?;
        let mut guest = Guest::load(&wasm)?;
        let built = guest.build()?;
        let libraries = Libraries::new(footprint_root);
        let resolved = resolve(built.into(), &libraries, &ArtworkDir(&dir))
            .with_context(|| format!("in the board built from {}", component.display()))?;
        let mut inputs = vec![component.clone()];
        inputs.extend(libraries.files());
        inputs.extend(resolved.artwork.iter().map(|art| art.image.clone()));
        inputs.sort();
        inputs.dedup();
        Ok(Self {
            dir,
            design: resolved.design,
            project: resolved.project,
            artwork: resolved.artwork,
            restore: resolved.restore,
            unlink_after_route: resolved.unlink_after_route,
            router_clearance: resolved.router_clearance.unwrap_or(0.7),
            warnings: resolved.warnings,
            inputs,
            component,
            guest: RefCell::new(guest),
        })
    }

    /// The board's route-width rule for each routed track (`None` keeps it).
    pub fn route_widths(&self, tracks: &[design::Track]) -> Result<Vec<Option<Nm>>> {
        let mut guest = self.guest.borrow_mut();
        let point = |p: Pt| wit::Point { x: p.x.to_mm(), y: p.y.to_mm() };
        tracks
            .iter()
            .map(|t| {
                let routed = wit::RoutedTrack {
                    net: t.net.clone(),
                    layer: t.layer.name().to_owned(),
                    width: t.width.to_mm(),
                    start: point(t.start),
                    end: point(t.end),
                };
                Ok(guest.route_width(&routed)?.map(Nm::from_design_mm))
            })
            .collect()
    }
}
