//! The design as JSON, for boards built for the browser playground.
//!
//! A board compiled in the browser is a plain WebAssembly module rather than
//! a component, so it hands its design to `bb-core` as JSON through the
//! `bb-design-json` export (see [`export_board!`](crate::export_board)). The
//! shape is serde's default for the WIT types: records as objects keyed by
//! field, enums as `"Case"`, variants as `"Case"` or `{"Case": payload}`,
//! options as `null` or the value. The impls for the WIT types are generated
//! from `wit/` into `design_json.rs`.

use crate::bindings::bb::board::types as w;

/// Writes a value as JSON.
pub trait ToJson {
    fn to_json(&self, out: &mut String);
}

/// `,"name":` (or `{"name":` and so on): a separator, then a key.
fn key(out: &mut String, before: &str, name: &str) {
    out.push_str(before);
    out.push('"');
    out.push_str(name);
    out.push_str("\":");
}

impl ToJson for String {
    fn to_json(&self, out: &mut String) {
        out.push('"');
        for c in self.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
                c => out.push(c),
            }
        }
        out.push('"');
    }
}

impl ToJson for f64 {
    /// Shortest round-trip form; JSON has no NaN or infinity (and the
    /// sandbox never produces them), so those become `null`.
    fn to_json(&self, out: &mut String) {
        if self.is_finite() {
            out.push_str(&self.to_string());
        } else {
            out.push_str("null");
        }
    }
}

impl ToJson for u32 {
    fn to_json(&self, out: &mut String) {
        out.push_str(&self.to_string());
    }
}

impl ToJson for bool {
    fn to_json(&self, out: &mut String) {
        out.push_str(if *self { "true" } else { "false" });
    }
}

impl<T: ToJson> ToJson for Option<T> {
    fn to_json(&self, out: &mut String) {
        match self {
            Some(value) => value.to_json(out),
            None => out.push_str("null"),
        }
    }
}

impl<T: ToJson> ToJson for Vec<T> {
    fn to_json(&self, out: &mut String) {
        out.push('[');
        for (i, value) in self.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            value.to_json(out);
        }
        out.push(']');
    }
}

impl<A: ToJson, B: ToJson> ToJson for (A, B) {
    fn to_json(&self, out: &mut String) {
        out.push('[');
        self.0.to_json(out);
        out.push(',');
        self.1.to_json(out);
        out.push(']');
    }
}

impl<A: ToJson, B: ToJson, C: ToJson> ToJson for (A, B, C) {
    fn to_json(&self, out: &mut String) {
        out.push('[');
        self.0.to_json(out);
        out.push(',');
        self.1.to_json(out);
        out.push(',');
        self.2.to_json(out);
        out.push(']');
    }
}

mod generated {
    #![allow(clippy::all)] // generated from wit/ by bb-sdk-bindgen
    use super::{ToJson, key};
    include!("design_json.rs");
}

/// The design as JSON.
pub fn to_json(design: &w::Design) -> String {
    let mut out = String::new();
    design.to_json(&mut out);
    out
}

/// `bb-design-json`'s result: the design's JSON, left in memory, as
/// `pointer << 32 | length` (a browser board is built once per module).
#[doc(hidden)]
pub fn design_json(board: crate::Board) -> u64 {
    let json: &'static [u8] = Box::leak(to_json(&crate::to_wit(board)).into_bytes().into_boxed_slice());
    ((json.as_ptr() as usize as u64) << 32) | json.len() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_and_numbers_are_valid_json() {
        let mut out = String::new();
        ("a \"q\" \\ \n\u{1}".to_owned(), 0.1f64).to_json(&mut out);
        assert_eq!(out, r#"["a \"q\" \\ \n\u0001",0.1]"#);
        out.clear();
        (vec![1u32, 2], Some(f64::NAN), true).to_json(&mut out);
        assert_eq!(out, "[[1,2],null,true]");
    }
}
