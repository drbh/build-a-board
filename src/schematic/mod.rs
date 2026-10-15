//! Connectivity schematic generation (`.kicad_sch`).
//!
//! The PCB is the source of truth for connectivity; the schematic is derived
//! from it so that KiCad's ERC and PCB/schematic cross-checks have something
//! authoritative to compare against. Every footprint becomes one generated
//! rectangular symbol with one passive pin per connected pad, and every pin
//! gets a global label carrying its net name, so KiCad joins the nets by name
//! without any drawn wires.
//!
//! Symbols are stacked in columns on a 50 mil grid, ordered by designator
//! (`C1, C2, ..., C10, ..., U1`). The output uses the same s-expression layout
//! that kiutils writes, so the file is byte-identical to the original Python
//! generator's apart from its random UUIDs.

use std::collections::HashMap;
use std::fmt::{self, Write as _};
use std::path::Path;

use anyhow::{Context, Result, bail};
use uuid::Uuid;

/// KiCad's 50 mil connection grid, in millimetres.
const GRID: f64 = 1.27;
/// Vertical pin pitch inside a symbol.
const PIN_PITCH: f64 = 2.54;
/// Pin length; pins extend this far left of the symbol body.
const PIN_LENGTH: f64 = 2.54;
/// Horizontal offset of the pin connection point from the symbol origin.
const PIN_X: f64 = -12.7;
/// Half width of the symbol body.
const BODY_HALF_WIDTH: f64 = 10.16;
/// Minimum body height, and the body height added on top of the pin span.
const MIN_BODY_HEIGHT: f64 = 10.16;
const BODY_PADDING: f64 = 5.08;
/// Distance from the body edge to the Reference/Value fields.
const FIELD_OFFSET: f64 = 2.0;
/// Vertical gap between stacked symbols.
const SYMBOL_GAP: f64 = 7.62;
const BODY_STROKE: f64 = 0.254;
const TEXT_SIZE: f64 = 1.0;

/// Everything about the generated schematic that is specific to a project.
#[derive(Clone, Debug)]
pub struct SchematicOptions {
    /// KiCad project name, used for symbol instance paths.
    pub project: String,
    /// `(generator ...)` token.
    pub generator: String,
    /// `(version ...)` file-format token.
    pub version: String,
    pub paper: String,
    pub title: String,
    pub revision: String,
    pub company: Option<String>,
    /// Numbered title-block comments.
    pub comments: Vec<(u32, String)>,
    /// Library nickname of the generated symbols (`<library>:Generated_<ref>`).
    pub library: String,
    /// `Description` field of every generated symbol.
    pub description: String,
    pub layout: Layout,
}

/// Where symbol columns sit on the sheet, in millimetres.
#[derive(Clone, Debug)]
pub struct Layout {
    /// X of each column's symbol origins (snapped to the grid).
    pub columns: Vec<f64>,
    /// Y of the top edge of the first symbol in each column.
    pub top: f64,
    /// A symbol whose body would extend below this Y starts a new column.
    pub bottom: f64,
}

impl Layout {
    /// Six columns on an A3 sheet.
    pub fn a3() -> Self {
        Self { columns: vec![40.64, 104.14, 167.64, 231.14, 294.64, 358.14], top: 20.32, bottom: 270.0 }
    }
}

impl SchematicOptions {
    /// A3 defaults for `project`; override the title block and library as needed.
    pub fn new(project: &str) -> Self {
        Self {
            project: project.to_owned(),
            generator: project.to_owned(),
            version: "20260306".to_owned(),
            paper: "A3".to_owned(),
            title: project.to_owned(),
            revision: "1.0".to_owned(),
            company: None,
            comments: vec![(1, "Generated from routed PCB connectivity".to_owned())],
            library: "Generated".to_owned(),
            description: "Generated from routed PCB".to_owned(),
            layout: Layout::a3(),
        }
    }
}

/// One connected pad from the PCB netlist CSV (`net,reference,pad,value`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Connection {
    pub net: String,
    pub reference: String,
    pub pad: String,
    pub value: String,
}

/// Read the PCB netlist CSV and generate the schematic with random UUIDs.
pub fn generate(netlist_csv: &Path, options: &SchematicOptions) -> Result<String> {
    let connections = read_netlist(netlist_csv)?;
    render(&connections, options, &mut Uuid::new_v4)
}

pub fn read_netlist(path: &Path) -> Result<Vec<Connection>> {
    let mut reader = csv::Reader::from_path(path).with_context(|| format!("reading {}", path.display()))?;
    reader.deserialize().collect::<Result<_, _>>().with_context(|| format!("parsing {}", path.display()))
}

