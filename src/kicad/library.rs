//! Stock KiCad footprint libraries (`<root>/<Library>.pretty/<Name>.kicad_mod`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context, Result};
use bb_core::footprint::FootprintSource;

use crate::sexpr::{self, Sexp};

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
}

impl FootprintSource for Libraries {
    fn load(&self, library: &str, name: &str) -> Result<Rc<Sexp>> {
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
}
