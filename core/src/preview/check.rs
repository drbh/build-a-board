//! Placement checks made straight from the scene, without KiCad: copper off
//! the board or too near its edge, and overlapping courtyards. KiCad's DRC
//! makes the same checks (`copper_edge_clearance`, `courtyards_overlap`),
//! but only once the board is routed; these catch a bad placement while it
//! is being made. Each takes its severity from the design's rule severities,
//! as DRC would, and `ignore` turns it off.

use std::collections::BTreeMap;
use std::f64::consts::TAU;

use super::{Footprint, Kind, P, PadShape, Scene, Shape, Side};

/// The copper-to-edge clearance KiCad checks, as the project template sets
/// it (`min_copper_edge_clearance`).
pub const EDGE_CLEARANCE: f64 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn name(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// One problem: what it is, and where on the board to mark it.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub severity: Severity,
    /// The KiCad DRC rule it corresponds to.
    pub rule: &'static str,
    pub message: String,
    /// Board-frame boxes (min, max) around what is wrong.
    pub marks: Vec<(P, P)>,
}

/// Run every check on `scene`. `severities` are the design's DRC rule
/// severities (`error`, `warning`, or `ignore`); a rule not named is an error.
pub fn check(scene: &Scene, severities: &[(String, String)]) -> Vec<Finding> {
    let severity = |rule: &str| match severities.iter().rev().find(|(r, _)| r == rule).map(|(_, s)| s.as_str()) {
        Some("ignore") => None,
        Some("warning") => Some(Severity::Warning),
        _ => Some(Severity::Error),
    };
    let mut findings = Vec::new();
    if let Some(severity) = severity("copper_edge_clearance") {
        edge_clearance(scene, severity, &mut findings);
    }
    if let Some(severity) = severity("courtyards_overlap") {
        courtyards(scene, severity, &mut findings);
    }
    findings
}

type Seg = (P, P);
/// A box (min, max) marking a finding.
type Mark = (P, P);

/// A piece of copper: the points (one, two, or a polygon) it grows from,
/// and how far it grows. A round pad is its centre grown by its radius; a
/// track is its centre line grown by half its width.
struct Copper {
    core: Vec<P>,
    radius: f64,
}

impl Copper {
    fn segments(&self) -> Vec<Seg> {
        match self.core.len() {
            0 => Vec::new(),
            1 => vec![(self.core[0], self.core[0])],
            2 => vec![(self.core[0], self.core[1])],
            n => (0..n).map(|i| (self.core[i], self.core[(i + 1) % n])).collect(),
        }
    }

    fn bounds(&self) -> (P, P) {
        let (lo, hi) = bounds(&self.core);
        ((lo.0 - self.radius, lo.1 - self.radius), (hi.0 + self.radius, hi.1 + self.radius))
    }
}

/// How a piece of copper sits against the board edge.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
enum Edge {
    /// Not on the board at all, or partly over the edge.
    Off,
    /// On the board, `margin` mm from the edge.
    Near { margin: f64 },
}

fn against(copper: &Copper, edge: &[Seg]) -> Option<Edge> {
    let gap = copper
        .segments()
        .iter()
        .flat_map(|s| edge.iter().map(move |e| seg_distance(*s, *e)))
        .fold(f64::INFINITY, f64::min);
    let margin = gap - copper.radius;
    if !inside(copper.core[0], edge) || margin < 0.0 {
        Some(Edge::Off)
    } else if margin < EDGE_CLEARANCE - 1e-6 {
        Some(Edge::Near { margin })
    } else {
        None
    }
}

