//! KiCad-exact polygon simplification and fracturing (SHAPE_POLY_SET).
//!
//! A KiCad board file cannot store a graphic polygon with holes: when pcbnew
//! is handed one (`EDA_SHAPE::SetPolyShape`), it calls
//! `SHAPE_POLY_SET::Fracture()`, which first simplifies the set with a
//! Clipper2 union and then bridges every hole into its outline. The saved
//! `fp_poly` points are the result. [`kicad_fracture`] reproduces that
//! pipeline from KiCad 10.0.5 exactly: the union runs in the same Clipper2
//! 1.3.0 build KiCad vendors (compiled by `build.rs`); everything around it
//! (`splitCollinearOutlines`, orientation handling, `importTree`, and the
//! cache-friendly fracture) is ported here.
//!
//! Coordinates are KiCad internal units (nanometres).

mod clipper;

pub type Point = (i64, i64);
pub type Ring = Vec<Point>;

/// One polygon: an outline followed by its holes.
type Polygon = Vec<Ring>;

/// What pcbnew saves for a filled graphic polygon with this outline and
/// these holes: one hole-free outline per resulting polygon. A shape without
/// holes is returned unchanged, because pcbnew only fractures when holes exist.
pub fn kicad_fracture(outline: &[Point], holes: &[Ring]) -> Vec<Ring> {
    let outline = closed_chain(outline);
    if holes.is_empty() {
        return vec![outline];
    }
    let mut polygons: Vec<Polygon> =
        vec![std::iter::once(outline).chain(holes.iter().map(|hole| closed_chain(hole))).collect()];
    simplify(&mut polygons);
    polygons.into_iter().map(fracture_single).collect()
}

/// `SHAPE_LINE_CHAIN::Append` per point (dropping consecutive duplicates)
/// followed by `SetClosed(true)` (dropping a repeated closing point).
fn closed_chain(points: &[Point]) -> Ring {
    let mut ring = append_dedup(points.iter().copied());
    if ring.len() > 1 && ring.first() == ring.last() {
        ring.pop();
    }
    ring
}

fn append_dedup(points: impl IntoIterator<Item = Point>) -> Ring {
    let mut ring: Ring = Vec::new();
    for point in points {
        if ring.last() != Some(&point) {
            ring.push(point);
        }
    }
    ring
}

/// `SHAPE_POLY_SET::Simplify()`.
fn simplify(polygons: &mut Vec<Polygon>) {
    split_collinear_outlines(polygons);
    union(polygons);
}

/// `SHAPE_POLY_SET::booleanOp(Union, empty)` followed by `importTree`.
fn union(polygons: &mut Vec<Polygon>) {
    // convertToClipper2: outlines go in with positive KiCad area, holes negative.
    let paths: Vec<Ring> = polygons
        .iter()
        .flat_map(|polygon| {
            polygon.iter().enumerate().map(|(i, ring)| {
                let required = i == 0;
                if (signed_area(ring) >= 0.0) == required { ring.clone() } else { ring.iter().rev().copied().collect() }
            })
        })
        .collect();
    let tree = clipper::union(&paths);
    polygons.clear();
    for root in tree.roots() {
        import_poly_path(&tree, root, polygons);
    }
}

/// `SHAPE_POLY_SET::importPolyPath`: an outer node becomes a polygon with its
/// children as holes; islands inside those holes are imported (recursively)
/// before the polygon that contains them.
fn import_poly_path(tree: &clipper::Tree, node: usize, polygons: &mut Vec<Polygon>) {
    if tree.is_hole(node) {
        return;
    }
    let mut polygon = vec![append_dedup(tree.points(node).iter().copied())];
    for &child in tree.children(node) {
        polygon.push(append_dedup(tree.points(child).iter().copied()));
        for &grandchild in tree.children(child) {
            import_poly_path(tree, grandchild, polygons);
        }
    }
    polygons.push(polygon);
}

/// `SHAPE_LINE_CHAIN::Area(false)`: positive for KiCad's outline orientation.
fn signed_area(ring: &[Point]) -> f64 {
    let mut area = 0.0;
    let mut j = ring.len().wrapping_sub(1);
    for (i, &(x, y)) in ring.iter().enumerate() {
        let (xj, yj) = ring[j];
        area += (xj as f64 + x as f64) * (yj as f64 - y as f64);
        j = i;
    }
    -area * 0.5
}

