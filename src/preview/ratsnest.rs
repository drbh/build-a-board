//! The ratsnest: connections the copper does not make yet.
//!
//! Pads, track ends, and vias are joined wherever they touch on a shared
//! layer, and a zone joins everything of its net inside its outline (or
//! KiCad's fill, when the board has one). Each net's remaining islands are
//! then linked by a minimum spanning tree of their nearest points.

use std::collections::BTreeMap;

use super::{P, PlacedPad, Side, Track, Via};
use crate::design::Layer;

/// Points closer than this are the same point.
const TOUCH: f64 = 1e-3;

struct Sets(Vec<usize>);

impl Sets {
    fn find(&mut self, mut i: usize) -> usize {
        while self.0[i] != i {
            self.0[i] = self.0[self.0[i]];
            i = self.0[i];
        }
        i
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        self.0[a] = b;
    }
}

/// A connectable point: where it is, which copper it reaches, and how far.
struct Node<'a> {
    net: &'a str,
    at: P,
    side: Side,
    /// Index into `pads` when the node is a pad.
    pad: Option<usize>,
}

fn dist(a: P, b: P) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// Whether `p` lies on the pad's copper.
fn on_pad(pad: &PlacedPad, p: P) -> bool {
    let (dx, dy) = (p.0 - pad.at.0, p.1 - pad.at.1);
    if pad.circle {
        return dx.hypot(dy) <= pad.size.0.max(pad.size.1) / 2.0 + TOUCH;
    }
    // Into the pad's own frame (counter-clockwise on screen).
    let (s, c) = pad.angle.to_radians().sin_cos();
    let (u, v) = (dx * c - dy * s, dx * s + dy * c);
    u.abs() <= pad.size.0 / 2.0 + TOUCH && v.abs() <= pad.size.1 / 2.0 + TOUCH
}

/// Distance from `p` to the segment `a`–`b`.
fn to_segment(p: P, a: P, b: P) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx * dx + dy * dy;
    let t = if len == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len).clamp(0.0, 1.0) };
    dist(p, (a.0 + t * dx, a.1 + t * dy))
}

