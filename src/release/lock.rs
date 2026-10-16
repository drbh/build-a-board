//! The reviewed-revision lock (`board-revision.json`) and connectivity gate.

use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::bom::Table;
use crate::process::Cmd;

fn hex(digest: impl AsRef<[u8]>) -> String {
    digest.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("reading {}", path.display()))?;
    let mut digest = Sha256::new();
    let mut chunk = vec![0; 1 << 20];
    loop {
        match file.read(&mut chunk)? {
            0 => return Ok(hex(digest.finalize())),
            n => digest.update(&chunk[..n]),
        }
    }
}

/// The board sources pinned by the lock, as repository-relative paths.
pub struct Sources<'a> {
    pub repo: &'a Path,
    pub files: Vec<&'a Path>,
}

impl Sources<'_> {
    fn hashes(&self) -> Result<Vec<(String, String)>> {
        self.files
            .iter()
            .map(|file| {
                let relative = file.strip_prefix(self.repo).unwrap_or(file).to_string_lossy().into_owned();
                Ok((relative, sha256(file)?))
            })
            .collect()
    }
}

/// Refuse to release a board that differs from the reviewed, committed one.
pub fn verify(lock: &Path, sources: &Sources<'_>) -> Result<()> {
    if !lock.is_file() {
        bail!("missing {}; run `bb lock-board` after reviewing and committing the board", lock.display());
    }
    let data: Value = serde_json::from_str(&std::fs::read_to_string(lock)?)?;
    let empty = Map::new();
    let expected = data.get("source_sha256").and_then(Value::as_object).unwrap_or(&empty);
    let commit = data.get("reviewed_commit").and_then(Value::as_str).unwrap_or("");
    let actual = sources.hashes()?;
    let mut errors: Vec<String> = actual
        .iter()
        .filter(|(path, digest)| expected.get(path).and_then(Value::as_str) != Some(digest))
        .map(|(path, digest)| {
            let locked = expected.get(path).and_then(Value::as_str).unwrap_or("(none)");
            format!("{path}: locked {locked}, current {digest}")
        })
        .collect();
    for (path, digest) in expected {
        let committed = Cmd::new("git")
            .args(["show", &format!("{commit}:{path}")])
            .cwd(sources.repo)
            .output()?
            .map(|out| hex(Sha256::digest(out)));
        if committed.as_deref() != digest.as_str() {
            errors.push(format!("{path}: locked hash is not the one in reviewed commit {commit}"));
        }
    }
    if !errors.is_empty() {
        bail!(
            "the reviewed board revision changed; commit and review it, then run `bb lock-board`:\n{}",
            errors.join("\n")
        );
    }
    let short: String = commit.chars().take(12).collect();
    println!("Board revision locked to {short} ({} source files)", actual.len());
    Ok(())
}

/// Pin the committed board sources as the reviewed revision.
pub fn lock(lock: &Path, sources: &Sources<'_>, project: &crate::board::Project) -> Result<()> {
    let clean =
        Cmd::new("git").args(["diff", "--quiet", "HEAD", "--"]).args(&sources.files).cwd(sources.repo).succeeds()?;
    if !clean {
        bail!("commit the reviewed PCB and schematic before updating the board lock");
    }
    let head = Cmd::new("git")
        .args(["rev-parse", "HEAD"])
        .cwd(sources.repo)
        .output()?
        .ok_or_else(|| anyhow!("git rev-parse HEAD failed"))?;
    let commit = String::from_utf8_lossy(&head).trim().to_owned();
    let hashes: Map<String, Value> = sources.hashes()?.into_iter().map(|(p, d)| (p, Value::String(d))).collect();
    let data = json!({
        "version": "1.0.0",
        "reviewed_commit": commit,
        "reviewed_on": chrono::Local::now().date_naive().to_string(),
        "source_sha256": hashes,
        "manufacturing": {"copper_weight_oz": project.copper_weight_oz},
        "decisions": project.decisions.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<Map<_, _>>(),
    });
    std::fs::write(lock, crate::json::to_pretty(&data) + "\n")?;
    println!("Board revision lock updated to {commit}; commit {}", lock.display());
    Ok(())
}

type Pin = (String, String);

/// Direct children with the given tag (ElementTree's `findall`).
fn children<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    tag: &'static str,
) -> impl Iterator<Item = roxmltree::Node<'a, 'input>> {
    node.children().filter(move |child| child.has_tag_name(tag))
}

/// Every connected pad must be on the same net in the PCB and the schematic.
pub fn connectivity(pcb_csv: &Path, schematic_xml: &Path) -> Result<()> {
    let mut pcb: HashMap<Pin, String> = HashMap::new();
    for row in Table::read(pcb_csv)?.rows {
        let pin = (row.field("reference")?.to_owned(), row.field("pad")?.to_owned());
        let net = row.field("net")?;
        if pcb.get(&pin).is_some_and(|known| known != net) {
            bail!("PCB has conflicting nets for {}.{}", pin.0, pin.1);
        }
        pcb.insert(pin, net.to_owned());
    }

    let xml = std::fs::read_to_string(schematic_xml).with_context(|| format!("reading {}", schematic_xml.display()))?;
    let doc = roxmltree::Document::parse(&xml).with_context(|| format!("parsing {}", schematic_xml.display()))?;
    let attr = |node: roxmltree::Node, name: &str| {
        node.attribute(name).map(str::to_owned).ok_or_else(|| anyhow!("netlist node without {name}"))
    };
    let mut schematic: HashMap<Pin, String> = HashMap::new();
    for nets in children(doc.root_element(), "nets") {
        for net in children(nets, "net") {
            let name = attr(net, "name")?;
            for node in children(net, "node") {
                let pin = (attr(node, "ref")?, attr(node, "pin")?);
                if schematic.get(&pin).is_some_and(|known| *known != name) {
                    bail!("schematic has conflicting nets for {}.{}", pin.0, pin.1);
                }
                schematic.insert(pin, name.clone());
            }
        }
    }

    let pins: BTreeSet<&Pin> = pcb.keys().chain(schematic.keys()).collect();
    let none = || "(unconnected)".to_owned();
    let errors: Vec<String> = pins
        .into_iter()
        .filter(|pin| pcb.get(*pin) != schematic.get(*pin))
        .map(|pin @ (reference, pad)| {
            let (p, s) = (pcb.get(pin).cloned().unwrap_or_else(none), schematic.get(pin).cloned().unwrap_or_else(none));
            format!("{reference}.{pad}: PCB {p}, schematic {s}")
        })
        .collect();
    if !errors.is_empty() {
        bail!("PCB/schematic connectivity mismatch:\n{}", errors.join("\n"));
    }
    println!("PCB/schematic connectivity agrees for {} connected pins", pcb.len());
    Ok(())
}