fn edge_clearance(scene: &Scene, severity: Severity, findings: &mut Vec<Finding>) {
    let edge: Vec<Seg> = scene
        .items
        .iter()
        .filter(|item| item.kind == Kind::Edge)
        .flat_map(|item| outline(&item.shape, |p| p))
        .collect();
    if edge.is_empty() {
        return;
    }
    let finding =
        |message: String, marks: Vec<(P, P)>| Finding { severity, rule: "copper_edge_clearance", message, marks };
    for fp in &scene.footprints {
        let mut off = Vec::new();
        let mut near: Vec<(String, f64, (P, P))> = Vec::new();
        for item in &fp.items {
            let Shape::Pad { number, at, angle, size, shape, .. } = &item.shape else { continue };
            let copper = pad(fp, *at, *angle, *size, shape);
            match against(&copper, &edge) {
                Some(Edge::Off) => off.push((number.clone(), copper.bounds())),
                Some(Edge::Near { margin }) => near.push((number.clone(), margin, copper.bounds())),
                None => {}
            }
        }
        if !off.is_empty() {
            let pads = pad_list(off.iter().map(|(n, _)| n.as_str()));
            findings.push(finding(
                format!("{} is off the board: {pads} not inside the board edge", fp.reference),
                off.into_iter().map(|(_, b)| b).collect(),
            ));
        } else if let Some(worst) = near.iter().map(|(_, m, _)| *m).reduce(f64::min) {
            let pads = pad_list(near.iter().map(|(n, _, _)| n.as_str()));
            findings.push(finding(
                format!(
                    "{} is too close to the board edge: {pads} {worst:.2} mm from it (copper needs {EDGE_CLEARANCE} mm)",
                    fp.reference
                ),
                near.into_iter().map(|(_, _, b)| b).collect(),
            ));
        }
    }
    // Tracks and vias, one finding per net.
    let mut by_net: BTreeMap<&str, (Option<Edge>, Vec<Mark>)> = BTreeMap::new();
    for item in &scene.items {
        let copper = match (&item.kind, &item.shape) {
            (Kind::CopperFront | Kind::CopperBack, Shape::Line { a, b, width }) => {
                Copper { core: vec![*a, *b], radius: width / 2.0 }
            }
            (Kind::Via, Shape::Via { at, diameter, .. }) => Copper { core: vec![*at], radius: diameter / 2.0 },
            _ => continue,
        };
        let Some(state) = against(&copper, &edge) else { continue };
        let entry = by_net.entry(item.net.as_deref().unwrap_or("no net")).or_default();
        entry.0 = Some(match (entry.0, state) {
            (Some(Edge::Near { margin: a }), Edge::Near { margin: b }) => Edge::Near { margin: a.min(b) },
            (Some(Edge::Off), _) | (_, Edge::Off) => Edge::Off,
            (None, state) => state,
        });
        entry.1.push(copper.bounds());
    }
    for (net, (state, marks)) in by_net {
        let message = match state {
            Some(Edge::Off) => format!("copper on {net} runs off the board"),
            Some(Edge::Near { margin }) => format!(
                "copper on {net} is too close to the board edge: {margin:.2} mm from it (copper needs {EDGE_CLEARANCE} mm)"
            ),
            None => continue,
        };
        findings.push(finding(message, marks));
    }
}

/// "pad 1" or "pads 1, 2".
fn pad_list<'a>(numbers: impl Iterator<Item = &'a str>) -> String {
    let numbers: Vec<&str> = numbers.collect();
    if numbers.len() == 1 { format!("pad {} is", numbers[0]) } else { format!("pads {} are", numbers.join(", ")) }
}

/// A footprint's pad as board-frame copper.
fn pad(fp: &Footprint, at: P, angle: f64, (w, h): P, shape: &PadShape) -> Copper {
    let (s, c) = (-angle).to_radians().sin_cos();
    let local = |(x, y): P| fp.place((at.0 + x * c - y * s, at.1 + x * s + y * c));
    let radius = match shape {
        PadShape::Circle => return Copper { core: vec![local((0.0, 0.0))], radius: w / 2.0 },
        PadShape::Rect { radius } => radius.min(w / 2.0).min(h / 2.0),
    };
    let (x, y) = (w / 2.0 - radius, h / 2.0 - radius);
    let core = if x < 1e-9 && y < 1e-9 {
        vec![local((0.0, 0.0))]
    } else if x < 1e-9 || y < 1e-9 {
        vec![local((-x, -y)), local((x, y))]
    } else {
        vec![local((-x, -y)), local((x, -y)), local((x, y)), local((-x, y))]
    };
    Copper { core, radius }
}

