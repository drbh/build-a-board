//! Approved-parts BOM validation and the BOM/CPL files built from it.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use serde_json::Value;

use crate::board::Project;

/// A CSV file read by header name.
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Row>,
}

pub struct Row(HashMap<String, String>);

impl Row {
    /// The cell, or `""` when the column is absent.
    pub fn get(&self, column: &str) -> &str {
        self.0.get(column).map_or("", String::as_str)
    }

    pub fn field(&self, column: &str) -> Result<&str> {
        self.0.get(column).map(String::as_str).ok_or_else(|| anyhow!("missing column {column:?}"))
    }

    fn number(&self, column: &str) -> Result<f64> {
        let text = self.field(column)?;
        text.trim().parse().with_context(|| format!("{column}: {text:?} is not a number"))
    }

    pub fn refs(&self) -> Result<impl Iterator<Item = &str>> {
        Ok(self.field("refs")?.split_whitespace())
    }
}

impl Table {
    pub fn read(path: &Path) -> Result<Self> {
        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .from_path(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let raw: Vec<String> = reader.headers()?.iter().map(str::to_owned).collect();
        let mut headers = Vec::with_capacity(raw.len());
        for header in &raw {
            if !headers.contains(header) {
                headers.push(header.clone());
            }
        }
        let rows = reader
            .records()
            .map(|record| Ok(Row(raw.iter().cloned().zip(record?.iter().map(str::to_owned)).collect())))
            .collect::<Result<_>>()
            .with_context(|| format!("reading {}", path.display()))?;
        Ok(Self { headers, rows })
    }
}

/// RFC 4180 CSV with CRLF line endings (what Python's `csv` module writes).
pub fn write_csv<R, S>(path: &Path, headers: &[&str], rows: impl IntoIterator<Item = R>) -> Result<()>
where
    R: IntoIterator<Item = S>,
    S: AsRef<[u8]>,
{
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::CRLF)
        .from_path(path)
        .with_context(|| format!("writing {}", path.display()))?;
    writer.write_record(headers)?;
    for row in rows {
        writer.write_record(row)?;
    }
    writer.flush()?;
    Ok(())
}

static TURNKEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(?:JLCPCB|LCSC)\s+(?:C)?\d+").unwrap());
static AMAZON: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)Amazon\s+ASIN\s+B0[A-Z0-9]{8}").unwrap());
static PCB_REFS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\(property "Reference" "([A-Z]+[0-9]+)"|\(fp_text reference "([A-Z]+[0-9]+)""#).unwrap()
});

/// Validate the approved parts list against itself and the board: every
/// line sourced and active, every placed reference covered.
pub fn validate(project: &Project, bom: &Path, board: &Path) -> Result<Table> {
    let table = Table::read(bom)?;
    let name = bom.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let mut errors = Vec::new();
    for (line, row) in (2..).zip(&table.rows) {
        for field in ["manufacturer", "mpn", "lifecycle", "source_checked"] {
            if row.get(field).trim().is_empty() {
                errors.push(format!("{name}:{line}: missing {field}"));
            }
        }
        let distributor = !row.get("mouser_pn").trim().is_empty() || !row.get("digikey_pn").trim().is_empty();
        let notes = row.get("notes");
        if !(distributor || TURNKEY.is_match(notes) || AMAZON.is_match(notes)) {
            errors.push(format!("{name}:{line}: missing distributor, Amazon ASIN, or JLCPCB/LCSC sourcing"));
        }
        if row.get("lifecycle") != "Active" {
            errors.push(format!("{name}:{line}: part is not Active"));
        }
    }
    let board = std::fs::read_to_string(board).with_context(|| format!("reading {}", board.display()))?;
    let placed: BTreeSet<&str> = PCB_REFS
        .captures_iter(&board)
        .filter_map(|caps| caps.get(1).or_else(|| caps.get(2)))
        .map(|m| m.as_str())
        .filter(|reference| !project.unsourced.iter().any(|prefix| reference.starts_with(prefix.as_str())))
        .collect();
    let mut listed = HashSet::new();
    for row in &table.rows {
        listed.extend(row.refs()?);
    }
    let missing: Vec<&str> = placed.into_iter().filter(|r| !listed.contains(r)).collect();
    if !missing.is_empty() {
        errors.push(format!("PCB refs missing from approved BOM: {}", missing.join(", ")));
    }
    if !errors.is_empty() {
        bail!("approved BOM is invalid:\n{}", errors.join("\n"));
    }
    println!("Approved BOM validated: {} sourced lines", table.rows.len());
    Ok(table)
}

/// Paths of the BOM-derived outputs.
pub struct BomOutputs<'a> {
    pub bom: &'a Path,
    pub jlcpcb: &'a Path,
    pub shopping: &'a Path,
}

