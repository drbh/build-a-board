//! Pad-level connectivity of a `.kicad_pcb`.

use std::path::Path;

use anyhow::Result;

use crate::sexpr::Sexp;

/// One connected pad.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Connection {
    pub net: String,
    pub reference: String,
    pub pad: String,
    pub value: String,
}

/// Every pad on a net, sorted by net, reference, pad, then value.
pub fn connections(board: &Sexp) -> Vec<Connection> {
    let mut rows: Vec<Connection> = board
        .children("footprint")
        .flat_map(|footprint| {
            let property = |name: &str| {
                footprint
                    .children("property")
                    .find(|p| p.arg(0).and_then(Sexp::text) == Some(name))
                    .and_then(|p| p.arg(1)?.text())
                    .unwrap_or_default()
                    .to_owned()
            };
            let (reference, value) = (property("Reference"), property("Value"));
            footprint.children("pad").filter_map(move |pad| {
                let net = pad.value("net").filter(|net| !net.is_empty())?;
                Some(Connection {
                    net: net.to_owned(),
                    reference: reference.clone(),
                    pad: pad.arg(0)?.text()?.to_owned(),
                    value: value.clone(),
                })
            })
        })
        .collect();
    rows.sort();
    rows
}

/// Write `net,reference,pad,value` CSV, the input to the schematic and the
/// PCB/schematic connectivity gate.
pub fn write_csv(board: &Sexp, path: &Path) -> Result<()> {
    let mut writer = csv::WriterBuilder::new().terminator(csv::Terminator::CRLF).from_path(path)?;
    writer.write_record(["net", "reference", "pad", "value"])?;
    for row in connections(board) {
        writer.write_record([&row.net, &row.reference, &row.pad, &row.value])?;
    }
    writer.flush()?;
    Ok(())
}