fn inside(polygon: &[P], p: P) -> bool {
    let mut inside = false;
    let mut j = polygon.len().wrapping_sub(1);
    for i in 0..polygon.len() {
        let ((xi, yi), (xj, yj)) = (polygon[i], polygon[j]);
        if (yi > p.1) != (yj > p.1) && p.0 < (xj - xi) * (p.1 - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn reaches(side: Side, layer: Layer) -> bool {
    side.has(layer)
}

/// The unrouted connections: `(net, from, to)`.
pub fn unrouted(
    pads: &[PlacedPad],
    tracks: &[Track],
    vias: &[Via],
    zones: &[(Layer, String, Vec<P>)],
) -> Vec<(String, P, P)> {
    let mut nodes: Vec<Node> = Vec::new();
    for (i, pad) in pads.iter().enumerate() {
        nodes.push(Node { net: &pad.net, at: pad.at, side: pad.side, pad: Some(i) });
    }
    let first_via = nodes.len();
    for via in vias {
        nodes.push(Node { net: &via.net, at: via.at, side: Side::Both, pad: None });
    }
    let first_end = nodes.len();
    for track in tracks {
        let side = if track.layer.is_back() { Side::Back } else { Side::Front };
        nodes.push(Node { net: &track.net, at: track.a, side, pad: None });
        nodes.push(Node { net: &track.net, at: track.b, side, pad: None });
    }
    let mut sets = Sets((0..nodes.len() + zones.len()).collect());

    for (t, track) in tracks.iter().enumerate() {
        let (a, b) = (first_end + 2 * t, first_end + 2 * t + 1);
        sets.join(a, b);
        for (n, node) in nodes.iter().enumerate() {
            if n == a || n == b || node.net != track.net || !reaches(node.side, track.layer) {
                continue;
            }
            let touches = match node.pad {
                // A track touches a pad if either end lies on it.
                Some(p) => on_pad(&pads[p], track.a) || on_pad(&pads[p], track.b),
                // Vias and other track ends touch anywhere along the track.
                None => {
                    let reach = if n < first_end { vias[n - first_via].diameter / 2.0 } else { 0.0 };
                    to_segment(node.at, track.a, track.b) <= track.width / 2.0 + reach + TOUCH
                }
            };
            if touches {
                sets.join(a, n);
            }
        }
    }
    for (v, via) in vias.iter().enumerate() {
        for (p, pad) in pads.iter().enumerate() {
            if pad.net == via.net && (on_pad(pad, via.at) || dist(pad.at, via.at) <= via.diameter / 2.0) {
                sets.join(first_via + v, p);
            }
        }
    }
    for (z, (layer, net, outline)) in zones.iter().enumerate() {
        let zone = nodes.len() + z;
        for (n, node) in nodes.iter().enumerate() {
            if node.net == net && reaches(node.side, *layer) && inside(outline, node.at) {
                sets.join(zone, n);
            }
        }
    }

    // Each net's islands, as lists of node indices.
    let mut islands: BTreeMap<&str, BTreeMap<usize, Vec<usize>>> = BTreeMap::new();
    for (n, node) in nodes.iter().enumerate() {
        let root = sets.find(n);
        islands.entry(node.net).or_default().entry(root).or_default().push(n);
    }
    let mut links = Vec::new();
    for (net, by_root) in islands {
        // Islands without a pad (a stray via or stub) need no connection.
        let groups: Vec<&Vec<usize>> = by_root.values().filter(|g| g.iter().any(|&n| nodes[n].pad.is_some())).collect();
        if groups.len() < 2 {
            continue;
        }
        // Prim's algorithm over islands, linking their closest points.
        let mut joined = vec![false; groups.len()];
        joined[0] = true;
        for _ in 1..groups.len() {
            let mut best: Option<(f64, usize, P, P)> = None;
            for group in groups.iter().enumerate().filter(|(i, _)| joined[*i]).map(|(_, g)| g) {
                for (j, other) in groups.iter().enumerate().filter(|(j, _)| !joined[*j]) {
                    for &a in group.iter() {
                        for &b in other.iter() {
                            let d = dist(nodes[a].at, nodes[b].at);
                            if best.is_none_or(|(bd, ..)| d < bd) {
                                best = Some((d, j, nodes[a].at, nodes[b].at));
                            }
                        }
                    }
                }
            }
            let (_, j, a, b) = best.expect("an unjoined island remains");
            joined[j] = true;
            links.push((net.to_owned(), a, b));
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(reference: &str, net: &str, at: P) -> PlacedPad {
        PlacedPad {
            reference: reference.into(),
            number: "1".into(),
            net: net.into(),
            at,
            angle: 0.0,
            size: (1.0, 1.0),
            circle: false,
            side: Side::Front,
        }
    }

    #[test]
    fn unconnected_pads_get_one_link_per_missing_join() {
        let pads = [pad("R1", "A", (0.0, 0.0)), pad("R2", "A", (10.0, 0.0)), pad("R3", "A", (20.0, 0.0))];
        assert_eq!(unrouted(&pads, &[], &[], &[]).len(), 2);
    }

    #[test]
    fn a_track_between_pads_connects_them() {
        let pads = [pad("R1", "A", (0.0, 0.0)), pad("R2", "A", (10.0, 0.0))];
        let track = Track { net: "A".into(), layer: Layer::FCu, a: (0.0, 0.0), b: (10.0, 0.0), width: 0.25 };
        assert!(unrouted(&pads, &[track], &[], &[]).is_empty());
    }

    #[test]
    fn a_back_track_needs_vias_to_reach_front_pads() {
        let pads = [pad("R1", "A", (0.0, 0.0)), pad("R2", "A", (10.0, 0.0))];
        let track = Track { net: "A".into(), layer: Layer::BCu, a: (0.0, 0.0), b: (10.0, 0.0), width: 0.25 };
        assert_eq!(unrouted(&pads, std::slice::from_ref(&track), &[], &[]).len(), 1);
        let vias = [(0.0, 0.0), (10.0, 0.0)].map(|at| Via { net: "A".into(), at, diameter: 0.6, drill: 0.3 });
        assert!(unrouted(&pads, &[track], &vias, &[]).is_empty());
    }

    #[test]
    fn a_zone_joins_what_lies_inside_it() {
        let mut pads = [pad("R1", "GND", (1.0, 1.0)), pad("R2", "GND", (9.0, 9.0))];
        pads.iter_mut().for_each(|p| p.side = Side::Both);
        let square = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert!(unrouted(&pads, &[], &[], &[(Layer::BCu, "GND".into(), square)]).is_empty());
    }
}
