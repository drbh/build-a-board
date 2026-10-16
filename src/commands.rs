//! The `bb` workflow: generate, route, check, render, and release a board.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result, bail};

use crate::board::BoardFile;
use crate::generate::{self, Outputs};
use crate::kicad::{edit, netlist, normalize};
use crate::process::{Cmd, Tools};
use crate::release::archive::{copy_with_times, zip_files};
use crate::release::bom::{self, BomOutputs, Table};
use crate::release::lock::{self, Sources};
use crate::schematic::{self, SchematicOptions};
use crate::sexpr;
use crate::specctra;

/// A loaded board plus where its files and tools are.
pub struct Ctx {
    pub board: BoardFile,
    pub tools: Tools,
    pub dir: PathBuf,
    pub repo: PathBuf,
    pub out: PathBuf,
    pub pcb: PathBuf,
    pub pro: PathBuf,
    pub sch: PathBuf,
    pub lock: PathBuf,
}

impl Ctx {
    pub fn new(board: BoardFile, tools: Tools) -> Self {
        let dir = board.dir.canonicalize().unwrap_or_else(|_| board.dir.clone());
        let repo = Cmd::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .cwd(&dir)
            .output()
            .ok()
            .flatten()
            .map(|out| PathBuf::from(String::from_utf8_lossy(&out).trim()))
            .unwrap_or_else(|| dir.parent().unwrap_or(&dir).to_path_buf());
        // `components()` drops `.` segments, so `kicad_dir = "."` is the board directory itself.
        let kicad: PathBuf = dir.join(&board.project.kicad_dir).components().collect();
        let file = |ext: &str| kicad.join(format!("{}.{ext}", board.project.name));
        Self {
            pcb: file("kicad_pcb"),
            pro: file("kicad_pro"),
            sch: file("kicad_sch"),
            lock: dir.join("board-revision.json"),
            out: dir.join("output"),
            repo,
            dir,
            board,
            tools,
        }
    }

    fn name(&self) -> &str {
        &self.board.project.name
    }

    /// `output/<dir>/<project><suffix>`.
    fn artifact(&self, dir: &str, suffix: &str) -> PathBuf {
        self.out.join(dir).join(format!("{}{suffix}", self.name()))
    }

    /// An artifact path about to be written: its folder is created on demand,
    /// so `output/` only ever holds folders with something in them.
    fn output_file(&self, dir: &str, suffix: &str) -> Result<PathBuf> {
        let path = self.artifact(dir, suffix);
        std::fs::create_dir_all(path.parent().expect("artifacts live in a folder"))?;
        Ok(path)
    }

    fn kicad<I, S>(&self, args: I) -> Result<Cmd>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        Ok(self.tools.kicad(args)?.cwd(&self.dir))
    }

    fn sources(&self) -> Sources<'_> {
        Sources { repo: &self.repo, files: vec![&self.pcb, &self.sch] }
    }
}

fn project_rules(ctx: &Ctx) -> Result<specctra::ProjectRules> {
    let text = std::fs::read_to_string(&ctx.pro).with_context(|| format!("reading {}", ctx.pro.display()))?;
    specctra::ProjectRules::from_project(&serde_json::from_str(&text)?)
}

fn read_board(path: &Path) -> Result<sexpr::Sexp> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    sexpr::parse(&text).with_context(|| format!("parsing {}", path.display()))
}

fn write_board(path: &Path, board: &sexpr::Sexp) -> Result<()> {
    std::fs::write(path, board.to_kicad()).with_context(|| format!("writing {}", path.display()))
}

pub fn doctor(ctx: &Ctx) -> Result<()> {
    let mut missing = Vec::new();
    for (name, path, found) in ctx.tools.inventory() {
        println!("{} {name:<17} {}", if found { "ok  " } else { "MISS" }, path.display());
        if !found {
            missing.push(name);
        }
    }
    if !missing.is_empty() {
        bail!("missing tools: {}", missing.join(", "));
    }
    ctx.kicad(["version"])?.run()?;
    Cmd::new(&ctx.tools.java).arg("-version").run()?;
    println!("Toolchain OK for {}", ctx.name());
    Ok(())
}

pub fn generate(ctx: &Ctx) -> Result<()> {
    generate::generate(&ctx.board, &ctx.tools, Outputs { board: &ctx.pcb, project: &ctx.pro })
}