fn courtyards(scene: &Scene, severity: Severity, findings: &mut Vec<Finding>) {
    let outlines: Vec<(&Footprint, Side, Vec<Seg>)> = scene
        .footprints
        .iter()
        .flat_map(|fp| {
            [Side::Front, Side::Back].into_iter().filter_map(move |side| {
                let segments: Vec<Seg> = fp
                    .courtyard
                    .iter()
                    .filter(|(s, _)| *s == side)
                    .flat_map(|(_, shape)| outline(shape, |p| fp.place(p)))
                    .collect();
                (!segments.is_empty()).then_some((fp, side, segments))
            })
        })
        .collect();
    let boxes: Vec<(P, P)> = outlines
        .iter()
        .map(|(_, _, segs)| bounds(&segs.iter().flat_map(|(a, b)| [*a, *b]).collect::<Vec<_>>()))
        .collect();
    for i in 0..outlines.len() {
        for j in i + 1..outlines.len() {
            let ((a, side_a, sa), (b, side_b, sb)) = (&outlines[i], &outlines[j]);
            if side_a != side_b || !overlapping_boxes(boxes[i], boxes[j]) || !overlap(sa, sb) {
                continue;
            }
            let ((alo, ahi), (blo, bhi)) = (boxes[i], boxes[j]);
            let mark = ((alo.0.max(blo.0), alo.1.max(blo.1)), (ahi.0.min(bhi.0), ahi.1.min(bhi.1)));
            findings.push(Finding {
                severity,
                rule: "courtyards_overlap",
                message: format!("{} and {} overlap: their courtyards cross", a.reference, b.reference),
                marks: vec![mark],
            });
        }
    }
}

fn overlapping_boxes((alo, ahi): (P, P), (blo, bhi): (P, P)) -> bool {
    alo.0 < bhi.0 && blo.0 < ahi.0 && alo.1 < bhi.1 && blo.1 < ahi.1
}

/// Whether two closed outlines share area (touching is not overlapping).
/// Each edge is cut wherever the other outline meets it; a piece lying
/// strictly inside the other outline means they overlap. That holds up when
/// edges run along each other, as they do for parts of equal height side by
/// side. Identical outlines meet everywhere, so they are caught separately.
fn overlap(a: &[Seg], b: &[Seg]) -> bool {
    const EPS: f64 = 1e-4;
    let strictly_inside =
        |p: P, outline: &[Seg]| inside(p, outline) && outline.iter().all(|e| point_segment_distance(p, *e) > EPS);
    let piece_inside = |from: &[Seg], into: &[Seg]| {
        from.iter().any(|&s| cuts(s, into).windows(2).any(|w| strictly_inside(at(s, (w[0] + w[1]) / 2.0), into)))
    };
    let on = |from: &[Seg], into: &[Seg]| {
        from.iter().all(|(p, _)| into.iter().any(|e| point_segment_distance(*p, *e) <= EPS))
    };
    piece_inside(a, b) || piece_inside(b, a) || (on(a, b) && on(b, a))
}

/// The point a fraction `t` along a segment.
fn at((p, q): Seg, t: f64) -> P {
    (p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t)
}

