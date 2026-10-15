//! Integer geometry helpers that reproduce KiCad's own arithmetic.
//!
//! The DSN exporter must place every vertex exactly where pcbnew would, so
//! these functions mirror the corresponding KiCad routines operation for
//! operation (`KiROUND`, `RotatePoint`, `EDA_ANGLE::Sin/Cos`, arc and
//! rounded-rectangle polygonisation).

use std::f64::consts::{FRAC_1_SQRT_2, PI};

/// A point in KiCad internal units (nanometres).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct P {
    pub x: i64,
    pub y: i64,
}

impl P {
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }
}

impl std::ops::Add for P {
    type Output = P;
    fn add(self, o: P) -> P {
        P::new(self.x + o.x, self.y + o.y)
    }
}

impl std::ops::Sub for P {
    type Output = P;
    fn sub(self, o: P) -> P {
        P::new(self.x - o.x, self.y - o.y)
    }
}

/// `KiROUND`: `std::llround`, i.e. round half away from zero.
pub fn kiround(v: f64) -> i64 {
    v.round() as i64
}

/// `EDA_ANGLE::Normalize`: degrees into `[0, 360)`.
pub fn normalize_deg(mut deg: f64) -> f64 {
    while deg < -0.0 {
        deg += 360.0;
    }
    while deg >= 360.0 {
        deg -= 360.0;
    }
    deg
}

/// `EDA_ANGLE::Sin`, with KiCad's exact values at multiples of 45°.
pub fn sin_deg(deg: f64) -> f64 {
    let t = normalize_deg(deg);
    if t == 0.0 || t == 180.0 {
        0.0
    } else if t == 45.0 || t == 135.0 {
        FRAC_1_SQRT_2
    } else if t == 225.0 || t == 315.0 {
        -FRAC_1_SQRT_2
    } else if t == 90.0 {
        1.0
    } else if t == 270.0 {
        -1.0
    } else {
        (deg * (PI / 180.0)).sin()
    }
}

/// `EDA_ANGLE::Cos`, with KiCad's exact values at multiples of 45°.
pub fn cos_deg(deg: f64) -> f64 {
    let t = normalize_deg(deg);
    if t == 0.0 {
        1.0
    } else if t == 180.0 {
        -1.0
    } else if t == 90.0 || t == 270.0 {
        0.0
    } else if t == 45.0 || t == 315.0 {
        FRAC_1_SQRT_2
    } else if t == 135.0 || t == 225.0 {
        -FRAC_1_SQRT_2
    } else {
        (deg * (PI / 180.0)).cos()
    }
}

/// `RotatePoint( int*, int*, EDA_ANGLE )`: counter-clockwise on screen.
pub fn rotate(p: P, deg: f64) -> P {
    let a = normalize_deg(deg);
    if a == 0.0 {
        p
    } else if a == 90.0 {
        P::new(p.y, -p.x)
    } else if a == 180.0 {
        P::new(-p.x, -p.y)
    } else if a == 270.0 {
        P::new(-p.y, p.x)
    } else {
        let (s, c) = (sin_deg(a), cos_deg(a));
        let (x, y) = (p.x as f64, p.y as f64);
        P::new(kiround(y * s + x * c), kiround(y * c - x * s))
    }
}

/// `GetArcToSegmentCount`.
pub fn arc_to_segment_count(radius: i64, error_max: i64, arc_deg: f64) -> i64 {
    let radius = radius.max(1) as f64;
    let error_max = error_max.max(1) as f64;
    let rel_error = error_max / radius;
    let mut arc_increment = 180.0 / PI * (1.0 - rel_error).acos() * 2.0;
    // MIN_SEGCOUNT_FOR_CIRCLE is 8.
    arc_increment = arc_increment.min(360.0 / 8.0);
    let seg_count = kiround(arc_deg.abs() / arc_increment);
    seg_count.max(2)
}

/// `CircleToEndSegmentDeltaRadius`.
pub fn circle_to_end_segment_delta_radius(radius: i64, seg_count: i64) -> i64 {
    let seg_count = if seg_count <= 2 { 3 } else { seg_count };
    let alpha = PI / seg_count as f64;
    kiround((radius as f64 * (1.0 - 1.0 / alpha.cos())).abs())
}

/// KiCad's 64-bit `rescale`: `a * b / c` rounded half away from zero.
pub fn rescale(a: i64, b: i64, c: i64) -> i64 {
    let sign = a.signum() * b.signum() * c.signum();
    let num = a.unsigned_abs() as u128 * b.unsigned_abs() as u128;
    let den = c.unsigned_abs() as u128;
    if den == 0 {
        return 0;
    }
    sign * ((num + den / 2) / den) as i64
}