/// FreeRouting under-honours small Specctra clearances; raise every rule
/// except SMD-to-SMD to at least `minimum` (the DSN is in µm).
fn widen_dsn_clearances(dsn: &str, minimum: u64) -> (String, usize) {
    let rule = regex::Regex::new(r"\(clearance\s+(\d+)(\s+\(type\s+[^)]+\))?\)").expect("valid regex");
    let mut count = 0;
    let text = rule.replace_all(dsn, |caps: &regex::Captures| {
        count += 1;
        let kind = caps.get(2).map_or("", |m| m.as_str());
        match caps[1].parse::<u64>() {
            Ok(value) if !kind.contains("smd_smd") => format!("(clearance {}{kind})", value.max(minimum)),
            _ => caps[0].to_owned(),
        }
    });
    (text.into_owned(), count)
}

/// After FreeRouting: import its session, apply the board's post-route
/// rules, and refill/check with KiCad. Returns whether DRC accepts it.
fn finish_route(ctx: &Ctx, ses: &Path, report: &Path) -> Result<bool> {
    let mut board = read_board(&ctx.pcb)?;
    let imported = specctra::import_ses(&mut board, &std::fs::read_to_string(ses)?, &project_rules(ctx)?)?;
    println!(
        "Imported {}: {} segments and {} vias (replaced {} items, moved {} footprints)",
        ses.display(),
        imported.segments,
        imported.vias,
        imported.removed,
        imported.moved
    );
    if let Some(library) = &ctx.board.unlink_after_route {
        let artwork: Vec<&str> = ctx.board.artwork.iter().map(|a| a.reference.as_str()).collect();
        edit::unlink_library(&mut board, library, &artwork)?;
    }
    let widths = ctx.board.route_widths(&edit::segments(&board))?;
    edit::set_widths(&mut board, &widths);
    for keep in &ctx.board.restore {
        edit::keep_segment(
            &mut board,
            &crate::design::Track {
                net: keep.net.clone(),
                layer: keep.layer,
                width: keep.width,
                start: keep.start,
                end: keep.end,
            },
        );
    }
    let removed = edit::dedupe_segments(&mut board);
    println!("Removed {removed} duplicate track segment(s)");
    write_board(&ctx.pcb, &board)?;
    ctx.kicad(["pcb", "drc", "--refill-zones", "--save-board", "--format", "json", "--output"])?
        .arg(report)
        .arg(&ctx.pcb)
        .run()?;
    let (text, _) = normalize::normalize(&std::fs::read_to_string(&ctx.pcb)?, ctx.name())?;
    std::fs::write(&ctx.pcb, text)?;
    route_accepted(report)
}

/// Accept a routed board only with no unconnected items and no DRC errors.
fn route_accepted(report: &Path) -> Result<bool> {
    let data: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(report)?)?;
    let count = |key: &str, keep: &dyn Fn(&serde_json::Value) -> bool| {
        data.get(key).and_then(|v| v.as_array()).map_or(0, |items| items.iter().filter(|v| keep(v)).count())
    };
    let unconnected = count("unconnected_items", &|_| true);
    let errors = count("violations", &|v| v.get("severity").and_then(|s| s.as_str()) == Some("error"));
    if unconnected > 0 || errors > 0 {
        println!("Route rejected: {unconnected} unconnected, {errors} DRC errors");
        return Ok(false);
    }
    println!("Route accepted: 0 unconnected items, 0 DRC errors");
    Ok(true)
}

/// Route the `unrouted` board into the project's board file, retrying
/// until KiCad's DRC accepts the result.
pub fn route(ctx: &Ctx, unrouted: &Path, attempts: u32, timeout: Duration) -> Result<()> {
    if !ctx.tools.freerouting.is_file() {
        bail!("FreeRouting jar not found: {}", ctx.tools.freerouting.display());
    }
    if !route_attempts(ctx, unrouted, attempts, timeout)? {
        bail!("FreeRouting failed after {attempts} attempts");
    }
    println!("Routed {}", ctx.pcb.display());
    Ok(())
}

