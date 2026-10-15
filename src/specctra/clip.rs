//! Just enough of Clipper2 1.3.0 (the version KiCad 10 bundles) to
//! reproduce KiCad's courtyard and outline polygons vertex for vertex.
//!
//! KiCad shrinks every courtyard by the DRC epsilon with
//! `SHAPE_POLY_SET::Inflate( -5 µm, CHAMFER_ACUTE_CORNERS )` and later
//! `Simplify()`s it. Both are Clipper2 operations, so the exported
//! coordinates carry Clipper2's own rounding (perpendicular offsets are
//! `std::round`ed, edge intersections are truncated) and its output vertex
//! order. This module implements the offset exactly and resolves the small
//! self-intersections it creates the way Clipper2's sweep does, which is
//! sufficient for the simple outlines footprints use.

use super::geom::P;

#[derive(Clone, Copy, Debug)]
struct Pd {
    x: f64,
    y: f64,
}

fn cross(a: Pd, b: Pd) -> f64 {
    a.y * b.x - b.y * a.x
}

fn dot(a: Pd, b: Pd) -> f64 {
    a.x * b.x + a.y * b.y
}

/// Clipper2 `Point64( double, double )`: `std::round` each coordinate.
fn p64(x: f64, y: f64) -> P {
    P::new(x.round() as i64, y.round() as i64)
}

fn unit_normal(a: P, b: P) -> Pd {
    if a == b {
        return Pd { x: 0.0, y: 0.0 };
    }
    let dx = (b.x - a.x) as f64;
    let dy = (b.y - a.y) as f64;
    let inv = 1.0 / dx.hypot(dy);
    Pd { x: dy * inv, y: -(dx * inv) }
}

/// Clipper2 `Area` (positive for the orientation Clipper treats as outer).
pub fn area(path: &[P]) -> f64 {
    let n = path.len();
    if n < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    let mut prev = path[n - 1];
    for &cur in path {
        a += (prev.y + cur.y) as f64 * (prev.x - cur.x) as f64;
        prev = cur;
    }
    a * 0.5
}

fn strip_duplicates(path: &[P]) -> Vec<P> {
    let mut out: Vec<P> = Vec::with_capacity(path.len());
    for &p in path {
        if out.last() != Some(&p) {
            out.push(p);
        }
    }
    while out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
    out
}

/// The points one `OffsetPoint` call contributes for vertex `j`, tagged
/// with whether they form a concave (self-crossing) insertion.
struct Joint {
    points: Vec<P>,
    concave: bool,
}