/// `VECTOR2I::Resize` for integer vectors.
pub fn resize(v: P, new_len: i64) -> P {
    if v.x == 0 && v.y == 0 {
        return P::new(0, 0);
    }
    let (nx, ny) = if v.x.abs() == v.y.abs() {
        let n = new_len.abs() as f64 * FRAC_1_SQRT_2;
        (n, n)
    } else {
        let x_sq = v.x as i128 * v.x as i128;
        let y_sq = v.y as i128 * v.y as i128;
        let l_sq = x_sq + y_sq;
        let n_sq = new_len as i128 * new_len as i128;
        let rescale128 = |value: i128| -> f64 {
            // rescale(newLength_sq, value, l_sq) with half-up rounding.
            let num = n_sq * value;
            ((num + l_sq / 2) / l_sq) as f64
        };
        (rescale128(x_sq).sqrt(), rescale128(y_sq).sqrt())
    };
    let sx = if v.x < 0 { -kiround(nx) } else { kiround(nx) };
    let sy = if v.y < 0 { -kiround(ny) } else { kiround(ny) };
    let s = new_len.signum();
    P::new(sx * s, sy * s)
}

/// `SEG( a, b ).ReflectPoint( p )`.
pub fn reflect_point(a: P, b: P, p: P) -> P {
    let d = b - a;
    let l_squared = d.x * d.x + d.y * d.y;
    if l_squared == 0 {
        return P::new(2 * p.x - p.x, 2 * p.y - p.y);
    }
    let t = d.x * (p.x - a.x) + d.y * (p.y - a.y);
    let cx = a.x + rescale(t, d.x, l_squared);
    let cy = a.y + rescale(t, d.y, l_squared);
    P::new(2 * cx - p.x, 2 * cy - p.y)
}

/// `ConvertArcToPolyline( chain, center, radius, start, arc, accuracy, ERROR_INSIDE )`.
pub fn arc_polyline_inside(center: P, radius: i64, start_deg: f64, arc_deg: f64, accuracy: i64) -> Vec<P> {
    let mut n = 2;
    if radius >= accuracy {
        n = arc_to_segment_count(radius, accuracy, arc_deg) + 1;
    }
    let delta = arc_deg / n as f64;
    let mut rot = start_deg;
    let mut out = Vec::with_capacity(n as usize + 1);
    for _ in 0..=n {
        let x = center.x as f64 + radius as f64 * cos_deg(rot);
        let y = center.y as f64 + radius as f64 * sin_deg(rot);
        out.push(P::new(kiround(x), kiround(y)));
        rot += delta;
    }
    out
}

/// `SHAPE_ARC( center, start, ANGLE_360 ).ConvertToPolyline( maxError )` for a
/// full circle: endpoints on the circle, intermediate vertices at
/// `r + error / 2`, exactly as courtyard and outline circles are polygonised.
pub fn circle_polyline_split_error(center: P, radius: i64, max_error: i64) -> Vec<P> {
    let start = P::new(center.x + radius, center.y);
    let r = radius as f64;
    // m_width is 0, so the external radius is the radius itself (truncated to int).
    let external = r as i64;
    let half_max_error = (max_error as f64 / 2.0).max(1.0);
    let mut out = vec![start];
    if (external as f64) < half_max_error {
        out.push(start);
        return out;
    }
    let n = arc_to_segment_count(external, max_error, 360.0);
    let seg360 = (n as f64 * 360.0 / 360.0) as i64;
    let effective_error = circle_to_end_segment_delta_radius(external, seg360) as f64;
    let r = r + effective_error / 2.0;
    let n = n * 2;
    let mut i = 1;
    while i < n {
        // EDA_ANGLE arithmetic: sa (0) + (ca * i) / n, in degrees.
        let a = 0.0 + (360.0 * i as f64) / n as f64;
        let x = center.x as f64 + r * cos_deg(a);
        let y = center.y as f64 + r * sin_deg(a);
        out.push(P::new(kiround(x), kiround(y)));
        i += 2;
    }
    out.push(start);
    out
}

struct Corner {
    pos: P,
    radius: i64,
}

