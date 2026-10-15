//! `cv::approxPolyDP` for closed integer contours, bit-exact with OpenCV 4.8
//! (`approxPolyDP_`): Douglas–Peucker seeded from two approximately
//! farthest points, then a clean-up pass over near-collinear vertices.

use super::contours::Point;

/// Simplify a closed contour so no point strays more than `epsilon` from it.
pub fn approx_closed(src: &[Point], epsilon: f64) -> Vec<Point> {
    let count = src.len();
    if count == 0 {
        return Vec::new();
    }
    let eps = epsilon * epsilon;
    let wrap = |pos: usize| if pos + 1 >= count { 0 } else { pos + 1 };
    let mut dst: Vec<Point> = Vec::with_capacity(count);
    let mut stack: Vec<(usize, usize)> = Vec::new();

    // 1. Find approximately the two farthest points (three refinement passes).
    let mut pos = 0;
    let mut right_start = 0;
    let mut start_pt = src[0];
    let mut le_eps = false;
    for _ in 0..3 {
        pos = (pos + right_start) % count;
        start_pt = src[pos];
        pos = wrap(pos);
        let mut max_dist = 0.0;
        for j in 1..count {
            let pt = src[pos];
            pos = wrap(pos);
            let (dx, dy) = (f64::from(pt.0 - start_pt.0), f64::from(pt.1 - start_pt.1));
            let dist = dx * dx + dy * dy;
            if dist > max_dist {
                max_dist = dist;
                right_start = j;
            }
        }
        le_eps = max_dist <= eps;
    }

    // 2. Seed the stack with the two halves between them.
    if le_eps {
        dst.push(start_pt);
    } else {
        let slice_start = pos % count;
        let slice_end = (right_start + slice_start) % count;
        stack.push((slice_end, slice_start));
        stack.push((slice_start, slice_end));
    }

    // 3. Split each slice at its farthest point until all are within epsilon.
    while let Some((start, end)) = stack.pop() {
        let end_pt = src[end];
        let mut pos = start;
        let mut start_pt = src[pos];
        pos = wrap(pos);
        let mut split = 0;
        let within = if pos != end {
            let (dx, dy) = (f64::from(end_pt.0 - start_pt.0), f64::from(end_pt.1 - start_pt.1));
            let mut max_dist = 0.0;
            while pos != end {
                let pt = src[pos];
                pos = wrap(pos);
                let dist = (f64::from(pt.1 - start_pt.1) * dx - f64::from(pt.0 - start_pt.0) * dy).abs();
                if dist > max_dist {
                    max_dist = dist;
                    split = (pos + count - 1) % count;
                }
            }
            max_dist * max_dist <= eps * (dx * dx + dy * dy)
        } else {
            start_pt = src[start];
            true
        };
        if within {
            dst.push(start_pt);
        } else {
            stack.push((split, end));
            stack.push((start, split));
        }
    }

    cleanup(dst, eps)
}

/// Drop vertices that sit (almost) on the line through their neighbours.
fn cleanup(mut dst: Vec<Point>, eps: f64) -> Vec<Point> {
    let count = dst.len();
    let mut new_count = count;
    let wrap = |pos: usize| if pos + 1 >= count { 0 } else { pos + 1 };
    let mut pos = count - 1;
    let mut start_pt = dst[pos];
    pos = wrap(pos);
    let mut wpos = pos;
    let mut pt = dst[pos];
    pos = wrap(pos);
    let mut i = 0;
    while i < count && new_count > 2 {
        let end_pt = dst[pos];
        pos = wrap(pos);
        let (dx, dy) = (f64::from(end_pt.0 - start_pt.0), f64::from(end_pt.1 - start_pt.1));
        let dist = (f64::from(pt.0 - start_pt.0) * dy - f64::from(pt.1 - start_pt.1) * dx).abs();
        let inner = f64::from(pt.0 - start_pt.0) * f64::from(end_pt.0 - pt.0)
            + f64::from(pt.1 - start_pt.1) * f64::from(end_pt.1 - pt.1);
        if dist * dist <= 0.5 * eps * (dx * dx + dy * dy) && dx != 0.0 && dy != 0.0 && inner >= 0.0 {
            new_count -= 1;
            start_pt = end_pt;
            dst[wpos] = end_pt;
            wpos = wrap(wpos);
            pt = dst[pos];
            pos = wrap(pos);
            i += 2;
            continue;
        }
        start_pt = pt;
        dst[wpos] = pt;
        wpos = wrap(wpos);
        pt = end_pt;
        i += 1;
    }
    dst.truncate(new_count);
    dst
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_a_square_and_drops_collinear_points() {
        let square = [(0, 0), (0, 5), (0, 10), (10, 10), (10, 0), (5, 0)];
        let simplified = approx_closed(&square, 0.65);
        assert_eq!(simplified.len(), 4);
        for corner in [(0, 0), (0, 10), (10, 10), (10, 0)] {
            assert!(simplified.contains(&corner));
        }
    }

    #[test]
    fn tiny_contour_is_kept_as_is() {
        // cv2.approxPolyDP keeps both points of a two-point closed contour.
        assert_eq!(approx_closed(&[(3, 3), (3, 4)], 0.65), vec![(3, 3), (3, 4)]);
    }
}
