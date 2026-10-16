//! `bb generate`: turn a board file into a filled, canonical `.kicad_pcb`.
//!
//! 1. Trace artwork into generated silkscreen footprints.
//! 2. Write the board and its project settings.
//! 3. Canonicalize item order, then let KiCad fill the zones and re-save
//!    (`kicad-cli pcb drc --refill-zones --save-board`). KiCad's fill depends
//!    on the order items appear in the file, so filling from canonical order
//!    is what makes the result reproducible.
//! 4. Apply the board finish to the stackup and canonicalize again.

use std::path::Path;

use anyhow::{Context, Result};

use crate::artwork;
use crate::board::{Artwork, BoardFile};
use crate::design::{self, Footprint, GeneratedFootprint, Graphic, Source};
use crate::kicad::{Libraries, normalize, project, writer};
use crate::polygon::kicad_fracture;
use crate::process::Tools;
use crate::units::{Nm, Pt};

/// Paths of the generated KiCad files.
pub struct Outputs<'a> {
    pub board: &'a Path,
    pub project: &'a Path,
}

pub fn generate(board: &BoardFile, tools: &Tools, out: Outputs<'_>) -> Result<()> {
    let mut design = board.design.clone();
    for art in &board.artwork {
        let footprint = trace(art).with_context(|| format!("tracing {}", art.image.display()))?;
        if let Source::Generated(generated) = &footprint.source {
            println!("Traced {} into {} silkscreen polygons", art.image.display(), generated.graphics.len());
        }
        design.footprints.push(footprint);
    }
    let libraries = Libraries::new(&tools.footprints);
    if let Some(dir) = out.board.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let (canonical, _) = normalize::normalize(&writer::write(&design, &libraries)?, &board.project.name)?;
    std::fs::write(out.board, canonical).with_context(|| format!("writing {}", out.board.display()))?;
    project::update(out.project, &design)?;

    let report = tempfile::Builder::new().suffix(".rpt").tempfile()?;
    tools
        .kicad(["pcb", "drc", "--refill-zones", "--save-board", "--output"])?
        .arg(report.path())
        .arg(out.board)
        .run_quiet()?;

    let mut text = std::fs::read_to_string(out.board)?;
    if let Some(finish) = &design.finish {
        text = apply_finish(&text, finish)?;
    }
    let (text, stats) = normalize::normalize(&text, &board.project.name)?;
    std::fs::write(out.board, text)?;
    println!("Generated {} ({} items, {} UUIDs canonicalized)", out.board.display(), stats.items, stats.uuids);
    Ok(())
}

/// A traced image as a generated footprint centred on the artwork.
fn trace(art: &Artwork) -> Result<Footprint> {
    let traced = artwork::trace(&art.image, art.width, art.tone)?;
    let origin = Pt::mm(art.center.0, art.center.1);
    let to_nm = |p: &(i32, i32)| {
        let (x, y) = traced.to_mm(art.center, *p);
        (Nm::from_design_mm(x).0, Nm::from_design_mm(y).0)
    };
    let mut graphics = Vec::new();
    for shape in &traced.shapes {
        let outline: Vec<(i64, i64)> = shape.outline.iter().map(to_nm).collect();
        let holes: Vec<Vec<(i64, i64)>> = shape.holes.iter().map(|h| h.iter().map(to_nm).collect()).collect();
        // pcbnew saves only the first outline of a polygon; a fracture that
        // splits a shape in two therefore loses the second piece on save.
        let Some(saved) = kicad_fracture(&outline, &holes).into_iter().next() else { continue };
        let points = saved.into_iter().map(|(x, y)| Pt::new(Nm(x), Nm(y)) - origin).collect();
        graphics.push(Graphic::Poly { layer: art.layer, points });
    }
    Ok(Footprint {
        reference: art.reference.clone(),
        value: art.value.clone(),
        at: origin,
        angle: 0.0,
        hide_reference: true,
        source: Source::Generated(GeneratedFootprint {
            library: art.library.clone(),
            name: art.name.clone(),
            graphics,
            exclude_from_bom: true,
            exclude_from_pos: true,
            ..Default::default()
        }),
    })
}

/// Write the mask/silk colours and copper finish into the board's stackup.
fn apply_finish(text: &str, finish: &design::Finish) -> Result<String> {
    if text.contains("\n\t\t(stackup\n") {
        let text = regex::Regex::new(r#"\(layer "([FB])\.Mask"((?:\s*\([^()]*\))*?)\s*\(color "[^"]*"\)"#)?
            .replace_all(text, |c: &regex::Captures| {
                format!("(layer \"{}.Mask\"{} (color \"{}\")", &c[1], &c[2], finish.mask)
            });
        let text = regex::Regex::new(r#"\(copper_finish "[^"]*"\)"#)?
            .replace_all(&text, format!("(copper_finish \"{}\")", finish.copper).as_str());
        return Ok(text.into_owned());
    }
    let (mask, silk, copper) = (&finish.mask, &finish.silk, &finish.copper);
    let stackup = format!(
        "\t\t(stackup\n\
\t\t\t(layer \"F.SilkS\" (type \"Top Silk Screen\") (color \"{silk}\"))\n\
\t\t\t(layer \"F.Paste\" (type \"Top Solder Paste\"))\n\
\t\t\t(layer \"F.Mask\" (type \"Top Solder Mask\") (color \"{mask}\") (thickness 0.01))\n\
\t\t\t(layer \"F.Cu\" (type \"copper\") (thickness 0.035))\n\
\t\t\t(layer \"dielectric 1\" (type \"core\") (thickness 1.51) (material \"FR4\") (epsilon_r 4.5) (loss_tangent 0.02))\n\
\t\t\t(layer \"B.Cu\" (type \"copper\") (thickness 0.035))\n\
\t\t\t(layer \"B.Mask\" (type \"Bottom Solder Mask\") (color \"{mask}\") (thickness 0.01))\n\
\t\t\t(layer \"B.Paste\" (type \"Bottom Solder Paste\"))\n\
\t\t\t(layer \"B.SilkS\" (type \"Bottom Silk Screen\") (color \"{silk}\"))\n\
\t\t\t(copper_finish \"{copper}\")\n\
\t\t\t(dielectric_constraints no)\n\
\t\t)\n"
    );
    let marker = "\t(setup\n";
    anyhow::ensure!(text.contains(marker), "could not find the board's setup block");
    Ok(text.replacen(marker, &format!("{marker}{stackup}"), 1))
}