/// Generate the schematic, drawing UUIDs from `uuids` (inject a deterministic
/// source for reproducible output).
pub fn render(
    connections: &[Connection],
    options: &SchematicOptions,
    uuids: &mut dyn FnMut() -> Uuid,
) -> Result<String> {
    let parts = group_by_part(connections)?;
    let mut uid = || uuids().to_string();
    let root = uid();
    let layout = &options.layout;
    let columns: Vec<f64> = layout.columns.iter().map(|&x| snap(x)).collect();

    let mut lib_symbols = String::new();
    let mut labels = String::new();
    let mut symbols = String::new();
    let mut column = 0;
    let mut y = snap(layout.top);
    for part in &parts {
        let n = part.pads.len();
        let height = MIN_BODY_HEIGHT.max((n as f64 - 1.0) * PIN_PITCH + BODY_PADDING);
        if y + height > layout.bottom {
            column += 1;
            y = snap(layout.top);
        }
        let Some(&x) = columns.get(column) else {
            bail!("schematic does not fit the {} sheet with {} columns", options.paper, columns.len());
        };
        let cy = snap(y + height / 2.0);
        // Pin Y in symbol-local coordinates (+Y up), top pin first.
        let pin_y = |i: usize| snap((n as f64 - 1.0) * GRID - i as f64 * PIN_PITCH);
        let name = format!("Generated_{}", part.reference);
        let lib_id = format!("{}:{name}", options.library);
        let value = &part.pads[0].value;

        lib_symbols.push('\n');
        write_lib_symbol(&mut lib_symbols, &lib_id, &name, part, value, height, options, pin_y);
        write_instance(&mut symbols, &lib_id, part, value, (x, cy), height, options, &root, &mut uid);
        for (i, pad) in part.pads.iter().enumerate() {
            // KiCad symbol-local +Y points opposite schematic screen +Y.
            let at = (Num::Float(snap(x + PIN_X)), Num::Float(snap(cy - pin_y(i))));
            let _ =
                writeln!(labels, "  (global_label {} (shape bidirectional) (at {} {} 0)", quote(&pad.net), at.0, at.1);
            let _ = writeln!(labels, "    {}", effects(false));
            let _ = writeln!(labels, "    (uuid {})", uid());
            labels.push_str("  )\n");
        }
        y = snap(y + height + SYMBOL_GAP);
    }

    let mut out = String::new();
    let _ = writeln!(out, "(kicad_sch (version {}) (generator {})", options.version, options.generator);
    let _ = write!(out, "\n  (uuid {root})\n\n");
    let _ = writeln!(out, "  (paper {})", quote(&options.paper));
    out.push_str("\n  (title_block\n");
    let _ = writeln!(out, "    (title {})", quote(&options.title));
    let _ = writeln!(out, "    (rev {})", quote(&options.revision));
    if let Some(company) = &options.company {
        let _ = writeln!(out, "    (company {})", quote(company));
    }
    for (number, comment) in &options.comments {
        let _ = writeln!(out, "    (comment {number} {})", quote(comment));
    }
    out.push_str("  )\n");
    if parts.is_empty() {
        out.push_str("  (lib_symbols)\n");
    } else {
        let _ = write!(out, "\n  (lib_symbols{lib_symbols}  )\n");
        let _ = write!(out, "\n{labels}{symbols}");
    }
    out.push_str("\n  (sheet_instances\n    (path \"/\" (page \"1\"))\n  )\n)\n");
    Ok(out)
}

/// One footprint and its connected pads, in schematic pin order.
struct Part<'a> {
    reference: &'a str,
    pads: Vec<&'a Connection>,
}

/// Group connections by footprint, rejecting a pad listed on two nets, and
/// order parts by designator and pads by number.
fn group_by_part(connections: &[Connection]) -> Result<Vec<Part<'_>>> {
    let mut parts: Vec<Part> = Vec::new();
    let mut index: HashMap<&str, usize> = HashMap::new();
    for row in connections {
        let slot = *index.entry(&row.reference).or_insert_with(|| {
            parts.push(Part { reference: &row.reference, pads: Vec::new() });
            parts.len() - 1
        });
        let pads = &mut parts[slot].pads;
        match pads.iter_mut().find(|pad| pad.pad == row.pad) {
            Some(existing) if existing.net != row.net => {
                bail!("conflicting PCB nets for {}.{}: {} / {}", row.reference, row.pad, existing.net, row.net)
            }
            Some(existing) => *existing = row,
            None => pads.push(row),
        }
    }
    parts.sort_by_cached_key(|part| designator_key(part.reference));
    for part in &mut parts {
        part.pads.sort_by_cached_key(|pad| (pad_number(&pad.pad), pad.pad.clone()));
    }
    Ok(parts)
}

