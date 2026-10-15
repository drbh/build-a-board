//! Exact board geometry in KiCad's internal unit: integer nanometres.
//!
//! Every coordinate is converted from millimetres exactly once and all later
//! arithmetic is integer. Two conversions exist because KiCad has two:
//! its file parser rounds, while its scripting API (`pcbnew.FromMM`)
//! truncates. Design inputs use the scripting rule, so boards described in
//! bb come out byte-identical to the same boards built through pcbnew.

use std::fmt;
use std::ops::{Add, Neg, Sub};

/// A length in nanometres.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Nm(pub i64);

impl Nm {
    /// KiCad's file parser: `KiROUND(mm * 1e6)`, half away from zero.
    pub fn from_mm(mm: f64) -> Self {
        Self((mm * 1e6).round() as i64)
    }

    /// KiCad's scripting API, `pcbnew.FromMM`: `int(mm * 1e6)`, toward zero.
    pub fn from_design_mm(mm: f64) -> Self {
        Self((mm * 1e6) as i64)
    }

    /// A length round-tripped through scripting millimetres (`FromMM(ToMM(x))`),
    /// which can lose a nanometre to floating point.
    pub fn via_design_mm(self) -> Self {
        Self::from_design_mm(self.to_mm())
    }

    pub fn to_mm(self) -> f64 {
        self.0 as f64 / 1e6
    }
}

impl fmt::Display for Nm {
    /// KiCad's millimetre formatting: exact, trailing zeros trimmed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let abs = self.0.unsigned_abs();
        let (whole, frac) = (abs / 1_000_000, abs % 1_000_000);
        if frac == 0 {
            write!(f, "{sign}{whole}")
        } else {
            let frac = format!("{frac:06}");
            write!(f, "{sign}{whole}.{}", frac.trim_end_matches('0'))
        }
    }
}

impl Add for Nm {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Nm {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Neg for Nm {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

/// A board-frame point (KiCad: X right, Y down).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pt {
    pub x: Nm,
    pub y: Nm,
}

impl Pt {
    pub const ORIGIN: Self = Self { x: Nm(0), y: Nm(0) };

    pub fn new(x: Nm, y: Nm) -> Self {
        Self { x, y }
    }

    /// A design-input point (see [`Nm::from_design_mm`]).
    pub fn mm(x: f64, y: f64) -> Self {
        Self { x: Nm::from_design_mm(x), y: Nm::from_design_mm(y) }
    }

    /// KiCad's `RotatePoint` for a footprint orientation in degrees
    /// (counter-clockwise on screen). Right angles are exact.
    pub fn rotate(self, degrees: f64) -> Self {
        let (x, y) = (self.x.0, self.y.0);
        let angle = degrees.rem_euclid(360.0);
        let (x, y) = match angle {
            0.0 => (x, y),
            90.0 => (y, -x),
            180.0 => (-x, -y),
            270.0 => (-y, x),
            a => {
                let (s, c) = a.to_radians().sin_cos();
                let (fx, fy) = (x as f64, y as f64);
                ((fx * c + fy * s).round() as i64, (fy * c - fx * s).round() as i64)
            }
        };
        Self { x: Nm(x), y: Nm(y) }
    }
}

impl Add for Pt {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self { x: self.x + rhs.x, y: self.y + rhs.y }
    }
}

impl Sub for Pt {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self { x: self.x - rhs.x, y: self.y - rhs.y }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_kicad() {
        assert_eq!(Nm::from_mm(80.4975).to_string(), "80.4975");
        assert_eq!(Nm::from_design_mm(75.19999999999999).to_string(), "75.199999");
        assert_eq!(Nm::from_mm(-0.0000001).to_string(), "0");
        assert_eq!(Nm::from_mm(-1.5).to_string(), "-1.5");
        assert_eq!(Nm::from_mm(100.0).to_string(), "100");
    }

    #[test]
    fn right_angle_rotation_is_exact() {
        let p = Pt::mm(-0.825, 0.0);
        assert_eq!(p.rotate(270.0), Pt::mm(0.0, -0.825));
        assert_eq!(p.rotate(90.0), Pt::mm(0.0, 0.825));
    }
}