/// Where along `s` (0 to 1, in order) the segments of `outline` meet it,
/// with its ends.
fn cuts(s: Seg, outline: &[Seg]) -> Vec<f64> {
    const EPS: f64 = 1e-9;
    let (p, q) = s;
    let d = (q.0 - p.0, q.1 - p.1);
    let length = d.0 * d.0 + d.1 * d.1;
    let mut ts = vec![0.0, 1.0];
    if length < EPS {
        return ts;
    }
    let along = |x: P| ((x.0 - p.0) * d.0 + (x.1 - p.1) * d.1) / length;
    for &(c, e) in outline {
        let f = (e.0 - c.0, e.1 - c.1);
        let denominator = d.0 * f.1 - d.1 * f.0;
        if denominator.abs() < EPS {
            // Parallel: only a collinear segment cuts, at its ends.
            if cross(p, q, c).abs() < EPS * length.sqrt() {
                ts.extend([along(c), along(e)]);
            }
            continue;
        }
        let t = ((c.0 - p.0) * f.1 - (c.1 - p.1) * f.0) / denominator;
        let u = ((c.0 - p.0) * d.1 - (c.1 - p.1) * d.0) / denominator;
        if (-EPS..=1.0 + EPS).contains(&u) {
            ts.push(t);
        }
    }
    ts.retain(|t| (0.0..=1.0).contains(t));
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|x, y| (*x - *y).abs() < EPS);
    ts
}

/// Board-frame line segments tracing a shape's outline (arcs and circles as
/// short chords).
fn outline(shape: &Shape, place: impl Fn(P) -> P) -> Vec<Seg> {
    let chain = |points: Vec<P>, closed: bool| -> Vec<Seg> {
        let points: Vec<P> = points.into_iter().map(&place).collect();
        let n = points.len();
        let count = if closed { n } else { n.saturating_sub(1) };
        (0..count).map(|i| (points[i], points[(i + 1) % n])).collect()
    };
    match shape {
        Shape::Line { a, b, .. } => chain(vec![*a, *b], false),
        Shape::Poly { points, .. } => chain(points.clone(), true),
        Shape::Circle { center, radius, .. } => {
            let n = 64;
            chain(
                (0..n)
                    .map(|i| {
                        let t = TAU * f64::from(i) / f64::from(n);
                        (center.0 + radius * t.cos(), center.1 + radius * t.sin())
                    })
                    .collect(),
                true,
            )
        }
        Shape::Arc { start, mid, end, .. } => chain(arc(*start, *mid, *end), false),
        _ => Vec::new(),
    }
}

/// Points along the arc from `start` through `mid` to `end`.
fn arc(start: P, mid: P, end: P) -> Vec<P> {
    let (ax, ay, bx, by, cx, cy) = (start.0, start.1, mid.0, mid.1, end.0, end.1);
    let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    if d.abs() < 1e-12 {
        return vec![start, end];
    }
    let sq = |x: f64, y: f64| x * x + y * y;
    let ux = (sq(ax, ay) * (by - cy) + sq(bx, by) * (cy - ay) + sq(cx, cy) * (ay - by)) / d;
    let uy = (sq(ax, ay) * (cx - bx) + sq(bx, by) * (ax - cx) + sq(cx, cy) * (bx - ax)) / d;
    let radius = (ax - ux).hypot(ay - uy);
    let angle = |(x, y): P| (y - uy).atan2(x - ux);
    let turn = |from: f64, to: f64| (to - from).rem_euclid(TAU);
    let a0 = angle(start);
    let (to_mid, to_end) = (turn(a0, angle(mid)), turn(a0, angle(end)));
    let sweep = if to_mid < to_end { to_end } else { to_end - TAU };
    let n = 32;
    (0..=n)
        .map(|i| {
            let t = a0 + sweep * f64::from(i) / f64::from(n);
            (ux + radius * t.cos(), uy + radius * t.sin())
        })
        .collect()
}

fn bounds(points: &[P]) -> (P, P) {
    points.iter().fold(((f64::INFINITY, f64::INFINITY), (f64::NEG_INFINITY, f64::NEG_INFINITY)), |(lo, hi), p| {
        ((lo.0.min(p.0), lo.1.min(p.1)), (hi.0.max(p.0), hi.1.max(p.1)))
    })
}

