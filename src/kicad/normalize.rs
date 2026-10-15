//! Byte-reproducible board files: canonical item order and stable UUIDs.
//!
//! KiCad mints a random UUID per object and writes top-level items in an
//! internal container order that varies between runs, so two saves of the
//! same design hash differently. Items whose order carries no meaning are
//! sorted by their own content (UUIDs stripped), then every UUID is replaced
//! by a name-based v5 UUID numbered in file order.

use std::sync::LazyLock;

use anyhow::{Result, bail};
use regex::{Captures, Regex};
use uuid::Uuid;

/// Item kinds whose position in the file is meaningless. Everything else
/// (version, generator, general, layers, setup, ...) keeps its place.
const REORDERABLE: &[&str] = &[
    "footprint",
    "segment",
    "via",
    "arc",
    "zone",
    "gr_line",
    "gr_poly",
    "gr_circle",
    "gr_arc",
    "gr_rect",
    "gr_text",
    "gr_curve",
    "dimension",
    "image",
];

static UUID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\(uuid "[^"]*"\)"#).unwrap());

/// What [`normalize`] changed.
#[derive(Debug, PartialEq, Eq)]
pub struct Normalized {
    pub items: usize,
    pub uuids: usize,
}

/// Canonicalize a `.kicad_pcb`. `namespace` seeds the UUIDs (the project
/// name), so different boards never share identifiers.
pub fn normalize(text: &str, namespace: &str) -> Result<(String, Normalized)> {
    let Some(root) = text.find("(kicad_pcb") else { bail!("not a KiCad board file") };
    let children = top_level_children(text, root);
    if children.is_empty() {
        bail!("could not parse the board structure");
    }
    let movable: Vec<&Child> = children.iter().filter(|c| REORDERABLE.contains(&c.tag)).collect();
    let mut out = text.to_owned();
    if let (Some(first), Some(last)) = (movable.iter().map(|c| c.start).min(), movable.iter().map(|c| c.end).max()) {
        let mut interleaved: Vec<&str> = children
            .iter()
            .filter(|c| !REORDERABLE.contains(&c.tag) && (first..last).contains(&c.start))
            .map(|c| c.tag)
            .collect();
        interleaved.sort_unstable();
        interleaved.dedup();
        if !interleaved.is_empty() {
            bail!("fixed items interleaved with movable ones: {}", interleaved.join(", "));
        }
        let mut blocks: Vec<(String, &str)> =
            movable.iter().map(|c| &text[c.start..c.end]).map(|b| (UUID.replace_all(b, "").into_owned(), b)).collect();
        blocks.sort();
        let sorted: Vec<&str> = blocks.iter().map(|(_, b)| *b).collect();
        out = format!("{}{}{}", &text[..first], sorted.join("\n\t"), &text[last..]);
    }

    let namespace_uuid = Uuid::NAMESPACE_DNS;
    let mut count = 0;
    let out = UUID
        .replace_all(&out, |_: &Captures| {
            let id = Uuid::new_v5(&namespace_uuid, format!("{namespace}/{count}").as_bytes());
            count += 1;
            format!("(uuid \"{id}\")")
        })
        .into_owned();
    Ok((out, Normalized { items: movable.len(), uuids: count }))
}

struct Child<'a> {
    tag: &'a str,
    start: usize,
    end: usize,
}

/// Direct children of the list opening at `open`, as byte ranges.
fn top_level_children(text: &str, open: usize) -> Vec<Child<'_>> {
    let bytes = text.as_bytes();
    let mut i = open + 1;
    while i < bytes.len() && !matches!(bytes[i], b'(' | b' ') {
        i += 1;
    }
    let mut children = Vec::new();
    while i < bytes.len() {
        match bytes[i] {
            b')' => break,
            b'(' => {}
            _ => {
                i += 1;
                continue;
            }
        }
        let start = i;
        let mut depth = 0;
        let mut j = i;
        while j < bytes.len() {
            match bytes[j] {
                b'"' => {
                    j += 1;
                    while j < bytes.len() && !(bytes[j] == b'"' && bytes[j - 1] != b'\\') {
                        j += 1;
                    }
                }
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        let end = (j + 1).min(bytes.len());
        let tag_end =
            text[start + 1..end].find(|c: char| !(c.is_alphanumeric() || c == '_')).map_or(end, |n| start + 1 + n);
        children.push(Child { tag: &text[start + 1..tag_end], start, end });
        i = end;
    }
    children
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_items_and_renumbers_uuids() {
        let a =
            "(kicad_pcb\n\t(version 1)\n\t(via (uuid \"x\") b)\n\t(segment (uuid \"y\") a)\n\t(embedded_fonts no)\n)\n";
        let b =
            "(kicad_pcb\n\t(version 1)\n\t(segment (uuid \"q\") a)\n\t(via (uuid \"r\") b)\n\t(embedded_fonts no)\n)\n";
        let (na, stats) = normalize(a, "board").unwrap();
        assert_eq!(na, normalize(b, "board").unwrap().0);
        assert_eq!(stats, Normalized { items: 2, uuids: 2 });
        assert!(na.contains("(segment (uuid"));
    }
}
