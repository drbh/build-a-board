//! Pretty JSON in the layout KiCad and Python's `json.dumps(indent=2)` share:
//! two-space indent, `": "` separators, ASCII-only strings, and floats as
//! shortest round-trip decimals that always carry a decimal point.

use std::fmt::Write as _;

use serde_json::Value;

pub fn to_pretty(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value, 0);
    out
}

fn write_value(out: &mut String, value: &Value, depth: usize) {
    let newline = |out: &mut String, depth: usize| {
        out.push('\n');
        out.push_str(&"  ".repeat(depth));
    };
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) if n.is_f64() => out.push_str(&float(n.as_f64().unwrap_or_default())),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => write_string(out, s),
        Value::Array(items) if items.is_empty() => out.push_str("[]"),
        Value::Object(map) if map.is_empty() => out.push_str("{}"),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                out.push_str(if i > 0 { "," } else { "" });
                newline(out, depth + 1);
                write_value(out, item, depth + 1);
            }
            newline(out, depth);
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (i, (key, item)) in map.iter().enumerate() {
                out.push_str(if i > 0 { "," } else { "" });
                newline(out, depth + 1);
                write_string(out, key);
                out.push_str(": ");
                write_value(out, item, depth + 1);
            }
            newline(out, depth);
            out.push('}');
        }
    }
}

fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            c => {
                for unit in c.encode_utf16(&mut [0; 2]) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
        }
    }
    out.push('"');
}

/// Shortest round-trip digits; exponent form outside 1e-4..1e16.
fn float(value: f64) -> String {
    let sci = format!("{value:e}");
    let (mantissa, exponent) = sci.split_once('e').expect("float exponent");
    let exponent: i32 = exponent.parse().expect("float exponent");
    let (sign, mantissa) = mantissa.strip_prefix('-').map_or(("", mantissa), |m| ("-", m));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    if !(-4..16).contains(&exponent) {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() { String::new() } else { format!(".{rest}") };
        return format!("{sign}{first}{rest}e{}{:02}", if exponent < 0 { '-' } else { '+' }, exponent.unsigned_abs());
    }
    let point = exponent + 1;
    let body = if point <= 0 {
        format!("0.{}{digits}", "0".repeat(point.unsigned_abs() as usize))
    } else if point as usize >= digits.len() {
        format!("{digits}{}.0", "0".repeat(point as usize - digits.len()))
    } else {
        let (whole, frac) = digits.split_at(point as usize);
        format!("{whole}.{frac}")
    };
    format!("{sign}{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_keep_a_decimal_point() {
        for (value, text) in [(1.0, "1.0"), (0.05, "0.05"), (0.00001, "1e-05"), (1.5, "1.5"), (2.5e20, "2.5e+20")] {
            assert_eq!(float(value), text);
        }
    }

    #[test]
    fn layout_matches_kicad() {
        let value: Value = serde_json::from_str(r#"{"a": [], "b": {"c": 1, "d": 0.25, "e": "µ"}}"#).unwrap();
        assert_eq!(
            to_pretty(&value),
            "{\n  \"a\": [],\n  \"b\": {\n    \"c\": 1,\n    \"d\": 0.25,\n    \"e\": \"\\u00b5\"\n  }\n}"
        );
    }
}