/// `ClipperOffset` with `JoinType::Miter` (miter limit 2) and
/// `EndType::Polygon` on one closed path, followed by the union that
/// removes the reversed loops at concave joins.
pub fn offset_polygon(path: &[P], delta: f64) -> Vec<P> {
    let path = strip_duplicates(path);
    let n = path.len();
    if n < 3 {
        return path;
    }
    // A single outer path is "reversed" when its area is negative; Clipper
    // then offsets with the opposite sign so shrinking stays inward.
    let reversed = area(&path) < 0.0;
    let delta = if reversed { -delta } else { delta };
    let temp_lim = 2.0 / (2.0 * 2.0); // miter limit 2.0
    let norms: Vec<Pd> = (0..n).map(|i| unit_normal(path[i], path[(i + 1) % n])).collect();

    let perp = |pt: P, norm: Pd| p64(pt.x as f64 + norm.x * delta, pt.y as f64 + norm.y * delta);
    let mut joints = Vec::with_capacity(n);
    for j in 0..n {
        let k = if j == 0 { n - 1 } else { j - 1 };
        let mut sin_a = cross(norms[j], norms[k]);
        let cos_a = dot(norms[j], norms[k]);
        sin_a = sin_a.clamp(-1.0, 1.0);
        let pt = path[j];
        if cos_a > -0.99 && sin_a * delta < 0.0 {
            joints.push(Joint { points: vec![perp(pt, norms[k]), pt, perp(pt, norms[j])], concave: true });
        } else if cos_a > 0.999 || cos_a > temp_lim - 1.0 {
            let q = delta / (cos_a + 1.0);
            let x = pt.x as f64 + (norms[k].x + norms[j].x) * q;
            let y = pt.y as f64 + (norms[k].y + norms[j].y) * q;
            joints.push(Joint { points: vec![p64(x, y)], concave: false });
        } else {
            joints.push(Joint { points: do_square(pt, path[k], norms[j], norms[k], delta), concave: false });
        }
    }

    let raw: Vec<P> = joints.iter().flat_map(|j| j.points.iter().copied()).collect();
    let scanlines = {
        let mut ys: Vec<i64> = raw.iter().map(|p| p.y).collect();
        ys.sort_unstable();
        ys.dedup();
        ys
    };

    // Each concave joint's loop collapses to the crossing of the offset
    // edges on either side of it.
    let mut out = Vec::with_capacity(n);
    for j in 0..n {
        let joint = &joints[j];
        if !joint.concave {
            out.extend_from_slice(&joint.points);
            continue;
        }
        let prev = &joints[(j + n - 1) % n];
        let next = &joints[(j + 1) % n];
        let in_edge = (*prev.points.last().unwrap(), joint.points[0]);
        let out_edge = (joint.points[2], next.points[0]);
        match sweep_intersection(in_edge, out_edge, &scanlines) {
            Some(ip) => out.push(ip),
            None => out.extend_from_slice(&joint.points),
        }
    }
    let out = strip_duplicates(&out);
    remove_collinear(&out)
}

fn do_square(pt: P, pk: P, nj: Pd, nk: Pd, delta: f64) -> Vec<P> {
    let normalize = |v: Pd| {
        let h = (v.x * v.x + v.y * v.y).sqrt();
        if h.abs() < 0.001 { Pd { x: 0.0, y: 0.0 } } else { Pd { x: v.x / h, y: v.y / h } }
    };
    let vec = normalize(Pd { x: -nk.y + nj.y, y: nk.x + -nj.x });
    let abs_delta = delta.abs();
    let ptq = Pd { x: pt.x as f64 + abs_delta * vec.x, y: pt.y as f64 + abs_delta * vec.y };
    let pt1 = Pd { x: ptq.x + delta * vec.y, y: ptq.y + delta * -vec.x };
    let pt2 = Pd { x: ptq.x + delta * -vec.y, y: ptq.y + delta * vec.x };
    let pt3 = Pd { x: pk.x as f64 + nk.x * delta, y: pk.y as f64 + nk.y * delta };
    let pt4 = Pd { x: pt.x as f64 + nk.x * delta, y: pt.y as f64 + nk.y * delta };
    let ip = intersect_lines(pt1, pt2, pt3, pt4);
    let reflected = Pd { x: ptq.x + (ptq.x - ip.x), y: ptq.y + (ptq.y - ip.y) };
    vec![p64(ip.x, ip.y), p64(reflected.x, reflected.y)]
}

fn intersect_lines(a1: Pd, a2: Pd, b1: Pd, b2: Pd) -> Pd {
    if a1.x == a2.x {
        if b1.x == b2.x {
            return Pd { x: 0.0, y: 0.0 };
        }
        let m2 = (b2.y - b1.y) / (b2.x - b1.x);
        let c2 = b1.y - m2 * b1.x;
        Pd { x: a1.x, y: m2 * a1.x + c2 }
    } else if b1.x == b2.x {
        let m1 = (a2.y - a1.y) / (a2.x - a1.x);
        let c1 = a1.y - m1 * a1.x;
        Pd { x: b1.x, y: m1 * b1.x + c1 }
    } else {
        let m1 = (a2.y - a1.y) / (a2.x - a1.x);
        let c1 = a1.y - m1 * a1.x;
        let m2 = (b2.y - b1.y) / (b2.x - b1.x);
        let c2 = b1.y - m2 * b1.x;
        if m1 == m2 {
            return Pd { x: 0.0, y: 0.0 };
        }
        let x = (c2 - c1) / (m1 - m2);
        Pd { x, y: m1 * x + c1 }
    }
}

