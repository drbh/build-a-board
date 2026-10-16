//! KiCad-flavoured s-expressions: a small tree, a parser, and a printer that
//! reproduces KiCad's own `KICAD_FORMAT::Prettify` layout.

use std::fmt::{self, Write as _};

use thiserror::Error;

/// One s-expression node.
#[derive(Clone, Debug, PartialEq)]
pub enum Sexp {
    /// A bare token: keyword, number, or symbol (`segment`, `0.25`, `yes`).
    Atom(String),
    /// A double-quoted string, stored unescaped.
    Str(String),
    List(Vec<Sexp>),
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("unexpected end of input")]
    Eof,
    #[error("unbalanced ')' at byte {0}")]
    Unbalanced(usize),
    #[error("trailing data after the top-level expression at byte {0}")]
    Trailing(usize),
}

impl Sexp {
    pub fn atom(text: impl Into<String>) -> Self {
        Self::Atom(text.into())
    }

    pub fn str(text: impl Into<String>) -> Self {
        Self::Str(text.into())
    }

    /// `(head items...)`.
    pub fn node(head: &str, items: impl IntoIterator<Item = Sexp>) -> Self {
        Self::List(std::iter::once(Self::atom(head)).chain(items).collect())
    }

    pub fn as_list(&self) -> Option<&[Sexp]> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_list_mut(&mut self) -> Option<&mut Vec<Sexp>> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }

    /// The token or string text of a leaf.
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Atom(text) | Self::Str(text) => Some(text),
            Self::List(_) => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Atom(text) => text.parse().ok(),
            _ => None,
        }
    }

    /// The leading keyword of a list, e.g. `footprint` in `(footprint ...)`.
    pub fn head(&self) -> Option<&str> {
        match self.as_list()?.first()? {
            Self::Atom(head) => Some(head),
            _ => None,
        }
    }

    pub fn is(&self, head: &str) -> bool {
        self.head() == Some(head)
    }

    /// Items after the head.
    pub fn args(&self) -> &[Sexp] {
        self.as_list().map_or(&[], |items| items.get(1..).unwrap_or(&[]))
    }

    pub fn arg(&self, index: usize) -> Option<&Sexp> {
        self.args().get(index)
    }

    /// Child lists with the given head.
    pub fn children<'a>(&'a self, head: &'a str) -> impl Iterator<Item = &'a Sexp> + 'a {
        self.args().iter().filter(move |child| child.is(head))
    }

    pub fn child(&self, head: &str) -> Option<&Sexp> {
        self.args().iter().find(|child| child.is(head))
    }

    pub fn child_mut(&mut self, head: &str) -> Option<&mut Sexp> {
        self.as_list_mut()?.iter_mut().skip(1).find(|child| child.is(head))
    }

    /// `(head value)` → the text of `value`.
    pub fn value(&self, head: &str) -> Option<&str> {
        self.child(head)?.arg(0)?.text()
    }

    /// Compact single-line form, as KiCad's formatter emits before prettifying.
    pub fn to_compact(&self) -> String {
        let mut out = String::new();
        self.write_compact(&mut out);
        out
    }

    fn write_compact(&self, out: &mut String) {
        match self {
            Self::Atom(text) => out.push_str(text),
            Self::Str(text) => write_quoted(out, text),
            Self::List(items) => {
                out.push('(');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    item.write_compact(out);
                }
                out.push(')');
            }
        }
    }

    /// The node in KiCad's canonical multi-line layout.
    pub fn to_kicad(&self) -> String {
        prettify(&self.to_compact())
    }
}

impl fmt::Display for Sexp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_compact())
    }
}

fn write_quoted(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Parse a single top-level expression.
pub fn parse(text: &str) -> Result<Sexp, ParseError> {
    let mut parser = Parser { bytes: text.as_bytes(), text, pos: 0 };
    let node = parser.node()?;
    parser.skip_ws();
    if parser.pos < parser.bytes.len() {
        return Err(ParseError::Trailing(parser.pos));
    }
    Ok(node)
}

struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while self.bytes.get(self.pos).is_some_and(u8::is_ascii_whitespace) {
            self.pos += 1;
        }
    }

    fn node(&mut self) -> Result<Sexp, ParseError> {
        self.skip_ws();
        match self.bytes.get(self.pos).ok_or(ParseError::Eof)? {
            b'(' => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_ws();
                    match self.bytes.get(self.pos).ok_or(ParseError::Eof)? {
                        b')' => {
                            self.pos += 1;
                            return Ok(Sexp::List(items));
                        }
                        _ => items.push(self.node()?),
                    }
                }
            }
            b')' => Err(ParseError::Unbalanced(self.pos)),
            b'"' => {
                self.pos += 1;
                let mut out = String::new();
                loop {
                    let start = self.pos;
                    while !matches!(self.bytes.get(self.pos), None | Some(b'"' | b'\\')) {
                        self.pos += 1;
                    }
                    out.push_str(&self.text[start..self.pos]);
                    match self.bytes.get(self.pos).ok_or(ParseError::Eof)? {
                        b'"' => {
                            self.pos += 1;
                            return Ok(Sexp::Str(out));
                        }
                        _ => {
                            let escaped = *self.bytes.get(self.pos + 1).ok_or(ParseError::Eof)?;
                            out.push(match escaped {
                                b'n' => '\n',
                                b'r' => '\r',
                                b't' => '\t',
                                other => other as char,
                            });
                            self.pos += 2;
                        }
                    }
                }
            }
            _ => {
                let start = self.pos;
                while self.bytes.get(self.pos).is_some_and(|b| !b.is_ascii_whitespace() && !matches!(b, b'(' | b')')) {
                    self.pos += 1;
                }
                Ok(Sexp::Atom(self.text[start..self.pos].to_owned()))
            }
        }
    }
}