/// `C10` sorts after `C9`: by prefix, then by the numeric part.
fn designator_key(reference: &str) -> (String, u128) {
    let prefix = reference.trim_end_matches(|c: char| c.is_ascii_digit());
    let digits: String = reference.chars().filter(char::is_ascii_digit).collect();
    (prefix.to_owned(), digits.parse().unwrap_or(0))
}

/// Numeric pads first in numeric order; named pads after them.
fn pad_number(pad: &str) -> u128 {
    if !pad.is_empty() && pad.chars().all(|c| c.is_ascii_digit()) { pad.parse().unwrap_or(u128::MAX) } else { 999 }
}

#[allow(clippy::too_many_arguments)]
fn write_lib_symbol(
    out: &mut String,
    lib_id: &str,
    name: &str,
    part: &Part,
    value: &str,
    height: f64,
    options: &SchematicOptions,
    pin_y: impl Fn(usize) -> f64,
) {
    let half = height / 2.0;
    let _ = writeln!(out, "    (symbol {} (in_bom yes) (on_board yes)", quote(lib_id));
    let field_y = half + FIELD_OFFSET;
    write_fields(
        out,
        6,
        part.reference,
        value,
        (Num::Int(0), Num::Float(field_y)),
        (Num::Int(0), Num::Float(-field_y)),
        options,
    );
    let _ = writeln!(out, "      (symbol {}", quote(&format!("{name}_1_1")));
    let (w, h) = (Num::Float(BODY_HALF_WIDTH), Num::Float(half));
    let _ = writeln!(
        out,
        "        (rectangle (start {} {h}) (end {w} {})",
        Num::Float(-BODY_HALF_WIDTH),
        Num::Float(-half)
    );
    let _ = writeln!(out, "          (stroke (width {}) (type default))", Num::Float(BODY_STROKE));
    out.push_str("          (fill (type background))\n        )\n");
    for (i, pad) in part.pads.iter().enumerate() {
        let _ = writeln!(
            out,
            "        (pin passive line (at {} {} 0) (length {})",
            Num::Float(PIN_X),
            Num::Float(pin_y(i)),
            Num::Float(PIN_LENGTH)
        );
        let _ = writeln!(out, "          (name {} {})", quote(&pad.net), effects(false));
        let _ = writeln!(out, "          (number {} {})", quote(&pad.pad), effects(false));
        out.push_str("        )\n");
    }
    out.push_str("      )\n    )\n");
}

#[allow(clippy::too_many_arguments)]
fn write_instance(
    out: &mut String,
    lib_id: &str,
    part: &Part,
    value: &str,
    (x, cy): (f64, f64),
    height: f64,
    options: &SchematicOptions,
    root: &str,
    uid: &mut impl FnMut() -> String,
) {
    let (x, half) = (Num::Float(x), height / 2.0);
    out.push('\n');
    let _ = writeln!(out, "  (symbol (lib_id {}) (at {x} {} 0) (unit 1)", quote(lib_id), Num::Float(cy));
    out.push_str("    (in_bom yes) (on_board yes)\n");
    let _ = writeln!(out, "    (uuid {})", uid());
    let reference_at = (x, Num::Float(cy - half - FIELD_OFFSET));
    let value_at = (x, Num::Float(cy + half + FIELD_OFFSET));
    write_fields(out, 4, part.reference, value, reference_at, value_at, options);
    for pad in &part.pads {
        let _ = writeln!(out, "    (pin {} (uuid {}))", quote(&pad.pad), uid());
    }
    out.push_str("    (instances\n");
    let _ = writeln!(out, "      (project {}", quote(&options.project));
    let _ = writeln!(out, "        (path {}", quote(&format!("/{root}")));
    let _ = writeln!(out, "          (reference {}) (unit 1)", quote(part.reference));
    out.push_str("        )\n      )\n    )\n  )\n");
}

/// The five standard fields; only Reference and Value are visible.
fn write_fields(
    out: &mut String,
    indent: usize,
    reference: &str,
    value: &str,
    reference_at: (Num, Num),
    value_at: (Num, Num),
    options: &SchematicOptions,
) {
    let origin = (Num::Int(0), Num::Int(0));
    let fields = [
        ("Reference", reference, reference_at, false),
        ("Value", value, value_at, false),
        ("Footprint", "", origin, true),
        ("Datasheet", "", origin, true),
        ("Description", options.description.as_str(), origin, true),
    ];
    let pad = " ".repeat(indent);
    for (key, text, (x, y), hidden) in fields {
        let _ = writeln!(out, "{pad}(property {} {} (at {x} {y} 0)", quote(key), quote(text));
        let _ = writeln!(out, "{pad}  {}", effects(hidden));
        let _ = writeln!(out, "{pad})");
    }
}

