//! `bb-core` for the browser playground.
//!
//! The page compiles a board in the browser and calls its `bb-design-json`
//! export; this resolves that design against the footprints the page has
//! fetched and draws the same SVG `bb preview` draws, with what changed since
//! the last drawing highlighted.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context as _, Result, anyhow};
use bb_core::artwork::{ArtworkImage, ArtworkSource};
use bb_core::footprint::FootprintSource;
use bb_core::preview::{Scene, svg};
use bb_core::sexpr::{self, Sexp};
use bb_core::wit;
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// Footprints the page has handed over, by `Library:Name`.
#[derive(Default)]
struct Footprints(HashMap<String, Rc<Sexp>>);

impl FootprintSource for Footprints {
    fn load(&self, library: &str, name: &str) -> Result<Rc<Sexp>> {
        self.0
            .get(&format!("{library}:{name}"))
            .cloned()
            .ok_or_else(|| anyhow!("footprint {library}:{name} was not loaded"))
    }
}

/// Artwork images exist for the resolver but are not shown: a playground
/// board has no files beside it.
struct NoImages;

impl ArtworkSource for NoImages {
    fn locate(&self, image: &str) -> Option<PathBuf> {
        Some(PathBuf::from(image))
    }

    fn image(&self, _: &Path) -> Option<ArtworkImage> {
        None
    }
}

/// What a drawing returns to the page.
#[derive(Serialize)]
struct Drawing {
    svg: String,
    /// What changed since the last drawing, if anything visible did.
    changes: Option<String>,
    status: String,
    warnings: Vec<String>,
    /// What the placement checks found: copper off the board or too near its
    /// edge, overlapping courtyards.
    findings: Vec<Finding>,
    parts: usize,
    unrouted: usize,
}

#[derive(Serialize)]
struct Finding {
    severity: &'static str,
    message: String,
}

/// Library footprints a design places, as `Library:Name`.
fn footprint_ids(design: &wit::Design) -> Vec<String> {
    let mut ids: Vec<String> = design
        .items
        .iter()
        .filter_map(|item| match &item.shape {
            wit::Shape::Placement(p) => match &p.footprint {
                wit::Footprint::Library(part) => Some(part.footprint.clone()),
                wit::Footprint::Generated(_) => None,
            },
            _ => None,
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

fn parse(design: &str) -> Result<wit::Design> {
    serde_json::from_str(design).context("reading the design")
}

fn js(err: anyhow::Error) -> JsError {
    JsError::new(&format!("{err:#}"))
}

/// Resolves and draws designs, remembering the last drawing for change highlights.
#[wasm_bindgen]
#[derive(Default)]
pub struct Renderer {
    footprints: Footprints,
    previous: Option<Scene>,
    /// The scene before the last visible change, so a rebuild that changes
    /// nothing keeps showing what the last edit did.
    baseline: Option<Scene>,
}

#[wasm_bindgen]
impl Renderer {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Renderer {
        Renderer::default()
    }

    /// The `Library:Name` footprints `design` (JSON) places that have not been added yet.
    pub fn missing_footprints(&self, design: &str) -> Result<Vec<String>, JsError> {
        let design = parse(design).map_err(js)?;
        Ok(footprint_ids(&design).into_iter().filter(|id| !self.footprints.0.contains_key(id)).collect())
    }

    /// Hand over a footprint's `.kicad_mod` text.
    pub fn add_footprint(&mut self, id: &str, text: &str) -> Result<(), JsError> {
        let parsed = sexpr::parse(text).map_err(|e| JsError::new(&format!("parsing {id}: {e}")))?;
        self.footprints.0.insert(id.to_owned(), Rc::new(parsed));
        Ok(())
    }

    /// Resolve `design` (JSON) and draw it: `{svg, changes, status, warnings, parts, unrouted}`.
    pub fn render(&mut self, design: &str) -> Result<JsValue, JsError> {
        let design = parse(design).map_err(js)?;
        let resolved = bb_core::resolve::resolve(design, &self.footprints, &NoImages).map_err(js)?;
        let scene =
            Scene::new(&resolved.project.name, &resolved.design, &resolved.artwork, &self.footprints, &NoImages, None)
                .map_err(js)?;
        let (mut svg, mut changes) = svg::svg(&scene, self.previous.as_ref());
        if changes.as_deref() == Some(svg::NO_CHANGE) {
            if let Some(baseline) = &self.baseline {
                (svg, changes) = svg::svg(&scene, Some(baseline));
            }
        } else if self.previous.is_some() {
            self.baseline = self.previous.take();
        }
        let drawing = Drawing {
            svg,
            changes,
            status: svg::status_line(&scene, &svg::Status::default()),
            warnings: resolved.warnings,
            findings: scene
                .findings
                .iter()
                .map(|f| Finding { severity: f.severity.name(), message: f.message.clone() })
                .collect(),
            parts: scene.footprints.len(),
            unrouted: scene.ratsnest.len(),
        };
        self.previous = Some(scene);
        serde_wasm_bindgen::to_value(&drawing).map_err(|e| JsError::new(&e.to_string()))
    }
}