/// Whether `p` is inside the closed outlines `edges` (even-odd), which may
/// come in any order.
fn inside(p: P, edges: &[Seg]) -> bool {
    edges.iter().fold(false, |inside, (a, b)| {
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) * (b.0 - a.0) / (b.1 - a.1) { !inside } else { inside }
    })
}

fn cross(o: P, a: P, b: P) -> f64 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

/// Whether two segments cross at a point inside both (not merely touching).
fn crosses((a, b): Seg, (c, d): Seg) -> bool {
    const EPS: f64 = 1e-9;
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    ((d1 > EPS && d2 < -EPS) || (d1 < -EPS && d2 > EPS)) && ((d3 > EPS && d4 < -EPS) || (d3 < -EPS && d4 > EPS))
}

fn point_segment_distance(p: P, (a, b): Seg) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length).clamp(0.0, 1.0) };
    (p.0 - (a.0 + t * dx)).hypot(p.1 - (a.1 + t * dy))
}

fn seg_distance(s: Seg, t: Seg) -> f64 {
    if crosses(s, t) {
        return 0.0;
    }
    [
        point_segment_distance(s.0, t),
        point_segment_distance(s.1, t),
        point_segment_distance(t.0, s),
        point_segment_distance(t.1, s),
    ]
    .into_iter()
    .fold(f64::INFINITY, f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(lo: f64, hi: f64) -> Vec<Seg> {
        let p = [(lo, lo), (hi, lo), (hi, hi), (lo, hi)];
        (0..4).map(|i| (p[i], p[(i + 1) % 4])).collect()
    }

    #[test]
    fn copper_against_the_edge() {
        let edge = square(0.0, 10.0);
        let dot = |x: f64, y: f64, r: f64| Copper { core: vec![(x, y)], radius: r };
        assert_eq!(against(&dot(5.0, 5.0, 0.5), &edge), None);
        assert_eq!(against(&dot(-3.0, 5.0, 0.5), &edge), Some(Edge::Off));
        assert_eq!(against(&dot(0.2, 5.0, 0.5), &edge), Some(Edge::Off));
        match against(&dot(0.8, 5.0, 0.5), &edge) {
            Some(Edge::Near { margin }) => assert!((margin - 0.3).abs() < 1e-9),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn courtyards_that_touch_do_not_overlap() {
        assert!(overlap(&square(0.0, 2.0), &square(1.0, 3.0)));
        assert!(!overlap(&square(0.0, 2.0), &square(2.0, 4.0)));
        assert!(overlap(&square(0.0, 10.0), &square(2.0, 3.0)), "one inside the other");
        assert!(!overlap(&square(0.0, 1.0), &square(5.0, 6.0)));
        assert!(overlap(&square(0.0, 2.0), &square(0.0, 2.0)), "identical");
    }

    #[test]
    fn courtyards_of_equal_height_side_by_side() {
        let rect = |x0: f64, x1: f64| {
            let p = [(x0, 0.0), (x1, 0.0), (x1, 1.5), (x0, 1.5)];
            (0..4).map(|i| (p[i], p[(i + 1) % 4])).collect::<Vec<Seg>>()
        };
        assert!(overlap(&rect(0.0, 3.0), &rect(2.0, 5.0)), "overlapping by 1 mm");
        assert!(!overlap(&rect(0.0, 3.0), &rect(3.0, 6.0)), "sharing an edge");
    }

    #[test]
    fn arcs_pass_through_their_middle() {
        let points = arc((1.0, 0.0), (0.0, 1.0), (-1.0, 0.0));
        assert!(points.iter().all(|(x, y)| (x.hypot(*y) - 1.0).abs() < 1e-9));
        assert!(points.iter().any(|&(x, y)| x.abs() < 1e-9 && (y - 1.0).abs() < 1e-9));
    }
}