fn route_attempts(ctx: &Ctx, unrouted: &Path, attempts: u32, timeout: Duration) -> Result<bool> {
    let router = ctx.out.join("router");
    std::fs::create_dir_all(&router)?;
    let profile = tempfile::Builder::new().prefix("freerouting-").tempdir_in(&router)?;
    let dsn = ctx.artifact("router", ".dsn");
    let ses = ctx.artifact("router", ".ses");
    for attempt in 1..=attempts {
        println!("FreeRouting attempt {attempt}/{attempts}");
        copy_with_times(unrouted, &ctx.pcb)?;
        let mut board = read_board(&ctx.pcb)?;
        if edit::dedupe_segments(&mut board) > 0 {
            write_board(&ctx.pcb, &board)?;
        }
        let exported = specctra::export_dsn(&board, &project_rules(ctx)?, &dsn.display().to_string())?;
        let (text, rules) = widen_dsn_clearances(&exported, (ctx.board.router_clearance * 1000.0).round() as u64);
        std::fs::write(&dsn, text)?;
        println!("Exported {} ({rules} clearance rules widened for FreeRouting)", dsn.display());
        let _ = std::fs::remove_file(&ses);
        let freerouting = Cmd::new(&ctx.tools.java)
            .arg("-Djava.awt.headless=true")
            .arg(format!("-Djava.io.tmpdir={}", profile.path().display()))
            .arg("-jar")
            .arg(&ctx.tools.freerouting)
            .args(["--gui.enabled=false", "-de"])
            .arg(&dsn)
            .arg("-do")
            .arg(&ses)
            .args(["-mp", "100", "-mt", "1"])
            .cwd(&ctx.dir)
            .timeout(timeout);
        match freerouting.run() {
            Ok(()) => {}
            Err(err) if err.is_tool_failure() => {
                eprintln!("FreeRouting stalled or failed ({err}); retrying.");
                continue;
            }
            Err(err) => return Err(err.into()),
        }
        // FreeRouting occasionally exits cleanly without writing a session.
        if std::fs::metadata(&ses).map_or(true, |m| m.len() == 0) {
            eprintln!("FreeRouting wrote no session; retrying.");
            continue;
        }
        let report = router.join(format!("route-attempt-{attempt}-drc.json"));
        if finish_route(ctx, &ses, &report)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn write_netlist(ctx: &Ctx) -> Result<PathBuf> {
    let path = ctx.output_file("netlists", "-netlist.csv")?;
    netlist::write_csv(&read_board(&ctx.pcb)?, &path)?;
    println!("Wrote PCB netlist {}", path.display());
    Ok(path)
}

fn core_checks(ctx: &Ctx) -> Result<Table> {
    lock::verify(&ctx.lock, &ctx.sources())?;
    let rows = bom::validate(&ctx.board.project, &ctx.dir.join(&ctx.board.project.bom), &ctx.pcb)?;
    let pcb_net = write_netlist(ctx)?;
    let sch_net = ctx.output_file("reports", "-schematic-netlist.xml")?;
    ctx.kicad(["sch", "export", "netlist", "--format", "kicadxml", "--output"])?.arg(&sch_net).arg(&ctx.sch).run()?;
    lock::connectivity(&pcb_net, &sch_net)?;
    ctx.kicad(["pcb", "drc", "--severity-error", "--exit-code-violations", "--output"])?
        .arg(ctx.output_file("reports", "-drc.rpt")?)
        .arg(&ctx.pcb)
        .run()?;
    ctx.kicad(["sch", "erc", "--severity-error", "--exit-code-violations", "--output"])?
        .arg(ctx.output_file("reports", "-erc.rpt")?)
        .arg(&ctx.sch)
        .run()?;
    Ok(rows)
}

fn export_boms(ctx: &Ctx, rows: &Table) -> Result<()> {
    bom::export(
        &ctx.board.project,
        rows,
        BomOutputs {
            bom: &ctx.artifact("bom", "-bom.csv"),
            jlcpcb: &ctx.artifact("bom", "-jlcpcb-bom.csv"),
            shopping: &ctx.out.join("digikey-shopping-list.csv"),
        },
    )
}

fn assembly(ctx: &Ctx, rows: &Table) -> Result<()> {
    let positions = ctx.output_file("assembly", "-position.csv")?;
    ctx.kicad(["pcb", "export", "pos", "--output"])?
        .arg(&positions)
        .args(["--format", "csv", "--units", "mm", "--side", "both"])
        .arg(&ctx.pcb)
        .run()?;
    let offsets: HashMap<&str, f64> =
        ctx.board.design.footprints.iter().map(|fp| (fp.reference.as_str(), fp.assembly_rotation)).collect();
    bom::export_cpl(rows, &positions, &offsets, &ctx.artifact("assembly", "-cpl.csv"))
}

pub fn check(ctx: &Ctx) -> Result<()> {
    let rows = core_checks(ctx)?;
    export_boms(ctx, &rows)?;
    assembly(ctx, &rows)
}

fn schematic_image(ctx: &Ctx, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target)?;
    ctx.kicad(["sch", "export", "svg", "--output"])?
        .arg(target)
        .arg("--exclude-drawing-sheet")
        .arg(&ctx.sch)
        .run_quiet()?;
    let svg = target.join(format!("{}.svg", ctx.name()));
    let png = target.join(format!("{}-schematic.png", ctx.name()));
    Cmd::new(&ctx.tools.magick)
        .args(["-density", "180"])
        .arg(&svg)
        .args(["-background", "white", "-alpha", "remove", "-alpha", "off", "-resize", "2600x2600>"])
        .arg(&png)
        .run_quiet()?;
    println!("Schematic images ready: {} and {}", svg.display(), png.display());
    Ok(())
}

/// Write the connectivity schematic from the board's netlist.
pub fn write_schematic(ctx: &Ctx) -> Result<()> {
    let netlist = write_netlist(ctx)?;
    let config = &ctx.board.project.schematic;
    let mut options = SchematicOptions::new(ctx.name());
    options.title = config.title.clone();
    options.revision = config.revision.clone();
    options.company = config.company.clone();
    options.comments = config.comment.iter().map(|c| (1, c.clone())).collect();
    options.library = config.library.clone();
    std::fs::write(&ctx.sch, schematic::generate(&netlist, &options)?)?;
    println!("Wrote {}", ctx.sch.display());
    Ok(())
}

/// Whether the existing schematic carries exactly the board's connectivity.
pub fn schematic_matches(ctx: &Ctx) -> Result<bool> {
    let pcb_net = write_netlist(ctx)?;
    let sch_net = ctx.output_file("reports", "-schematic-netlist.xml")?;
    ctx.kicad(["sch", "export", "netlist", "--format", "kicadxml", "--output"])?
        .arg(&sch_net)
        .arg(&ctx.sch)
        .run_quiet()?;
    Ok(lock::connectivity(&pcb_net, &sch_net).is_ok())
}

/// The schematic's SVG and PNG renders.
pub fn schematic_outputs(ctx: &Ctx) -> [PathBuf; 2] {
    let target = ctx.artifact("render", "");
    [target.join(format!("{}.svg", ctx.name())), target.join(format!("{}-schematic.png", ctx.name()))]
}

pub fn render_schematic(ctx: &Ctx) -> Result<()> {
    schematic_image(ctx, &ctx.artifact("render", ""))
}

pub fn schematic(ctx: &Ctx, output: Option<&Path>) -> Result<()> {
    write_schematic(ctx)?;
    let target = match output {
        Some(dir) => {
            std::fs::create_dir_all(dir)?;
            dir.canonicalize()?
        }
        None => ctx.artifact("render", ""),
    };
    schematic_image(ctx, &target)
}

fn fabrication(ctx: &Ctx) -> Result<()> {
    let fab = ctx.artifact("fabrication", "");
    let _ = std::fs::remove_dir_all(&fab);
    std::fs::create_dir_all(&fab)?;
    ctx.kicad(["pcb", "export", "gerbers", "--output"])?
        .arg(&fab)
        .args(["--layers", &ctx.board.project.gerber_layers, "--precision", "6"])
        .arg(&ctx.pcb)
        .run()?;
    ctx.kicad(["pcb", "export", "drill", "--output"])?
        .arg(&fab)
        .args(["--format", "excellon", "--excellon-units", "mm", "--excellon-zeros-format", "decimal"])
        .args(["--excellon-separate-th", "--generate-map", "--map-format", "gerberx2"])
        .arg(&ctx.pcb)
        .run()?;
    Ok(())
}

const RENDER_COMMON: [&str; 4] = ["--background", "opaque", "--use-board-stackup-colors", "--floor"];

/// A straight-down render of one side, the quick way to see the board.
fn wiring_view(ctx: &Ctx, side: &str) -> Result<PathBuf> {
    let target = ctx.artifact("render", "");
    std::fs::create_dir_all(&target)?;
    let output = target.join(format!("{}-wiring-{side}.png", ctx.name()));
    let flip: &[&str] = if side == "bottom" { &["--side", "bottom"] } else { &[] };
    ctx.kicad(["pcb", "render", "--output"])?
        .arg(&output)
        .args(["--width", "1800", "--height", "1800"])
        .args(flip)
        .args(["--quality", "basic"])
        .args(RENDER_COMMON)
        .args(["--rotate", "0,0,0"])
        .arg(&ctx.pcb)
        .run_quiet()?;
    Ok(output)
}

/// The images [`render`] writes.
pub fn render_outputs(ctx: &Ctx, all_views: bool) -> Vec<PathBuf> {
    let target = ctx.artifact("render", "");
    let name = ctx.name();
    let mut images =
        vec![target.join(format!("{name}-wiring-top.png")), target.join(format!("{name}-wiring-bottom.png"))];
    if all_views {
        images.extend(["3-view.png", "9-angle.png"].map(|suffix| target.join(format!("{name}-{suffix}"))));
        images.push(target.join(format!("{name}.glb")));
    }
    images
}

/// Top and bottom views, plus with `all_views` nine perspective views, a
/// three-view sheet, and a GLB model.
pub fn render(ctx: &Ctx, all_views: bool) -> Result<()> {
    println!("Rendering {}...", if all_views { "all views" } else { "top and bottom" });
    let top = wiring_view(ctx, "top")?;
    let bottom = wiring_view(ctx, "bottom")?;
    if !all_views {
        return Ok(());
    }
    let name = ctx.name();
    let target = ctx.artifact("render", "");
    let snapshots = target.join("snapshots");
    let _ = std::fs::remove_dir_all(&snapshots);
    std::fs::create_dir_all(&snapshots)?;
    ctx.kicad(["pcb", "export", "glb", "--output"])?
        .arg(target.join(format!("{name}.glb")))
        .args(["--force", "--include-tracks", "--include-pads", "--include-zones", "--include-inner-copper"])
        .args(["--include-silkscreen", "--include-soldermask"])
        .arg(&ctx.pcb)
        .run_quiet()?;
    let common = RENDER_COMMON;
    let mut views = Vec::new();
    for angle in (0..360).step_by(40) {
        let image = snapshots.join(format!("view-{angle}.png"));
        ctx.kicad(["pcb", "render", "--output"])?
            .arg(&image)
            .args(["--width", "960", "--height", "720", "--quality", "high"])
            .args(common)
            .args(["--perspective", "--rotate", &format!("-45,0,{angle}")])
            .arg(&ctx.pcb)
            .run_quiet()?;
        views.push(image);
    }
    let style = ["-background", "#20252b", "-bordercolor", "#20252b", "-border", "4"];
    Cmd::new(&ctx.tools.magick)
        .arg("montage")
        .args(&views)
        .args(["-tile", "3x3", "-geometry", "960x720+10+10"])
        .args(style)
        .arg(target.join(format!("{name}-9-angle.png")))
        .run_quiet()?;
    Cmd::new(&ctx.tools.magick)
        .arg("montage")
        .args([&top, &bottom, &snapshots.join("view-40.png")])
        .args(["-tile", "3x1", "-geometry", "960x720+10+10"])
        .args(style)
        .arg(target.join(format!("{name}-3-view.png")))
        .run_quiet()?;
    Ok(())
}

fn packages(ctx: &Ctx, complete: bool) -> Result<()> {
    let release = ctx.out.join("release");
    let fab = ctx.artifact("fabrication", "");
    std::fs::create_dir_all(&release)?;
    let gerbers = ctx.artifact("release", "-jlcpcb-gerbers.zip");
    zip_files(&gerbers, &ctx.dir, std::slice::from_ref(&fab), true)?;
    // Everything JLCPCB's upload form asks for, side by side.
    let upload = ctx.out.join("jlcpcb");
    std::fs::create_dir_all(&upload)?;
    for source in [gerbers, ctx.artifact("bom", "-jlcpcb-bom.csv"), ctx.artifact("assembly", "-cpl.csv")] {
        copy_with_times(&source, &upload.join(source.file_name().unwrap_or_default()))?;
    }
    if complete {
        let mut files = vec![ctx.pcb.clone(), ctx.sch.clone(), ctx.pro.clone()];
        files.extend(ctx.board.project.release_files.iter().map(|f| ctx.dir.join(f)));
        files.extend([
            fab,
            ctx.artifact("assembly", "-position.csv"),
            ctx.artifact("assembly", "-cpl.csv"),
            ctx.artifact("bom", "-bom.csv"),
            ctx.artifact("bom", "-jlcpcb-bom.csv"),
            ctx.artifact("render", ""),
            ctx.artifact("reports", "-drc.rpt"),
            ctx.artifact("reports", "-erc.rpt"),
            ctx.artifact("netlists", "-netlist.csv"),
            ctx.out.join("digikey-shopping-list.csv"),
        ]);
        zip_files(&ctx.artifact("release", "-complete.zip"), &ctx.dir, &files, false)?;
    }
    Ok(())
}

pub fn export(ctx: &Ctx) -> Result<()> {
    lock::verify(&ctx.lock, &ctx.sources())?;
    let rows = bom::validate(&ctx.board.project, &ctx.dir.join(&ctx.board.project.bom), &ctx.pcb)?;
    export_boms(ctx, &rows)?;
    write_netlist(ctx)?;
    fabrication(ctx)?;
    assembly(ctx, &rows)?;
    packages(ctx, false)
}

pub fn prep(ctx: &Ctx) -> Result<()> {
    // Release generation never modifies the reviewed board sources.
    let rows = core_checks(ctx)?;
    export_boms(ctx, &rows)?;
    assembly(ctx, &rows)?;
    schematic_image(ctx, &ctx.artifact("render", ""))?;
    fabrication(ctx)?;
    render(ctx, true)?;
    packages(ctx, true)?;
    println!("Manufacturing outputs ready in {}", ctx.out.display());
    Ok(())
}

pub fn lock_board(ctx: &Ctx) -> Result<()> {
    lock::lock(&ctx.lock, &ctx.sources(), &ctx.board.project)
}

/// Remove `output/`, and with `all` the KiCad files `bb` generates. Files
/// git tracks are kept: in a board repository the committed `.kicad_pcb`
/// and `.kicad_sch` are the reviewed, routed board.
/// Remove generated output. The KiCad project is kept (it may hold a routed
/// board, which cannot be regenerated identically) unless `all` is set, and
/// files tracked by git are never removed.
pub fn clean(ctx: &Ctx, all: bool) -> Result<()> {
    let kicad_dir = ctx.pcb.parent().unwrap_or(&ctx.dir).to_path_buf();
    let project: Vec<PathBuf> = ["kicad_pcb", "kicad_pro", "kicad_prl", "kicad_sch", "bb.json"]
        .map(|ext| kicad_dir.join(format!("{}.{ext}", ctx.name())))
        .into_iter()
        .chain([kicad_dir.join("fp-info-cache"), kicad_dir.join(format!("{}-backups", ctx.name()))])
        .collect();
    let keep = |path: &Path| !all && project.iter().any(|p| p == path);

    let mut doomed: Vec<PathBuf> = Vec::new();
    if ctx.out.is_dir() {
        for entry in std::fs::read_dir(&ctx.out)? {
            let path = entry?.path();
            if !keep(&path) && !project.contains(&path) {
                doomed.push(path);
            }
        }
    }
    if all {
        doomed.extend(project.iter().filter(|p| p.exists()).cloned());
    }
    let mut removed = Vec::new();
    for path in doomed {
        if tracked_by_git(ctx, &path) {
            println!("Kept {} (tracked by git)", path.display());
            continue;
        }
        if path.is_dir() {
            std::fs::remove_dir_all(&path)?
        } else {
            std::fs::remove_file(&path)?
        }
        removed.push(path);
    }
    if ctx.out.is_dir() && std::fs::read_dir(&ctx.out)?.next().is_none() {
        std::fs::remove_dir(&ctx.out)?;
    }
    if removed.is_empty() {
        println!("Nothing to clean");
    }
    for path in &removed {
        println!("Removed {}", path.display());
    }
    if !all && project.iter().any(|p| p.exists()) {
        println!("Kept the KiCad project in {} (use --all to remove it)", kicad_dir.display());
    }
    Ok(())
}

pub(crate) fn tracked_by_git(ctx: &Ctx, path: &Path) -> bool {
    Cmd::new("git").args(["ls-files", "--error-unmatch"]).arg(path).cwd(&ctx.dir).output().ok().flatten().is_some()
}
