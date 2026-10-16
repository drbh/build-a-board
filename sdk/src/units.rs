//! Electrical values with units, and the BOM-style strings they print as.
//!
//! Values are stored as integers (milliohms, picofarads) so they format
//! exactly: `kohm(4.7)` prints `4.7k`, `nf(100)` prints `100n`.

use std::fmt;

/// Resistance, in milliohms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ohms(u64);

/// Capacitance, in picofarads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Farads(u64);

/// Voltage.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Volts(pub f64);

pub fn ohm(value: impl Into<f64>) -> Ohms {
    Ohms((value.into() * 1e3).round() as u64)
}

pub fn kohm(value: impl Into<f64>) -> Ohms {
    Ohms((value.into() * 1e6).round() as u64)
}

pub fn mohm(value: impl Into<f64>) -> Ohms {
    Ohms((value.into() * 1e9).round() as u64)
}

pub fn pf(value: impl Into<f64>) -> Farads {
    Farads(value.into().round() as u64)
}

pub fn nf(value: impl Into<f64>) -> Farads {
    Farads((value.into() * 1e3).round() as u64)
}

pub fn uf(value: impl Into<f64>) -> Farads {
    Farads((value.into() * 1e6).round() as u64)
}

pub fn v(value: impl Into<f64>) -> Volts {
    Volts(value.into())
}

/// Resistors in series.
impl std::ops::Add for Ohms {
    type Output = Ohms;

    fn add(self, other: Ohms) -> Ohms {
        Ohms(self.0 + other.0)
    }
}

impl std::iter::Sum for Ohms {
    fn sum<I: Iterator<Item = Ohms>>(iter: I) -> Ohms {
        iter.fold(Ohms(0), |a, b| a + b)
    }
}

impl Ohms {
    pub fn ohms(self) -> f64 {
        self.0 as f64 / 1e3
    }
}

impl Farads {
    pub fn farads(self) -> f64 {
        self.0 as f64 / 1e12
    }
}

/// `numerator / denominator` as the shortest exact decimal.
fn decimal(numerator: u64, denominator: u64) -> String {
    let whole = numerator / denominator;
    let mut rest = numerator % denominator;
    if rest == 0 {
        return whole.to_string();
    }
    let mut digits = String::new();
    while rest != 0 && digits.len() < 12 {
        rest *= 10;
        digits.push(char::from(b'0' + (rest / denominator) as u8));
        rest %= denominator;
    }
    format!("{whole}.{digits}")
}

/// Engineering notation with an SI prefix and no unit symbol: `4.7k`, `100n`.
fn engineering(value: u64, base: u64, prefixes: &[(u64, &str)]) -> String {
    for &(scale, prefix) in prefixes {
        let unit = base * scale;
        if value >= unit {
            return format!("{}{prefix}", decimal(value, unit));
        }
    }
    decimal(value, base)
}

impl fmt::Display for Ohms {
    /// `4.7k`, `120k`, `10`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&engineering(self.0, 1_000, &[(1_000_000, "M"), (1_000, "k")]))
    }
}

impl fmt::Display for Farads {
    /// `10u`, `100n`, `22p`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&engineering(self.0, 1, &[(1_000_000, "u"), (1_000, "n"), (1, "p")]))
    }
}

impl fmt::Display for Volts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}V", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_print_like_a_bom() {
        assert_eq!(kohm(4.7).to_string(), "4.7k");
        assert_eq!(kohm(120).to_string(), "120k");
        assert_eq!(ohm(47).to_string(), "47");
        assert_eq!(uf(10).to_string(), "10u");
        assert_eq!(nf(100).to_string(), "100n");
        assert_eq!(uf(470).to_string(), "470u");
        assert_eq!(pf(22).to_string(), "22p");
        assert_eq!(v(50).to_string(), "50V");
        assert_eq!((kohm(120) + kohm(10)).to_string(), "130k");
    }
}
