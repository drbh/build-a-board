//! Specctra DSN export and SES session import for FreeRouting.
//!
//! A Rust port of KiCad 10.0.5's `pcbnew` Specctra exporter
//! (`ExportSpecctraDSN`) and session importer (`ImportSpecctraSES`),
//! operating directly on a parsed `.kicad_pcb` plus the net classes from its
//! `.kicad_pro`.
//!
//! The DSN text is byte-for-byte what pcbnew writes for the boards this
//! crate builds. Known gaps relative to pcbnew, none of which occur in those
//! boards: back-side footprints are rejected, footprint arcs and custom pad
//! primitives are approximated, more than two non-default net classes are
//! emitted in reverse insertion order (pcbnew uses libc++ hash order), and
//! courtyards are reproduced exactly only for outlines Clipper2 resolves
//! locally (rectangles, circles, convex polygons).

mod board;
mod clip;
pub(crate) mod export;
mod fmt;
mod geom;
mod ses;

pub use board::{Board, NetClass, ProjectRules};
pub use export::HOST_VERSION;
pub use ses::{ImportStats, import_ses};

use anyhow::Result;

use crate::sexpr::Sexp;

/// Export a parsed board as Specctra DSN text. `pcb_name` is written as the
/// design name (pcbnew uses the output file path).
pub fn export_dsn(board: &Sexp, rules: &ProjectRules, pcb_name: &str) -> Result<String> {
    export::export_dsn(&Board::parse(board)?, rules, pcb_name)
}

#[cfg(test)]
mod reference_tests;

/// An angle token as KiCad writes it (`90`, `-45.5`).
fn geom_angle(degrees: f64) -> Sexp {
    crate::sexpr::num(degrees)
}
