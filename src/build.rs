//! `bb build`: one idempotent pipeline from board file to routed, rendered board.
//!
//! Each stage records what it was built from in `<name>.bb.json` beside the
//! KiCad project and is skipped when that hasn't changed:
//!
//! 1. **generate**: when the board's inputs (board files, artwork, footprints)
//!    or the tool versions change. The unrouted board is kept in
//!    `output/router/` so routing always starts from it.
//! 2. **route**: when the unrouted board changes. FreeRouting is slow and not
//!    deterministic, so an existing routed board is never redone needlessly.
//! 3. **schematic**: when the board's connectivity changes. An existing
//!    schematic that already matches the board net for net is kept.
//! 4. **render**: when the board or schematic changes or images are missing.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::commands::{self, Ctx};
use crate::process::Cmd;
use crate::release::archive::copy_with_times;
use crate::release::lock::sha256;

/// Which stages to run.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub route: bool,
    pub schematic: bool,
    pub render: bool,
    /// Render every view (perspective sheets, GLB), not just top and bottom.
    pub all_views: bool,
    /// Rebuild every enabled stage regardless of the record.
    pub force: bool,
    pub attempts: u32,
    pub timeout: std::time::Duration,
}

/// What each stage was last built from (hashes are SHA-256).
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
struct Record {
    /// Board inputs plus tool versions.
    inputs: Option<String>,
    /// The unrouted board generated from those inputs.
    unrouted: Option<String>,
    /// The routed board, and the unrouted board it was routed from.
    routed: Option<String>,
    routed_from: Option<String>,
    /// The board connectivity the schematic was written for.
    schematic: Option<String>,
    /// The board and schematic the images show, and whether they include every view.
    rendered: Option<String>,
    rendered_schematic: Option<String>,
    all_views: bool,
}

impl Record {
    fn path(ctx: &Ctx) -> PathBuf {
        ctx.pcb.with_extension("bb.json")
    }

    fn load(ctx: &Ctx) -> Self {
        std::fs::read_to_string(Self::path(ctx))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save(&self, ctx: &Ctx) -> Result<()> {
        let path = Self::path(ctx);
        std::fs::write(&path, serde_json::to_string_pretty(self)? + "\n")
            .with_context(|| format!("writing {}", path.display()))
    }
}

/// A fingerprint of everything generation depends on.
fn input_hash(ctx: &Ctx) -> Result<String> {
    let mut digest = Sha256::new();
    digest.update(format!("bb {}\n", env!("CARGO_PKG_VERSION")));
    let kicad = Cmd::new(&ctx.tools.kicad_cli).arg("version").output()?.unwrap_or_default();
    digest.update(b"kicad ");
    digest.update(&kicad);
    for input in &ctx.board.inputs {
        // Name inputs portably, so moving the board or the KiCad install
        // does not count as a change.
        let name =
            input.strip_prefix(&ctx.board.dir).or_else(|_| input.strip_prefix(&ctx.tools.footprints)).unwrap_or(input);
        digest.update(name.to_string_lossy().as_bytes());
        digest.update(b"\0");
        digest.update(sha256(input)?.as_bytes());
        digest.update(b"\n");
    }
    Ok(digest.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn current(path: &Path) -> Option<String> {
    path.is_file().then(|| sha256(path).ok()).flatten()
}

pub fn build(ctx: &Ctx, options: Options) -> Result<()> {
    let mut record = Record::load(ctx);
    let unrouted = ctx.out.join("router").join(format!("{}-unrouted.kicad_pcb", ctx.board.project.name));

    // A committed board that bb has no record of was reviewed elsewhere;
    // rebuilding it would throw away its routing.
    if !options.force && record == Record::default() && ctx.pcb.is_file() && commands::tracked_by_git(ctx, &ctx.pcb) {
        anyhow::bail!(
            "{} is committed but has no build record, so bb can't tell whether it is current. \
             Rebuilding would replace its routing; run with --force to rebuild it from the board file.",
            ctx.pcb.display()
        );
    }

    // 1. Generate.
    let inputs = input_hash(ctx)?;
    if options.force || record.inputs.as_ref() != Some(&inputs) || current(&unrouted) != record.unrouted {
        commands::generate(ctx)?;
        std::fs::create_dir_all(unrouted.parent().expect("router directory"))?;
        copy_with_times(&ctx.pcb, &unrouted)?;
        record = Record { inputs: Some(inputs), unrouted: current(&unrouted), ..Record::default() };
        record.save(ctx)?;
    } else {
        println!("Board up to date ({} inputs unchanged): {}", ctx.board.inputs.len(), ctx.pcb.display());
    }

    // 2. Route, or make sure the unrouted board is the result.
    let board = current(&ctx.pcb);
    if options.route {
        let routed = record.routed.is_some() && board == record.routed && record.routed_from == record.unrouted;
        if options.force || !routed {
            if let Err(err) = commands::route(ctx, &unrouted, options.attempts, options.timeout) {
                copy_with_times(&unrouted, &ctx.pcb)?;
                record.routed = None;
                record.save(ctx)?;
                return Err(err);
            }
            record.routed = current(&ctx.pcb);
            record.routed_from = record.unrouted.clone();
            record.save(ctx)?;
        } else {
            println!("Routed board up to date: {}", ctx.pcb.display());
        }
    } else if board != record.unrouted {
        copy_with_times(&unrouted, &ctx.pcb)?;
        println!("Using the unrouted board (--no-route): {}", ctx.pcb.display());
    }

    // 3. Schematic.
    if options.schematic {
        let netlist = connectivity_hash(ctx)?;
        if options.force || record.schematic.as_ref() != Some(&netlist) || !ctx.sch.is_file() {
            if !options.force && record.schematic.is_none() && ctx.sch.is_file() && commands::schematic_matches(ctx)? {
                println!("Keeping {}: it matches the board's connectivity", ctx.sch.display());
            } else {
                commands::write_schematic(ctx)?;
            }
            record.schematic = Some(netlist);
            record.save(ctx)?;
        } else {
            println!("Schematic up to date: {}", ctx.sch.display());
        }
    }

    // 4. Render.
    if options.render {
        let board = current(&ctx.pcb);
        let schematic = if options.schematic { current(&ctx.sch) } else { None };
        let mut images = commands::render_outputs(ctx, options.all_views);
        if schematic.is_some() {
            images.extend(commands::schematic_outputs(ctx));
        }
        let fresh = board == record.rendered
            && schematic == record.rendered_schematic
            && (record.all_views || !options.all_views)
            && images.iter().all(|i| i.is_file());
        if options.force || !fresh {
            commands::render(ctx, options.all_views)?;
            if schematic.is_some() {
                commands::render_schematic(ctx)?;
            }
            record.rendered = board;
            record.rendered_schematic = schematic;
            record.all_views = options.all_views;
            record.save(ctx)?;
        } else {
            println!("Renders up to date");
        }
        for image in images {
            println!("  {}", image.display());
        }
    }
    Ok(())
}

/// A fingerprint of the board's pad-level connectivity.
fn connectivity_hash(ctx: &Ctx) -> Result<String> {
    let text = std::fs::read_to_string(&ctx.pcb).with_context(|| format!("reading {}", ctx.pcb.display()))?;
    let board = crate::sexpr::parse(&text)?;
    let mut digest = Sha256::new();
    for c in crate::kicad::netlist::connections(&board) {
        digest.update(format!("{}\0{}\0{}\0{}\n", c.net, c.reference, c.pad, c.value));
    }
    Ok(digest.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