/// `TransformRoundChamferedRectToPolygon` with no inflation, zero rotation,
/// centred at the origin and `ERROR_INSIDE`, as the DSN exporter calls it.
pub fn round_chamfered_rect(
    size: P,
    corner_radius: i64,
    chamfer_ratio: f64,
    chamfer_corners: u32,
    error: i64,
) -> Vec<P> {
    let half = P::new(size.x / 2, size.y / 2);
    let mut corners = vec![
        Corner { pos: P::new(-half.x, -half.y), radius: corner_radius },
        Corner { pos: P::new(half.x, -half.y), radius: corner_radius },
        Corner { pos: P::new(half.x, half.y), radius: corner_radius },
        Corner { pos: P::new(-half.x, half.y), radius: corner_radius },
    ];
    if chamfer_corners != 0 {
        let shorter = size.x.min(size.y);
        let chamfer = kiround(chamfer_ratio * shorter as f64).max(0);
        // RECT_CHAMFER_TOP_LEFT, TOP_RIGHT, BOTTOM_RIGHT, BOTTOM_LEFT.
        let ids = [1u32, 2, 8, 4];
        let sign = [0i64, 1, -1, 0, 0, -1, 1, 0];
        let mut pos = 0usize;
        for (cc, id) in ids.iter().enumerate() {
            if chamfer_corners & id != 0 {
                corners[pos].radius = 0;
                if chamfer != 0 {
                    let base = corners[pos].pos;
                    corners.insert(pos + 1, Corner { pos: base, radius: 0 });
                    let cc = cc as i64;
                    corners[pos].pos.x += sign[((2 * cc) & 7) as usize] * chamfer;
                    corners[pos].pos.y += sign[((2 * cc - 2) & 7) as usize] * chamfer;
                    corners[pos + 1].pos.x += sign[((2 * cc + 1) & 7) as usize] * chamfer;
                    corners[pos + 1].pos.y += sign[((2 * cc - 1) & 7) as usize] * chamfer;
                    pos += 1;
                }
            }
            pos += 1;
        }
        let chamfer_count = chamfer_corners.count_ones();
        if chamfer_count > 1 && 2 * chamfer >= shorter {
            let mut prev = corners[0].pos;
            let mut i = corners.len();
            while i > 0 {
                i -= 1;
                if corners[i].pos == prev {
                    corners.remove(i);
                } else {
                    prev = corners[i].pos;
                }
            }
        }
    }
    corner_list_to_polygon(&corners, error)
}

/// `CornerListToPolygon` for `aInflate == 0` and `ERROR_INSIDE`.
fn corner_list_to_polygon(corners: &[Corner], error: i64) -> Vec<P> {
    let mut out = Vec::new();
    let count = corners.len();
    let mut incoming = corners[0].pos - corners[count - 1].pos;
    for n in 0..count {
        let cur = &corners[n];
        let next = &corners[(n + 1) % count];
        let outgoing = next.pos - cur.pos;
        if cur.radius == 0 {
            out.push(cur.pos);
        } else {
            let corner_position = cur.pos;
            let radius = cur.radius;
            let (end_angle, tan_angle2) =
                if (incoming.x == 0 && outgoing.y == 0) || (incoming.y == 0 && outgoing.x == 0) {
                    (90.0, 1.0)
                } else {
                    let cos_num = incoming.x as f64 * outgoing.x as f64 + incoming.y as f64 * outgoing.y as f64;
                    let norm = |v: P| ((v.x as f64).powi(2) + (v.y as f64).powi(2)).sqrt();
                    let angle = (cos_num / (norm(incoming) * norm(outgoing))).acos();
                    (angle * 180.0 / PI, ((PI - angle) / 2.0).tan())
                };
            let num_segs = 16.max(arc_to_segment_count(radius, error, 360.0));
            let ang_delta = 360.0 / num_segs as f64;
            let mut last_seg = end_angle;
            if last_seg > 0.0 {
                while last_seg > ang_delta {
                    last_seg -= ang_delta;
                }
            } else {
                while last_seg < -ang_delta {
                    last_seg += ang_delta;
                }
            }
            let mut ang_pos = if last_seg == 0.0 { ang_delta } else { (ang_delta + last_seg) / 2.0 };
            let arc_transition = if tan_angle2 > 0.0 { radius as f64 / tan_angle2 } else { 0.0 };
            let perp = |v: P| P::new(-v.y, v.x);
            let arc_start = corner_position - resize(incoming, arc_transition as i64);
            let arc_center = arc_start + resize(perp(incoming), radius);
            let arc_end = reflect_point(corner_position, arc_center, arc_start);
            let arc_start_origin = arc_start - arc_center;
            out.push(arc_start);
            while ang_pos < end_angle {
                let pt = rotate(arc_start_origin, -ang_pos);
                out.push(pt + arc_center);
                ang_pos += ang_delta;
            }
            out.push(arc_end);
        }
        incoming = outgoing;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_counts_match_kicad() {
        assert_eq!(arc_to_segment_count(1_250_000, 5_000, 360.0), 35);
        assert_eq!(arc_to_segment_count(3_450_000, 5_000, 360.0), 58);
        assert_eq!(arc_to_segment_count(1_000_000, 5_000, 360.0), 31);
    }

    #[test]
    fn right_angles_are_exact() {
        assert_eq!(rotate(P::new(-825_000, 0), 270.0), P::new(0, -825_000));
        assert_eq!(rotate(P::new(3, 4), 90.0), P::new(4, -3));
    }
}
