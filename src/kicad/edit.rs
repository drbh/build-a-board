//! In-place edits of a parsed `.kicad_pcb` for the routing pipeline.

use std::collections::HashMap;

use anyhow::{Result, anyhow};

use crate::design::{Layer, Track};
use crate::sexpr::{self, Sexp};
use crate::units::{Nm, Pt};

fn point(node: &Sexp, head: &str) -> Option<Pt> {
    let at = node.child(head)?;
    Some(Pt::new(Nm::from_mm(at.arg(0)?.as_f64()?), Nm::from_mm(at.arg(1)?.as_f64()?)))
}

/// A board segment read back as a [`Track`].
pub fn segment(node: &Sexp) -> Option<Track> {
    if !node.is("segment") {
        return None;
    }
    Some(Track {
        net: node.value("net").unwrap_or_default().to_owned(),
        layer: Layer::parse(node.value("layer")?)?,
        width: Nm::from_mm(node.child("width")?.arg(0)?.as_f64()?),
        start: point(node, "start")?,
        end: point(node, "end")?,
    })
}

fn items(board: &mut Sexp) -> &mut Vec<Sexp> {
    board.as_list_mut().expect("board is a list")
}

fn set_width(node: &mut Sexp, width: Nm) {
    if let Some(list) = node.child_mut("width").and_then(Sexp::as_list_mut) {
        list.truncate(1);
        list.push(sexpr::mm_atom(width.0));
    }
}

fn ends(track: &Track) -> (Pt, Pt) {
    if track.start <= track.end { (track.start, track.end) } else { (track.end, track.start) }
}

/// Remove routed tracks on the given nets.
pub fn rip_up(board: &mut Sexp, nets: &[String]) -> usize {
    let before = items(board).len();
    items(board).retain(|item| {
        !(matches!(item.head(), Some("segment" | "via" | "arc")) && nets.iter().any(|n| item.value("net") == Some(n)))
    });
    before - items(board).len()
}

/// Drop segments duplicated on the same net, layer, and endpoints, keeping
/// the widest of each group.
pub fn dedupe_segments(board: &mut Sexp) -> usize {
    // (net, layer, endpoints) → (index, width) of the widest segment seen.
    type Key = (String, Layer, (Pt, Pt));
    let mut keep: HashMap<Key, (usize, Nm)> = HashMap::new();
    for (i, item) in items(board).iter().enumerate() {
        let Some(track) = segment(item) else { continue };
        let key = (track.net.clone(), track.layer, ends(&track));
        keep.entry(key)
            .and_modify(|best| {
                if track.width > best.1 {
                    *best = (i, track.width)
                }
            })
            .or_insert((i, track.width));
    }
    let kept: std::collections::HashSet<usize> = keep.values().map(|(i, _)| *i).collect();
    let mut index = 0;
    let before = items(board).len();
    items(board).retain(|item| {
        let i = index;
        index += 1;
        !item.is("segment") || segment(item).is_none() || kept.contains(&i)
    });
    before - items(board).len()
}

/// Every segment on the board, in file order.
pub fn segments(board: &Sexp) -> Vec<Track> {
    board.args().iter().filter_map(segment).collect()
}

/// Apply new widths to segments, matched by position in [`segments`] order.
pub fn set_widths(board: &mut Sexp, widths: &[Option<Nm>]) {
    let mut widths = widths.iter();
    for item in items(board).iter_mut() {
        if segment(item).is_some()
            && let Some(Some(width)) = widths.next()
        {
            set_width(item, *width);
        }
    }
}

/// Make sure a segment exists at least `width` wide, adding it if needed.
pub fn keep_segment(board: &mut Sexp, wanted: &Track) {
    let target = ends(wanted);
    for item in items(board).iter_mut() {
        let Some(track) = segment(item) else { continue };
        if track.net == wanted.net && track.layer == wanted.layer && ends(&track) == target {
            set_width(item, track.width.max(wanted.width));
            return;
        }
    }
    let xy = |head: &str, p: Pt| Sexp::node(head, [sexpr::mm_atom(p.x.0), sexpr::mm_atom(p.y.0)]);
    let node = Sexp::node(
        "segment",
        [
            xy("start", wanted.start),
            xy("end", wanted.end),
            Sexp::node("width", [sexpr::mm_atom(wanted.width.0)]),
            Sexp::node("layer", [Sexp::str(wanted.layer.name())]),
            Sexp::node("net", [Sexp::str(&wanted.net)]),
            Sexp::node("uuid", [Sexp::str(uuid::Uuid::new_v4().to_string())]),
        ],
    );
    let fonts = items(board).iter().rposition(|i| i.is("embedded_fonts")).unwrap_or(items(board).len());
    items(board).insert(fonts, node);
}

/// Detach footprints from a library that only exists inside this board
/// and hide their designators. Returns how many changed.
pub fn unlink_library(board: &mut Sexp, library: &str, except: &[&str]) -> Result<usize> {
    let prefix = format!("{library}:");
    let mut count = 0;
    for item in items(board).iter_mut().filter(|i| i.is("footprint")) {
        let list = item.as_list_mut().ok_or_else(|| anyhow!("malformed footprint"))?;
        let Some(Sexp::Str(id)) = list.get(1) else { continue };
        let reference = list
            .iter()
            .find(|c| c.is("property") && c.arg(0).and_then(Sexp::text) == Some("Reference"))
            .and_then(|p| p.arg(1)?.text())
            .unwrap_or_default()
            .to_owned();
        if !id.starts_with(&prefix) || except.contains(&reference.as_str()) {
            continue;
        }
        list[1] = Sexp::str("");
        for child in list.iter_mut() {
            if child.is("property") && child.arg(0).and_then(Sexp::text) == Some("Reference") {
                let field = child.as_list_mut().expect("property list");
                if !field.iter().any(|c| c.is("hide")) {
                    let at = field.iter().position(|c| c.is("layer")).map_or(field.len(), |i| i + 1);
                    field.insert(at, Sexp::node("hide", [Sexp::atom("yes")]));
                }
            }
        }
        count += 1;
    }
    Ok(count)
}