/// An edge as Clipper2's sweep sees it: `bot` has the larger Y.
#[derive(Clone, Copy)]
struct Edge {
    bot: P,
    top: P,
}

impl Edge {
    fn new(a: P, b: P) -> Self {
        if a.y >= b.y { Self { bot: a, top: b } } else { Self { bot: b, top: a } }
    }

    fn dx(&self) -> f64 {
        let dy = (self.top.y - self.bot.y) as f64;
        if dy == 0.0 { f64::NEG_INFINITY } else { (self.top.x - self.bot.x) as f64 / dy }
    }

    /// Clipper2 `TopX`.
    fn x_at(&self, y: i64) -> i64 {
        if y == self.top.y || self.top.x == self.bot.x {
            self.top.x
        } else if y == self.bot.y {
            self.bot.x
        } else {
            self.bot.x + (self.dx() * (y - self.bot.y) as f64).round_ties_even() as i64
        }
    }

    fn is_horizontal(&self) -> bool {
        self.top.y == self.bot.y
    }
}

/// Clipper2 1.3.0 `GetIntersectPoint`: the crossing along `ln1`, truncated.
fn get_intersect_point(ln1a: P, ln1b: P, ln2a: P, ln2b: P) -> Option<P> {
    let dx1 = (ln1b.x - ln1a.x) as f64;
    let dy1 = (ln1b.y - ln1a.y) as f64;
    let dx2 = (ln2b.x - ln2a.x) as f64;
    let dy2 = (ln2b.y - ln2a.y) as f64;
    let det = dy1 * dx2 - dy2 * dx1;
    if det == 0.0 {
        return None;
    }
    let t = ((ln1a.x - ln2a.x) as f64 * dy2 - (ln1a.y - ln2a.y) as f64 * dx2) / det;
    Some(if t <= 0.0 {
        ln1a
    } else if t >= 1.0 {
        ln1b
    } else {
        P::new((ln1a.x as f64 + t * dx1) as i64, (ln1a.y as f64 + t * dy1) as i64)
    })
}

fn closest_point_on_segment(p: P, a: P, b: P) -> P {
    if a == b {
        return a;
    }
    let dx = (b.x - a.x) as f64;
    let dy = (b.y - a.y) as f64;
    let mut q = ((p.x - a.x) as f64 * dx + (p.y - a.y) as f64 * dy) / (dx * dx + dy * dy);
    q = q.clamp(0.0, 1.0);
    P::new((a.x as f64 + q * dx).round() as i64, (a.y as f64 + q * dy).round() as i64)
}

