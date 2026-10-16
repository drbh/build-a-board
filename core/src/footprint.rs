//! Stock KiCad footprints, wherever they are kept.

use std::rc::Rc;

use anyhow::{Result, anyhow};

use crate::sexpr::Sexp;
use crate::units::{Nm, Pt};

/// Where library footprints come from: `bb` reads `<root>/<Library>.pretty/
/// <Name>.kicad_mod` from disk, a browser serves whatever it was given.
pub trait FootprintSource {
    /// The parsed footprint `library:name`.
    fn load(&self, library: &str, name: &str) -> Result<Rc<Sexp>>;

    /// Board-frame centres of a library footprint's pads, in file order.
    fn pad_positions(&self, library: &str, name: &str, at: Pt, angle: f64) -> Result<Vec<(String, Pt)>> {
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