fn effects(hidden: bool) -> String {
    let size = Num::Float(TEXT_SIZE);
    format!("(effects (font (size {size} {size})){})", if hidden { " hide" } else { "" })
}

/// A quoted string; only `"` is escaped, as kiutils does.
fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\\\""))
}

/// Snap to the connection grid, rounding halves to even like Python's `round`.
fn snap(value: f64) -> f64 {
    (value / GRID).round_ties_even() as i64 as f64 * GRID
}

/// A number printed the way the original generator's values print: integers
/// bare, floats as their shortest round-trip repr (`7.08`, `1.0`, `1e-05`).
#[derive(Clone, Copy, Debug)]
enum Num {
    Int(i64),
    Float(f64),
}

impl fmt::Display for Num {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Int(value) => write!(f, "{value}"),
            Self::Float(value) => write_float(f, value),
        }
    }
}

fn write_float(f: &mut fmt::Formatter<'_>, value: f64) -> fmt::Result {
    if !value.is_finite() {
        return f.write_str(if value.is_nan() {
            "nan"
        } else if value > 0.0 {
            "inf"
        } else {
            "-inf"
        });
    }
    // `{:e}` gives the shortest round-trip digits, e.g. "-4.191e1".
    let sci = format!("{value:e}");
    let (mantissa, exponent) = sci.split_once('e').expect("exponent");
    let exponent: i32 = exponent.parse().expect("exponent");
    let (sign, mantissa) = mantissa.strip_prefix('-').map_or(("", mantissa), |m| ("-", m));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    f.write_str(sign)?;
    if (-4..16).contains(&exponent) {
        let point = exponent + 1;
        if point <= 0 {
            write!(f, "0.{}{digits}", "0".repeat(point.unsigned_abs() as usize))
        } else if point as usize >= digits.len() {
            write!(f, "{digits}{}.0", "0".repeat(point as usize - digits.len()))
        } else {
            let (whole, frac) = digits.split_at(point as usize);
            write!(f, "{whole}.{frac}")
        }
    } else {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() { String::new() } else { format!(".{rest}") };
        write!(f, "{first}{rest}e{}{:02}", if exponent < 0 { '-' } else { '+' }, exponent.unsigned_abs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Replace each distinct UUID by its order of first appearance.
    fn canonical_uuids(text: &str) -> String {
        let re = regex::Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").unwrap();
        let mut seen: HashMap<String, usize> = HashMap::new();
        re.replace_all(text, |caps: &regex::Captures| {
            let next = seen.len();
            format!("UUID-{}", seen.entry(caps[0].to_owned()).or_insert(next))
        })
        .into_owned()
    }

    fn balance_bot() -> SchematicOptions {
        SchematicOptions {
            title: "Balance Bot Devboard".to_owned(),
            company: Some("Balance Bot devboard".to_owned()),
            library: "BalanceBot_Devboard".to_owned(),
            ..SchematicOptions::new("balance-bot-devboard")
        }
    }

    /// The fixture's expected output was written by the original
    /// kiutils-based `tools/schematic.py`.
    #[test]
    fn matches_original_generator() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/schematic/fixtures");
        let mut counter = 0u128;
        let mut uuids = || {
            counter += 1;
            Uuid::from_u128(counter)
        };
        let connections = read_netlist(&dir.join("netlist.csv")).unwrap();
        let text = render(&connections, &balance_bot(), &mut uuids).unwrap();
        let expected = std::fs::read_to_string(dir.join("expected.kicad_sch")).unwrap();
        assert_eq!(canonical_uuids(&text), canonical_uuids(&expected));
    }

    #[test]
    fn rejects_conflicting_pads() {
        let row =
            |net: &str| Connection { net: net.into(), reference: "R1".into(), pad: "1".into(), value: "1k".into() };
        let err = render(&[row("A"), row("B")], &balance_bot(), &mut Uuid::new_v4).unwrap_err();
        assert!(err.to_string().contains("R1.1"));
    }

    #[test]
    fn orders_designators_numerically() {
        let mut refs = ["R10", "C2", "R9", "C10"];
        refs.sort_by_key(|r| designator_key(r));
        assert_eq!(refs, ["C2", "C10", "R9", "R10"]);
    }

    #[test]
    fn floats_print_like_python() {
        let show = |v: f64| Num::Float(v).to_string();
        assert_eq!(show(1.0), "1.0");
        assert_eq!(show(41.910000000000004), "41.910000000000004");
        assert_eq!(show(-7.08), "-7.08");
        assert_eq!(show(0.00001), "1e-05");
        assert_eq!(show(snap(-0.3)), "0.0");
    }
}
