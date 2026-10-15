//! Exact board positions as Starlark values.
//!
//! `pin("U1", 8)` returns an [`Anchor`]: a point held in nanometres, so
//! routing to a pad lands exactly on it. Plain `(x, y)` tuples of
//! millimetres are accepted everywhere an anchor is, and an anchor can be
//! read back as millimetres with `a.x`, `a.y`, or `a[0]`, `a[1]`.

use std::fmt;

use allocative::Allocative;
use starlark::starlark_simple_value;
use starlark::typing::Ty;
use starlark::values::float::UnpackFloat;
use starlark::values::type_repr::StarlarkTypeRepr;
use starlark::values::{
    Heap, NoSerialize, ProvidesStaticType, StarlarkPagable, StarlarkValue, UnpackValue, Value, ValueLike,
    starlark_value,
};

use crate::units::{Nm, Pt};

#[derive(Debug, Clone, Copy, PartialEq, ProvidesStaticType, NoSerialize, StarlarkPagable, Allocative)]
pub struct Anchor {
    x: i64,
    y: i64,
}
starlark_simple_value!(Anchor);

impl Anchor {
    pub fn new(at: Pt) -> Self {
        Self { x: at.x.0, y: at.y.0 }
    }

    fn point(self) -> Pt {
        Pt::new(Nm(self.x), Nm(self.y))
    }
}

impl fmt::Display for Anchor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "anchor({}, {})", Nm(self.x), Nm(self.y))
    }
}

#[starlark_value(type = "anchor")]
impl<'v> StarlarkValue<'v> for Anchor {
    fn at(&self, index: Value<'v>, heap: Heap<'v>) -> starlark::Result<Value<'v>> {
        match index.unpack_i32() {
            Some(0) => Ok(heap.alloc(Nm(self.x).to_mm())),
            Some(1) => Ok(heap.alloc(Nm(self.y).to_mm())),
            _ => Err(starlark::Error::new_other(anyhow::anyhow!("anchor index must be 0 or 1"))),
        }
    }

    fn get_attr(&self, attribute: &str, heap: Heap<'v>) -> Option<Value<'v>> {
        match attribute {
            "x" => Some(heap.alloc(Nm(self.x).to_mm())),
            "y" => Some(heap.alloc(Nm(self.y).to_mm())),
            _ => None,
        }
    }

    fn length(&self) -> starlark::Result<i32> {
        Ok(2)
    }

    fn iterate_collect(&self, heap: Heap<'v>) -> starlark::Result<Vec<Value<'v>>> {
        Ok(vec![heap.alloc(Nm(self.x).to_mm()), heap.alloc(Nm(self.y).to_mm())])
    }

    fn equals(&self, other: Value<'v>) -> starlark::Result<bool> {
        Ok(other.downcast_ref::<Self>().is_some_and(|o| o == self))
    }
}

/// A point argument: an exact [`Anchor`] or an `(x, y)` tuple in mm.
#[derive(Clone, Copy, Debug)]
pub enum At {
    Exact(Pt),
    Mm(f64, f64),
}

impl At {
    /// The board-frame point, converting millimetres like pcbnew's `FromMM`.
    pub fn pt(self) -> Pt {
        match self {
            Self::Exact(p) => p,
            Self::Mm(x, y) => Pt::mm(x, y),
        }
    }

    /// Millimetres, for arithmetic such as footprint-local offsets.
    pub fn mm(self) -> (f64, f64) {
        match self {
            Self::Exact(p) => (p.x.to_mm(), p.y.to_mm()),
            Self::Mm(x, y) => (x, y),
        }
    }
}

impl StarlarkTypeRepr for At {
    type Canonical = <(UnpackFloat, UnpackFloat) as StarlarkTypeRepr>::Canonical;

    fn starlark_type_repr() -> Ty {
        <(UnpackFloat, UnpackFloat) as StarlarkTypeRepr>::starlark_type_repr()
    }
}

impl<'v> UnpackValue<'v> for At {
    type Error = starlark::Error;

    fn unpack_value_impl(value: Value<'v>) -> starlark::Result<Option<Self>> {
        if let Some(anchor) = Anchor::from_value(value) {
            return Ok(Some(Self::Exact(anchor.point())));
        }
        let pair = <(UnpackFloat, UnpackFloat)>::unpack_value_impl(value)
            .map_err(|e| starlark::Error::new_other(anyhow::anyhow!("{e:?}")))?;
        Ok(pair.map(|(x, y)| Self::Mm(x.0, y.0)))
    }
}