/// KiCad's `KICAD_FORMAT::Prettify` (non-compact mode): one list per line,
/// tab indentation, `(xy ...)` runs packed up to column 99, and long token
/// runs wrapped after column 72.
pub fn prettify(source: &str) -> String {
    const XY_COLUMN_LIMIT: usize = 99;
    const WRAP_THRESHOLD: usize = 72;

    let bytes = source.as_bytes();
    let is_ws = |b: u8| matches!(b, b' ' | b'\t' | b'\n' | b'\r');
    let next_non_ws = |from: usize| bytes[from..].iter().copied().find(|b| !is_ws(*b)).unwrap_or(0);
    let is_xy = |at: usize| bytes.get(at..at + 4) == Some(b"(xy ");

    let mut out = String::with_capacity(source.len() + source.len() / 4);
    let mut depth = 0usize;
    let mut last = 0u8;
    let mut in_quote = false;
    let mut inserted_space = false;
    let mut multi_line = false;
    let mut in_xy = false;
    let mut column = 0usize;
    let mut backslashes = 0usize;
    let newline = |out: &mut String, depth: usize| {
        out.push('\n');
        out.extend(std::iter::repeat_n('\t', depth));
    };

    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if is_ws(c) && !in_quote {
            let next = next_non_ws(i);
            if !inserted_space && depth > 0 && last != b'(' && next != b')' && next != b'(' {
                if in_xy || column < WRAP_THRESHOLD {
                    out.push(' ');
                    column += 1;
                } else {
                    newline(&mut out, depth);
                    column = depth;
                    multi_line = true;
                }
                inserted_space = true;
            }
        } else {
            inserted_space = false;
            if c == b'(' && !in_quote {
                let current_xy = is_xy(i);
                if out.is_empty() {
                    out.push('(');
                    column += 1;
                } else if in_xy && current_xy && column < XY_COLUMN_LIMIT {
                    out.push_str(" (");
                    column += 2;
                } else {
                    newline(&mut out, depth);
                    out.push('(');
                    column = depth + 1;
                }
                in_xy = current_xy;
                depth += 1;
            } else if c == b')' && !in_quote {
                depth = depth.saturating_sub(1);
                if last == b')' || multi_line {
                    newline(&mut out, depth);
                    out.push(')');
                    column = depth + 1;
                    multi_line = false;
                } else {
                    out.push(')');
                    column += 1;
                }
            } else {
                if c == b'\\' {
                    backslashes += 1;
                } else {
                    if c == b'"' && backslashes.is_multiple_of(2) {
                        in_quote = !in_quote;
                    }
                    backslashes = 0;
                }
                // Copy the whole UTF-8 sequence so multi-byte text stays intact.
                let width = utf8_width(c);
                out.push_str(&source[i..i + width]);
                column += 1;
                i += width - 1;
            }
            last = c;
        }
        i += 1;
    }
    out.push('\n');
    out
}

fn utf8_width(first: u8) -> usize {
    match first {
        0xF0.. => 4,
        0xE0.. => 3,
        0xC0.. => 2,
        _ => 1,
    }
}

/// Format a millimetre quantity the way KiCad does: shortest exact decimal
/// of the nanometre value, no trailing zeros, no `-0`.
pub fn mm_atom(nm: i64) -> Sexp {
    Sexp::Atom(crate::units::Nm(nm).to_string())
}

/// Format a plain number token (angles, counts) without a trailing `.0`.
pub fn num(value: f64) -> Sexp {
    let mut text = String::new();
    if value.fract() == 0.0 && value.abs() < 1e15 {
        let _ = write!(text, "{}", value as i64);
    } else {
        let _ = write!(text, "{value}");
    }
    Sexp::Atom(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_kicad_layout() {
        let text = "(kicad_pcb\n\t(version 20260206)\n\t(segment\n\t\t(start 70 75.5)\n\t\t(end 70 78)\n\t\t(net \"3V3\")\n\t)\n)\n";
        let tree = parse(text).unwrap();
        assert_eq!(tree.to_kicad(), text);
        assert_eq!(tree.child("segment").unwrap().value("net"), Some("3V3"));
    }

    #[test]
    fn packs_xy_runs() {
        let pts = Sexp::node("pts", (0..3).map(|i| Sexp::node("xy", [num(i as f64), num(1.0)])));
        assert_eq!(Sexp::node("poly", [pts]).to_kicad(), "(poly\n\t(pts\n\t\t(xy 0 1) (xy 1 1) (xy 2 1)\n\t)\n)\n");
    }

    #[test]
    fn escapes_strings() {
        let node = Sexp::node("text", [Sexp::str("a \"b\"\nc")]);
        assert_eq!(parse(&node.to_compact()).unwrap(), node);
    }
}