/// KiCad's `rescale()`: `a * b / c` rounded to nearest, in 128-bit.
fn rescale(numerator: i64, value: i64, denominator: i64) -> i64 {
    let product = numerator as i128 * value as i128;
    let denominator = denominator as i128;
    let rounded = if (product < 0) ^ (denominator < 0) { product - denominator / 2 } else { product + denominator / 2 };
    (rounded / denominator) as i64
}

/// `SHAPE_POLY_SET::splitCollinearOutlines`: split an outline that touches
/// itself along a collinear "waist" (overlapping edges with no material on
/// either side) into two outlines.
fn split_collinear_outlines(polygons: &mut Vec<Polygon>) {
    let mut index = 0;
    while index < polygons.len() {
        while let Some((seg_a, seg_b)) = find_exterior_waist(polygons, index) {
            let outline = &polygons[index][0];
            let count = outline.len();
            let (a0, a1, b0, b1) = (seg_a, (seg_a + 1) % count, seg_b, (seg_b + 1) % count);
            let walk = |from: usize, to: usize| {
                let mut ring = vec![outline[from]];
                let mut i = from;
                while i != to {
                    i = (i + 1) % count;
                    ring.push(outline[i]);
                }
                closed_chain(&ring)
            };
            let (first, second) = (walk(a1, b0), walk(b1, a0));
            polygons[index][0] = first;
            polygons.push(vec![second]);
        }
        index += 1;
    }
}

fn find_exterior_waist(polygons: &[Polygon], index: usize) -> Option<(usize, usize)> {
    let outline = &polygons[index][0];
    let count = outline.len();
    let segment = |i: usize| (outline[i], outline[(i + 1) % count]);
    let bbox = |(a, b): (Point, Point)| ((a.0.min(b.0), a.1.min(b.1)), (a.0.max(b.0), a.1.max(b.1)));
    let overlaps = |p: ((i64, i64), (i64, i64)), q: ((i64, i64), (i64, i64))| {
        p.0.0 <= q.1.0 && q.0.0 <= p.1.0 && p.0.1 <= q.1.1 && q.0.1 <= p.1.1
    };
    for i in 0..count {
        let seg = segment(i);
        // KiCad visits candidates through an R-tree; any order finds the same
        // pair as long as one segment has at most one waist partner.
        for j in 0..count {
            if j == i || j == (i + 1) % count || j == (i + count - 1) % count || !overlaps(bbox(seg), bbox(segment(j)))
            {
                continue;
            }
            let other = segment(j);
            if other == seg || other == (seg.1, seg.0) {
                continue;
            }
            if approx_collinear(seg, other, 10) && is_exterior_waist(polygons, seg, other) {
                return Some((i, j));
            }
        }
    }
    None
}

/// `SEG::ApproxCollinear` via `SEG::mutualDistanceSquared`.
fn approx_collinear(a: (Point, Point), b: (Point, Point), threshold: i64) -> bool {
    let length = |(p, q): (Point, Point)| (q.0 - p.0) * (q.0 - p.0) + (q.1 - p.1) * (q.1 - p.1);
    let (a, b) = if length(a) < length(b) { (b, a) } else { (a, b) };
    let p = a.0.1 - a.1.1;
    let q = a.1.0 - a.0.0;
    let r = -p * a.0.0 - q * a.0.1;
    let l = p * p + q * q;
    if l == 0 {
        return false;
    }
    let distance = |(x, y): Point| {
        let det = p * x + q * y + r;
        det.signum() * rescale(det, det, l)
    };
    let limit = threshold * threshold;
    distance(b.0).abs() <= limit && distance(b.1).abs() <= limit
}