/// The crossing of two segments as Clipper2's union computes it, or `None`
/// when they do not cross.
fn sweep_intersection(a: (P, P), b: (P, P), scanlines: &[i64]) -> Option<P> {
    let (ea, eb) = (Edge::new(a.0, a.1), Edge::new(b.0, b.1));
    if !segments_cross(a.0, a.1, b.0, b.1) {
        return None;
    }
    // Horizontal edges are processed by DoHorizontal, which meets the other
    // edge at its current X on the horizontal's Y.
    if ea.is_horizontal() || eb.is_horizontal() {
        let (h, o) = if ea.is_horizontal() { (ea, eb) } else { (eb, ea) };
        return Some(P::new(o.x_at(h.bot.y), h.bot.y));
    }
    // The scanbeam holding the true crossing.
    let true_y = {
        let dx1 = (a.1.x - a.0.x) as f64;
        let dy1 = (a.1.y - a.0.y) as f64;
        let dx2 = (b.1.x - b.0.x) as f64;
        let dy2 = (b.1.y - b.0.y) as f64;
        let det = dy1 * dx2 - dy2 * dx1;
        let t = ((a.0.x - b.0.x) as f64 * dy2 - (a.0.y - b.0.y) as f64 * dx2) / det;
        a.0.y as f64 + t * dy1
    };
    let bot_limit = ea.bot.y.min(eb.bot.y);
    let top_limit = ea.top.y.max(eb.top.y);
    let beam: Vec<i64> = scanlines.iter().copied().filter(|&y| y >= top_limit && y <= bot_limit).collect();
    let bot_y = beam.iter().copied().filter(|&y| y as f64 >= true_y).min().unwrap_or(bot_limit);
    let top_y = beam.iter().copied().filter(|&y| y < bot_y).max().unwrap_or(top_limit);
    // e1 is the edge on the left at the bottom of the scanbeam.
    let (e1, e2) = if ea.x_at(bot_y) <= eb.x_at(bot_y) { (ea, eb) } else { (eb, ea) };
    let mut ip = get_intersect_point(e1.bot, e1.top, e2.bot, e2.top).unwrap_or(P::new(e1.x_at(top_y), top_y));
    if ip.y > bot_y || ip.y < top_y {
        let (d1, d2) = (e1.dx().abs(), e2.dx().abs());
        if d1 > 100.0 && d2 > 100.0 {
            ip = if d1 > d2 {
                closest_point_on_segment(ip, e1.bot, e1.top)
            } else {
                closest_point_on_segment(ip, e2.bot, e2.top)
            };
        } else if d1 > 100.0 {
            ip = closest_point_on_segment(ip, e1.bot, e1.top);
        } else if d2 > 100.0 {
            ip = closest_point_on_segment(ip, e2.bot, e2.top);
        } else {
            ip.y = if ip.y < top_y { top_y } else { bot_y };
            ip.x = if d1 < d2 { e1.x_at(ip.y) } else { e2.x_at(ip.y) };
        }
    }
    Some(ip)
}

fn orient(a: P, b: P, c: P) -> i128 {
    (b.x - a.x) as i128 * (c.y - a.y) as i128 - (b.y - a.y) as i128 * (c.x - a.x) as i128
}

fn segments_cross(a1: P, a2: P, b1: P, b2: P) -> bool {
    let d1 = orient(b1, b2, a1);
    let d2 = orient(b1, b2, a2);
    let d3 = orient(a1, a2, b1);
    let d4 = orient(a1, a2, b2);
    ((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0))
}

/// Clipper2 with `PreserveCollinear( false )` drops exactly collinear vertices.
fn remove_collinear(path: &[P]) -> Vec<P> {
    let mut pts = path.to_vec();
    let mut changed = true;
    while changed && pts.len() > 3 {
        changed = false;
        let n = pts.len();
        for i in 0..n {
            let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            if orient(a, b, c) == 0 {
                pts.remove(i);
                changed = true;
                break;
            }
        }
    }
    pts
}

/// The vertex order Clipper2's union emits for a simple outer polygon:
/// positive area, ending at the top-most (then right-most) vertex.
pub fn canonical_order(path: &[P]) -> Vec<P> {
    let mut pts = remove_collinear(&strip_duplicates(path));
    if area(&pts) < 0.0 {
        pts.reverse();
    }
    if let Some(last) = (0..pts.len()).min_by(|&a, &b| pts[a].y.cmp(&pts[b].y).then(pts[b].x.cmp(&pts[a].x))) {
        let len = pts.len();
        pts.rotate_left((last + 1) % len);
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrinks_rectangles_exactly() {
        let rect = [
            P::new(-1_480_000, -730_000),
            P::new(1_480_000, -730_000),
            P::new(1_480_000, 730_000),
            P::new(-1_480_000, 730_000),
        ];
        let shrunk = canonical_order(&offset_polygon(&rect, -5000.0));
        assert_eq!(
            shrunk,
            vec![
                P::new(1_475_000, 725_000),
                P::new(-1_475_000, 725_000),
                P::new(-1_475_000, -725_000),
                P::new(1_475_000, -725_000)
            ]
        );
    }
}
