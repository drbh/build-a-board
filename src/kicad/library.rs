//! Stock KiCad footprint libraries (`<root>/<Library>.pretty/<Name>.kicad_mod`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context, Result, anyhow};

use crate::sexpr::{self, Sexp};
use crate::units::{Nm, Pt};

/// Footprint libraries on disk, parsed lazily and cached.
pub struct Libraries {
    root: PathBuf,
    cache: RefCell<HashMap<(String, String), Rc<Sexp>>>,
}

impl Libraries {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into(), cache: RefCell::default() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, library: &str, name: &str) -> PathBuf {
        self.root.join(format!("{library}.pretty")).join(format!("{name}.kicad_mod"))
    }

    /// Every footprint file loaded so far.
    pub fn files(&self) -> Vec<PathBuf> {
        self.cache.borrow().keys().map(|(library, name)| self.path(library, name)).collect()
    }

    pub fn load(&self, library: &str, name: &str) -> Result<Rc<Sexp>> {
        let key = (library.to_owned(), name.to_owned());
        if let Some(found) = self.cache.borrow().get(&key) {
            return Ok(found.clone());
        }
        let path = self.path(library, name);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("missing KiCad footprint {library}:{name} ({})", path.display()))?;
        let footprint = Rc::new(sexpr::parse(&text).with_context(|| format!("parsing {}", path.display()))?);
        self.cache.borrow_mut().insert(key, footprint.clone());
        Ok(footprint)
    }

    /// Board-frame centres of a library footprint's pads, in file order.
    pub fn pad_positions(&self, library: &str, name: &str, at: Pt, angle: f64) -> Result<Vec<(String, Pt)>> {
        let footprint = self.load(library, name)?;
        footprint
            .children("pad")
            .map(|pad| {
                let number = pad.arg(0).and_then(Sexp::text).unwrap_or_default().to_owned();
                Ok((number, at + local_point(pad)?.rotate(angle)))
            })
            .collect()
    }
}

/// The `(at x y ...)` of a footprint child, in footprint-local nm.
pub fn local_point(node: &Sexp) -> Result<Pt> {
    let at = node.child("at").ok_or_else(|| anyhow!("footprint item without (at): {node}"))?;
    let coord =
        |i: usize| at.arg(i).and_then(Sexp::as_f64).map(Nm::from_mm).ok_or_else(|| anyhow!("bad coordinate in {at}"));
    Ok(Pt::new(coord(0)?, coord(1)?))
}