/// `SHAPE_POLY_SET::isExteriorWaist`.
fn is_exterior_waist(polygons: &[Polygon], a: (Point, Point), b: (Point, Point)) -> bool {
    let (dx, dy) = (a.1.0 - a.0.0, a.1.1 - a.0.1);
    let mut points = [a.0, a.1, b.0, b.1];
    if dx.abs() >= dy.abs() {
        points.sort_by_key(|&(x, y)| (x, y));
    } else {
        points.sort_by_key(|&(x, y)| (y, x));
    }
    let (s, e) = (points[1], points[2]);
    let mid = ((s.0 + e.0) / 2, (s.1 + e.1) / 2);
    let dir = (e.0 - s.0, e.1 - s.1);
    if euclidean_norm(dir) <= 25 {
        return false;
    }
    let perp = resize((-dir.1, dir.0), 10);
    let inside = |pt: Point| polygons.iter().any(|polygon| point_inside(&polygon[0], pt));
    !inside((mid.0 + perp.0, mid.1 + perp.1)) && !inside((mid.0 - perp.0, mid.1 - perp.1))
}

/// `VECTOR2I::EuclideanNorm`.
fn euclidean_norm((x, y): Point) -> i64 {
    if x.abs() == y.abs() {
        (x.abs() as f64 * std::f64::consts::SQRT_2).round() as i64
    } else if x == 0 {
        y.abs()
    } else if y == 0 {
        x.abs()
    } else {
        (x as f64).hypot(y as f64).round() as i64
    }
}

/// `VECTOR2I::Resize`.
fn resize((x, y): Point, length: i64) -> Point {
    if x == 0 && y == 0 {
        return (0, 0);
    }
    let (nx, ny) = if x.abs() == y.abs() {
        let v = length.abs() as f64 * std::f64::consts::FRAC_1_SQRT_2;
        (v, v)
    } else {
        let (xs, ys) = (x * x, y * y);
        let (ls, ns) = (xs + ys, length * length);
        ((rescale(ns, xs, ls) as f64).sqrt(), (rescale(ns, ys, ls) as f64).sqrt())
    };
    let signed = |v: f64, neg: bool| if neg { -(v.round() as i64) } else { v.round() as i64 };
    let sign = length.signum();
    (signed(nx, x < 0) * sign, signed(ny, y < 0) * sign)
}

/// `SHAPE_LINE_CHAIN_BASE::PointInside` with zero accuracy (ray casting).
fn point_inside(ring: &[Point], (px, py): Point) -> bool {
    if ring.len() < 3 {
        return false;
    }
    let mut inside = false;
    for i in 0..ring.len() {
        let (p1, p2) = (ring[i], ring[(i + 1) % ring.len()]);
        let (dx, dy) = (p2.0 - p1.0, p2.1 - p1.1);
        if dy == 0 {
            continue;
        }
        let d = rescale(dx, py - p1.1, dy);
        if ((p1.1 >= py) != (p2.1 >= py)) && (px - p1.0 < d) {
            inside = !inside;
        }
    }
    inside
}

#[derive(Clone, Copy, Default)]
struct Edge {
    p1: Point,
    p2: Point,
    next: usize,
}

impl Edge {
    fn matches(&self, y: i64) -> bool {
        (y >= self.p1.1 || y >= self.p2.1) && (y <= self.p1.1 || y <= self.p2.1)
    }
}

/// `fractureSingleCacheFriendly`: bridge every hole, leftmost first, to the
/// nearest edge on its left with a horizontal cut.
fn fracture_single(polygon: Polygon) -> Ring {
    if polygon.len() == 1 {
        return polygon.into_iter().next().unwrap_or_default();
    }

    struct PathInfo {
        index: usize,
        leftmost: usize,
        x: i64,
        y: i64,
        bridge: usize,
    }
    let mut sorted: Vec<PathInfo> = polygon
        .iter()
        .enumerate()
        .map(|(index, ring)| {
            let (mut x_min, mut y_min, mut leftmost) = (i64::MAX, i64::MAX, 0);
            for (i, &(x, y)) in ring.iter().enumerate() {
                if x < x_min {
                    x_min = x;
                    leftmost = i;
                }
                y_min = y_min.min(y);
            }
            PathInfo { index, leftmost, x: x_min, y: y_min, bridge: 0 }
        })
        .collect();
    sorted[1..].sort_by_key(|info| (info.x, info.y));

    let mut edges: Vec<Edge> = Vec::with_capacity(polygon.iter().map(Vec::len).sum::<usize>() + polygon.len() * 3);
    for (n, info) in sorted.iter_mut().enumerate() {
        let ring = &polygon[info.index];
        let provoking = edges.len();
        for i in 0..ring.len() {
            let next = if i + 1 == ring.len() { provoking } else { edges.len() + 1 };
            edges.push(Edge { p1: ring[i], p2: ring[(i + 1) % ring.len()], next });
        }
        if n > 0 {
            info.index = provoking;
            info.bridge = edges.len();
            edges.resize(edges.len() + 3, Edge::default());
        }
    }

    for info in &sorted[1..] {
        if !process_hole(&mut edges, info.index, info.index + info.leftmost, info.bridge) {
            // KiCad logs "Broken polygon, dropping path" and keeps the
            // unfractured polygon; its saved outline is the original one.
            return polygon.into_iter().next().unwrap_or_default();
        }
    }

    let mut ring = Vec::new();
    let mut e = 0;
    loop {
        if ring.last() != Some(&edges[e].p1) {
            ring.push(edges[e].p1);
        }
        e = edges[e].next;
        if e == 0 {
            break;
        }
    }
    ring
}

