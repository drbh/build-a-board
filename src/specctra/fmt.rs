//! KiCad's `OUTPUTFORMATTER` as the Specctra writer uses it: two-space
//! nesting, printf number formats, and DSN token quoting.

/// Accumulates output; `print` returns the characters written, including
/// indentation, as `OUTPUTFORMATTER::Print` does (line-wrapping relies on it).
#[derive(Default)]
pub struct Out {
    pub buf: String,
}

impl Out {
    pub fn print(&mut self, nest: usize, text: &str) -> usize {
        for _ in 0..nest {
            self.buf.push_str("  ");
        }
        self.buf.push_str(text);
        nest * 2 + text.len()
    }
}

/// `OUTPUTFORMATTER::GetQuoteChar` with `"` as the quote character.
pub fn quote(token: &str) -> String {
    let needs = token.is_empty()
        || token.starts_with('#')
        || token.chars().enumerate().any(|(i, c)| "\t ()%{}".contains(c) || (i > 0 && c == '-'));
    if needs { format!("\"{token}\"") } else { token.to_owned() }
}

/// printf `%.Ng`.
pub fn g(value: f64, precision: usize) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    if !value.is_finite() {
        return if value.is_nan() {
            "nan".into()
        } else if value > 0.0 {
            "inf".into()
        } else {
            "-inf".into()
        };
    }
    let p = precision.max(1);
    let sci = format!("{:.*e}", p - 1, value);
    let (mantissa, exp) = sci.split_once('e').expect("exponent");
    let exp: i32 = exp.parse().expect("exponent");
    if exp < -4 || exp >= p as i32 {
        let mantissa = trim_zeros(mantissa);
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{:02}", exp.unsigned_abs())
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        trim_zeros(&format!("{value:.decimals$}")).to_owned()
    }
}

fn trim_zeros(text: &str) -> &str {
    if text.contains('.') { text.trim_end_matches('0').trim_end_matches('.') } else { text }
}

/// `%.6g`, the format of every DSN coordinate.
pub fn g6(value: f64) -> String {
    g(value, 6)
}

/// KiCad's `FormatDouble2Str` (used for placements).
pub fn double2str(value: f64) -> String {
    if value != 0.0 && value.abs() <= 0.0001 {
        let text = format!("{value:.16}");
        trim_zeros(&text).to_owned()
    } else {
        g(value, 10)
    }
}

/// `std::fixed << std::setprecision( 6 )`.
pub fn fixed6(value: f64) -> String {
    format!("{value:.6}")
}

/// `ELEM::makeHash`'s `STRING_FORMATTER::StripUseless`.
pub fn strip_useless(text: &str) -> String {
    text.chars().filter(|c| !c.is_ascii_whitespace() && !matches!(c, '(' | ')' | '"')).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printf_g() {
        assert_eq!(g6(120000.0), "120000");
        assert_eq!(g6(-59334.5), "-59334.5");
        assert_eq!(g6(1216.229), "1216.23");
        assert_eq!(g6(1234567.0), "1.23457e+06");
        assert_eq!(g6(0.00001234), "1.234e-05");
        assert_eq!(double2str(42573.499), "42573.499");
        assert_eq!(double2str(-90.0), "-90");
    }

    #[test]
    fn quoting() {
        assert_eq!(quote("GND"), "GND");
        assert_eq!(quote("Via[0-1]_600:300_um"), "\"Via[0-1]_600:300_um\"");
        assert_eq!(quote("-1"), "-1");
        assert_eq!(quote(""), "\"\"");
    }
}
