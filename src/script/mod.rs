//! Board files: Starlark programs that describe a board with `bb`'s primitives.
//!
//! A board directory holds `board.star` plus the inputs it names (artwork,
//! parts lists). The file runs top to bottom; each primitive call adds to
//! the design. Coordinates are millimetres in KiCad's board frame (Y down),
//! angles are degrees. Other `.star` files in the directory can hold shared
//! components and be imported with `load("parts.star", "buck")`.
//!
//! Optional top-level hooks the tool calls later:
//! - `def route_width(track)`: after autorouting, return a new width in mm
//!   for a routed track (a struct with `net`, `layer`, `width`, `start`,
//!   `end`), or `None` to keep it.

mod anchor;
mod api;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use starlark::any::ProvidesStaticType;
use starlark::environment::{FrozenModule, Globals, GlobalsBuilder, LibraryExtension, Module};
use starlark::eval::{Evaluator, FileLoader};
use starlark::syntax::{AstModule, Dialect};
use starlark::values::float::UnpackFloat;
use starlark::values::structs::AllocStruct;
use starlark::values::{UnpackValue as _, Value};

use crate::design;
use crate::kicad::Libraries;
use crate::units::Nm;

pub use api::{Artwork, Project, Restore, SchematicConfig};

/// The evaluated board file.
pub struct BoardFile {
    pub dir: PathBuf,
    pub design: design::Board,
    pub project: Project,
    pub artwork: Vec<Artwork>,
    pub restore: Vec<Restore>,
    pub unlink_after_route: Option<String>,
    /// Every file the design was built from: the board file, loaded
    /// modules, artwork, and the library footprints it placed.
    pub inputs: Vec<PathBuf>,
    file: PathBuf,
    footprint_root: PathBuf,
}

/// Mutable state the primitives write into while the file runs.
#[derive(ProvidesStaticType)]
pub(crate) struct EvalContext {
    pub dir: PathBuf,
    pub libraries: Libraries,
    pub state: RefCell<api::State>,
}

fn globals() -> Globals {
    GlobalsBuilder::extended_by(&[
        LibraryExtension::StructType,
        LibraryExtension::Print,
        LibraryExtension::Map,
        LibraryExtension::Filter,
    ])
    .with(api::primitives)
    .build()
}

fn dialect() -> Dialect {
    Dialect { enable_f_strings: true, ..Dialect::Extended }
}

/// Resolves `load("x.star", ...)` relative to the board directory, running
/// each module once with the same primitives and design state.
struct Loader<'a> {
    context: &'a EvalContext,
    globals: &'a Globals,
    loaded: RefCell<HashMap<PathBuf, FrozenModule>>,
}

impl FileLoader for Loader<'_> {
    fn load(&self, path: &str) -> starlark::Result<FrozenModule> {
        let file = self.context.dir.join(path);
        if let Some(module) = self.loaded.borrow().get(&file) {
            return Ok(module.clone());
        }
        let module = run(&file, self.context, self.globals, self).map_err(starlark::Error::from)?;
        self.loaded.borrow_mut().insert(file, module.clone());
        Ok(module)
    }
}

fn run(file: &Path, context: &EvalContext, globals: &Globals, loader: &dyn FileLoader) -> Result<FrozenModule> {
    let source = std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    let ast = AstModule::parse(&file.display().to_string(), source, &dialect()).map_err(|e| anyhow!("{e}"))?;
    Module::with_temp_heap(|module| {
        {
            let mut eval = Evaluator::new(&module);
            eval.extra = Some(context);
            eval.set_loader(loader);
            eval.eval_module(ast, globals).map_err(|e| anyhow!("{e}"))?;
        }
        module.freeze().map_err(|e| anyhow!("{e:?}"))
    })
}

impl BoardFile {
    /// Evaluate `board.star` (or the given file) and resolve the design.
    pub fn load(path: &Path, footprint_root: &Path) -> Result<Self> {
        let file = if path.is_dir() { path.join("board.star") } else { path.to_path_buf() };
        let dir = file.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let context =
            EvalContext { dir: dir.clone(), libraries: Libraries::new(footprint_root), state: RefCell::default() };
        let globals = globals();
        let loader = Loader { context: &context, globals: &globals, loaded: RefCell::default() };
        run(&file, &context, &globals, &loader)?;
        let mut inputs = vec![file.clone()];
        inputs.extend(loader.loaded.into_inner().into_keys());
        inputs.extend(context.libraries.files());
        let state = context.state.into_inner();
        inputs.extend(state.artwork.iter().map(|art| art.image.clone()));
        inputs.sort();
        inputs.dedup();
        let project = state.project.ok_or_else(|| anyhow!("{}: no project(...) declared", file.display()))?;
        Ok(Self {
            dir,
            design: state.board,
            project,
            artwork: state.artwork,
            restore: state.restore,
            unlink_after_route: state.unlink_after_route,
            inputs,
            file,
            footprint_root: footprint_root.to_path_buf(),
        })
    }

    /// Ask the board's `route_width(track)` hook for each routed track's
    /// new width. The file is re-run so the hook executes in a live module.
    pub fn route_widths(&self, tracks: &[design::Track]) -> Result<Vec<Option<Nm>>> {
        let context = EvalContext {
            dir: self.dir.clone(),
            libraries: Libraries::new(&self.footprint_root),
            state: RefCell::default(),
        };
        let globals = globals();
        let loader = Loader { context: &context, globals: &globals, loaded: RefCell::default() };
        let source = std::fs::read_to_string(&self.file).with_context(|| format!("reading {}", self.file.display()))?;
        let ast = AstModule::parse(&self.file.display().to_string(), source, &dialect()).map_err(|e| anyhow!("{e}"))?;
        let mm = |nm: Nm| nm.to_mm();
        Module::with_temp_heap(|module| {
            let mut eval = Evaluator::new(&module);
            eval.extra = Some(&context);
            eval.set_loader(&loader);
            eval.eval_module(ast, &globals).map_err(|e| anyhow!("{e}"))?;
            let Some(hook) = module.get("route_width") else { return Ok(vec![None; tracks.len()]) };
            let heap = module.heap();
            tracks
                .iter()
                .map(|track| {
                    let value = heap.alloc(AllocStruct([
                        ("net", heap.alloc(track.net.as_str())),
                        ("layer", heap.alloc(track.layer.name())),
                        ("width", heap.alloc(mm(track.width))),
                        ("start", heap.alloc((mm(track.start.x), mm(track.start.y)))),
                        ("end", heap.alloc((mm(track.end.x), mm(track.end.y)))),
                    ]));
                    let result: Value = eval.eval_function(hook, &[value], &[]).map_err(|e| anyhow!("{e}"))?;
                    if result.is_none() {
                        return Ok(None);
                    }
                    let width = UnpackFloat::unpack_value_err(result).map_err(|e| anyhow!("route_width: {e}"))?;
                    Ok(Some(Nm::from_design_mm(width.0)))
                })
                .collect()
        })
    }
}