/// `processHole`: connect the hole starting at `edge_index` to the nearest
/// edge to its left among those already merged (indices below `provoking`).
fn process_hole(edges: &mut [Edge], provoking: usize, edge_index: usize, bridge: usize) -> bool {
    let (x, y) = edges[edge_index].p1;
    let mut min_dist = i64::from(i32::MAX);
    let mut nearest: Option<(usize, i64)> = None;
    for (i, e) in edges[..provoking].iter().enumerate() {
        if !e.matches(y) {
            continue;
        }
        let x_intersect = if e.p1.1 == e.p2.1 {
            e.p1.0.max(e.p2.0)
        } else {
            e.p1.0 + rescale(e.p2.0 - e.p1.0, y - e.p1.1, e.p2.1 - e.p1.1)
        };
        let dist = x - x_intersect;
        if dist >= 0 && dist < min_dist {
            min_dist = dist;
            nearest = Some((i, x_intersect));
        }
    }
    let Some((near, x_nearest)) = nearest else { return false };
    let (outline_to_hole, hole_to_outline, split) = (bridge, bridge + 1, bridge + 2);
    let hole_start = edges[edge_index].p1;
    edges[outline_to_hole] = Edge { p1: (x_nearest, y), p2: hole_start, next: edge_index };
    edges[hole_to_outline] = Edge { p1: hole_start, p2: (x_nearest, y), next: split };
    edges[split] = Edge { p1: (x_nearest, y), p2: edges[near].p2, next: edges[near].next };
    edges[near].p2 = (x_nearest, y);
    edges[near].next = outline_to_hole;
    let mut last = edge_index;
    while edges[last].next != edge_index {
        last = edges[last].next;
    }
    edges[last].next = hole_to_outline;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Case {
        name: String,
        outline: Ring,
        holes: Vec<Ring>,
        expected: Vec<Ring>,
    }

    /// Reference outputs from KiCad 10.0.5's `SHAPE_POLY_SET::Fracture(true)`
    /// for synthetic squares and a sample of the Balance Bot logo shapes.
    #[test]
    fn matches_kicad() {
        let cases: Vec<Case> = serde_json::from_str(include_str!("kicad_fracture_cases.json")).unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(kicad_fracture(&case.outline, &case.holes), case.expected, "{}", case.name);
        }
    }

    #[test]
    fn square_with_hole_becomes_one_bridged_outline() {
        let square = vec![(0, 0), (10, 0), (10, 10), (0, 10)];
        let hole = vec![(3, 3), (3, 7), (7, 7), (7, 3)];
        let out = kicad_fracture(&square, std::slice::from_ref(&hole));
        assert_eq!(out.len(), 1);
        // Every original vertex survives, plus the bridge's two endpoints
        // (the hole vertex appears twice: into and out of the hole).
        assert!(square.iter().chain(&hole).all(|p| out[0].contains(p)));
        assert_eq!(out[0].len(), square.len() + hole.len() + 3);
    }

    #[test]
    fn hole_free_shapes_pass_through() {
        let tri = vec![(0, 0), (5, 0), (0, 5), (0, 0)];
        assert_eq!(kicad_fracture(&tri, &[]), vec![vec![(0, 0), (5, 0), (0, 5)]]);
    }
}