/// The full BOM, the JLCPCB assembly BOM, and the DigiKey shopping list.
pub fn export(project: &Project, table: &Table, out: BomOutputs<'_>) -> Result<()> {
    let headers: Vec<&str> = table.headers.iter().map(String::as_str).collect();
    write_csv(out.bom, &headers, table.rows.iter().map(|row| headers.iter().map(|h| row.get(h)).collect::<Vec<_>>()))?;

    let shopping = table
        .rows
        .iter()
        .map(|r| {
            let per_board: i64 = r.field("qty_per_board")?.trim().parse().context("qty_per_board is not an integer")?;
            Ok([
                r.field("digikey_pn")?.to_owned(),
                r.field("mpn")?.to_owned(),
                r.field("manufacturer")?.to_owned(),
                (per_board * i64::from(project.order_quantity)).to_string(),
                r.field("refs")?.to_owned(),
                r.field("description")?.to_owned(),
            ])
        })
        .collect::<Result<Vec<_>>>()?;
    let shopping_headers = [
        "DigiKey Part Number",
        "Manufacturer Part Number",
        "Manufacturer",
        "Quantity",
        "Customer Reference",
        "Description",
    ];
    write_csv(out.shopping, &shopping_headers, shopping)?;

    let mut jlc = Vec::new();
    for row in &table.rows {
        if row.field("assembly")? != "SMT" {
            continue;
        }
        let mpn = row.field("mpn")?;
        let lcsc = project
            .lcsc
            .iter()
            .find(|(part, _)| part == mpn)
            .map(|(_, lcsc)| lcsc)
            .ok_or_else(|| anyhow!("no LCSC number for SMT part {mpn}"))?;
        let footprint = row.field("footprint")?;
        jlc.push([
            row.field("description")?.to_owned(),
            row.refs()?.collect::<Vec<_>>().join(","),
            footprint.rsplit(':').next().unwrap_or(footprint).to_owned(),
            lcsc.clone(),
        ]);
    }
    write_csv(out.jlcpcb, &["Comment", "Designator", "Footprint", "LCSC Part #"], jlc)?;
    println!("BOM, JLCPCB BOM, and {}-board DigiKey list exported", project.order_quantity);
    Ok(())
}

/// Floored modulo, so a negative rotation lands in [0, 360).
fn modulo_360(degrees: f64) -> f64 {
    let m = degrees % 360.0;
    if m == 0.0 {
        0.0
    } else if m < 0.0 {
        m + 360.0
    } else {
        m
    }
}

/// JLCPCB's CPL from KiCad's position file: centroids in the same negative-Y
/// frame as the Gerbers, with per-reference rotation corrections.
pub fn export_cpl(table: &Table, positions: &Path, contract: Option<&Path>, cpl: &Path) -> Result<()> {
    let contract: Value = match contract {
        Some(path) => serde_json::from_str(
            &std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?,
        )?,
        None => Value::Null,
    };
    let offsets = contract.pointer("/manufacturing/cpl_rotation_offsets_deg");
    let offset = |reference: &str| offsets.and_then(|o| o.get(reference)).and_then(Value::as_f64).unwrap_or(0.0);

    let mut smt = HashSet::new();
    for part in &table.rows {
        if part.field("assembly")? == "SMT" {
            smt.extend(part.refs()?);
        }
    }
    let positions = Table::read(positions)?;
    let mut rows = Vec::new();
    for r in &positions.rows {
        let reference = r.field("Ref")?;
        if !smt.contains(reference) {
            continue;
        }
        rows.push([
            reference.to_owned(),
            format!("{:.4}", r.number("PosX")?),
            // KiCad's Gerbers use this same negative-Y frame; negating it here
            // would mirror placement in JLC's viewer.
            format!("{:.4}", r.number("PosY")?),
            format!("{:.2}", modulo_360(r.number("Rot")? + offset(reference))),
            if r.field("Side")?.eq_ignore_ascii_case("top") { "Top" } else { "Bottom" }.to_owned(),
        ]);
    }
    write_csv(cpl, &["Designator", "Mid X", "Mid Y", "Rotation", "Layer"], &rows)?;

    let placed: HashSet<&str> =
        positions.rows.iter().filter_map(|r| r.field("Ref").ok()).filter(|r| smt.contains(r)).collect();
    if placed != rows.iter().map(|[reference, ..]| reference.as_str()).collect() {
        bail!("CPL SMT reference coverage differs from KiCad's position output");
    }
    println!("JLCPCB CPL validated: {} centroids in Gerber coordinates", rows.len());
    Ok(())
}
