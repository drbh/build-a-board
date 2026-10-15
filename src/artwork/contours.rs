//! `cv::findContours(RETR_CCOMP, CHAIN_APPROX_SIMPLE)`, bit-exact with
//! OpenCV 4.8's Suzuki–Abe border follower (`cvFindNextContour` and
//! `icvFetchContourEx`), including its contour numbering, parent search,
//! and depth-first output order.

use super::raster::Gray;

pub type Point = (i32, i32);

/// One traced border with its two-level `RETR_CCOMP` links (indices into
/// the returned list, `None` for OpenCV's `-1`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contour {
    pub points: Vec<Point>,
    pub next: Option<usize>,
    pub prev: Option<usize>,
    pub first_child: Option<usize>,
    pub parent: Option<usize>,
}

/// Chain-code neighbour offsets, counter-clockwise from east.
const CODE_DELTAS: [Point; 8] = [(1, 0), (1, -1), (0, -1), (-1, -1), (-1, 0), (-1, 1), (0, 1), (1, 1)];
const MAX_SIZE: usize = 16;

/// Contour bookkeeping during the scan (`_CvContourInfo`).
struct Info {
    is_hole: bool,
    /// Start pixel, in padded-image coordinates.
    origin: (usize, usize),
    /// `(x, y, width, height)` in padded-image coordinates.
    rect: (i32, i32, i32, i32),
    /// `None` is the image frame.
    parent: Option<usize>,
    /// Next contour sharing the same 7-bit border label.
    next_same_label: Option<usize>,
    points: Vec<Point>,
}

/// The border follower's view of the padded, 0/1-binarised image.
struct Scan {
    img: Vec<i8>,
    step: isize,
    deltas: [isize; MAX_SIZE],
}

impl Scan {
    fn new(src: &Gray) -> Self {
        // findContours pads with a one-pixel zero border so borders that touch
        // the image edge are followed, then works on a 0/1 image.
        let step = src.width + 2;
        let mut img = vec![0i8; step * (src.height + 2)];
        for y in 0..src.height {
            for x in 0..src.width {
                img[(y + 1) * step + x + 1] = i8::from(src.get(x, y) != 0);
            }
        }
        let s = step as isize;
        let base = [1, -s + 1, -s, -s - 1, -1, s - 1, s, s + 1];
        let mut deltas = [0; MAX_SIZE];
        deltas[..8].copy_from_slice(&base);
        deltas[8..].copy_from_slice(&base);
        Self { img, step: s, deltas }
    }

    fn at(&self, index: isize) -> i8 {
        self.img[index as usize]
    }

    /// `icvFetchContourEx` with `CHAIN_APPROX_SIMPLE`: follow the border from
    /// `start`, label it `nbd`, and return its corner points (unpadded
    /// coordinates) and padded bounding rect.
    fn fetch(&mut self, start: isize, mut pt: Point, is_hole: bool, nbd: i8) -> (Vec<Point>, (i32, i32, i32, i32)) {
        let mut points = Vec::new();
        let (mut rx, mut ry, mut rw, mut rh) = (pt.0, pt.1, pt.0, pt.1);
        let i0 = start;
        let mut s: usize = if is_hole { 0 } else { 4 };
        let s_end = s;
        let mut i1;
        loop {
            s = (s + 7) & 7;
            i1 = i0 + self.deltas[s];
            if self.at(i1) != 0 || s == s_end {
                break;
            }
        }
        if s == s_end {
            // Single-pixel component.
            self.img[i0 as usize] = nbd | i8::MIN;
            points.push(pt);
        } else {
            let mut i3 = i0;
            let mut prev_s = s ^ 4;
            loop {
                let s_end = s;
                s = s.min(MAX_SIZE - 1);
                let mut i4 = i3;
                while s < MAX_SIZE - 1 {
                    s += 1;
                    i4 = i3 + self.deltas[s];
                    if self.at(i4) != 0 {
                        break;
                    }
                }
                s &= 7;
                // Mark the pixel as a right-hand border or a plain border pixel.
                if s.wrapping_sub(1) < s_end {
                    self.img[i3 as usize] = nbd | i8::MIN;
                } else if self.at(i3) == 1 {
                    self.img[i3 as usize] = nbd;
                }
                if s != prev_s {
                    points.push(pt);
                    if pt.0 < rx {
                        rx = pt.0;
                    } else if pt.0 > rw {
                        rw = pt.0;
                    }
                    if pt.1 < ry {
                        ry = pt.1;
                    } else if pt.1 > rh {
                        rh = pt.1;
                    }
                }
                prev_s = s;
                pt.0 += CODE_DELTAS[s].0;
                pt.1 += CODE_DELTAS[s].1;
                if i4 == i0 && i3 == i1 {
                    break;
                }
                i3 = i4;
                s = (s + 4) & 7;
            }
        }
        // findContours scans with a (-1, -1) offset; the rect goes back to padded space.
        ((points), (rx + 1, ry + 1, rw - rx + 1, rh - ry + 1))
    }

    /// `icvTraceContour`: does the border starting at `start` pass through `stop`?
    fn passes_through(&self, start: isize, stop: isize, is_hole: bool) -> bool {
        let i0 = start;
        let mut s: usize = if is_hole { 0 } else { 4 };
        let s_end = s;
        let mut i1;
        loop {
            s = (s + 7) & 7;
            i1 = i0 + self.deltas[s];
            if self.at(i1) != 0 || s == s_end {
                break;
            }
        }
        let mut i3 = i0;
        if s != s_end {
            loop {
                s = s.min(MAX_SIZE - 1);
                let mut i4 = i3;
                while s < MAX_SIZE - 1 {
                    s += 1;
                    i4 = i3 + self.deltas[s];
                    if self.at(i4) != 0 {
                        break;
                    }
                }
                if i3 == stop || (i4 == i0 && i3 == i1) {
                    break;
                }
                i3 = i4;
                s = (s + 4) & 7;
            }
        }
        i3 == stop
    }
}

/// Find all borders of the non-zero regions of `src`.
pub fn find_contours(src: &Gray) -> Vec<Contour> {
    let mut scan = Scan::new(src);
    let (width, height) = (src.width + 1, src.height + 1);
    let step = scan.step;
    let mut infos: Vec<Info> = Vec::new();
    let mut by_label: [Option<usize>; 128] = [None; 128];
    let mut nbd: i8 = 2;

    for y in 1..height {
        let row = y as isize * step;
        let mut lnbd_x: usize = 0;
        let lnbd_y = y;
        let mut prev: i8 = 0;
        let mut x = 1;
        while x < width {
            let p = scan.at(row + x as isize);
            if p != prev {
                let outer_start = prev == 0 && p == 1;
                let hole_start = !outer_start && p == 0 && prev >= 1;
                if outer_start || hole_start {
                    let is_hole = hole_start;
                    if is_hole && (prev & -2) != 0 {
                        lnbd_x = x - 1;
                    }
                    let parent = if !is_hole || lnbd_x == 0 {
                        None
                    } else {
                        find_parent(&scan, &infos, &by_label, (lnbd_x, lnbd_y), is_hole)
                    };
                    let start_x = x - usize::from(is_hole);
                    lnbd_x = start_x;
                    let label = nbd;
                    nbd = ((nbd as i32 + 1) & 127) as i8;
                    if nbd == 0 {
                        nbd = 3;
                    }
                    let origin_pt = (start_x as i32 - 1, y as i32 - 1);
                    let (points, rect) = scan.fetch(row + start_x as isize, origin_pt, is_hole, label);
                    let index = infos.len();
                    infos.push(Info {
                        is_hole,
                        origin: (start_x, y),
                        rect,
                        parent,
                        next_same_label: by_label[label as usize],
                        points,
                    });
                    by_label[label as usize] = Some(index);
                    // The scan resumes after the start pixel, re-reading the
                    // (now labelled) pixel just left of the resume point.
                    x += 1;
                    prev = scan.at(row + x as isize - 1);
                    continue;
                }
                prev = p;
                if (prev & -2) != 0 {
                    lnbd_x = x;
                }
            }
            x += 1;
        }
    }
    build_tree(infos)
}

/// The parent search for a hole starting right of `lnbd`, as in
/// `cvFindNextContour`: among contours carrying `lnbd`'s label whose rect
/// contains it, take the one whose border passes through `lnbd`.
fn find_parent(
    scan: &Scan,
    infos: &[Info],
    by_label: &[Option<usize>; 128],
    (lx, ly): (usize, usize),
    is_hole: bool,
) -> Option<usize> {
    let lnbd = ly as isize * scan.step + lx as isize;
    let label = (scan.at(lnbd) as u8 & 0x7f) as usize;
    let mut found: Option<usize> = None;
    let mut cursor = by_label[label];
    while let Some(index) = cursor {
        let info = &infos[index];
        let (rx, ry, rw, rh) = info.rect;
        if ((lx as i32 - rx) as u32) < rw as u32 && ((ly as i32 - ry) as u32) < rh as u32 {
            if let Some(candidate) = found {
                let c = &infos[candidate];
                let start = c.origin.1 as isize * scan.step + c.origin.0 as isize;
                if scan.passes_through(start, lnbd, c.is_hole) {
                    break;
                }
            }
            found = Some(index);
        }
        cursor = info.next_same_label;
    }
    let found = found.expect("hole border without an enclosing contour");
    // A hole's parent must be an outer border; skip over same-kind contours.
    if infos[found].is_hole == is_hole { infos[found].parent } else { Some(found) }
}

/// Link contours the way `cvInsertNodeIntoTree` does (each new contour
/// becomes its parent's first child) and emit them in the depth-first order
/// of `cvTreeToNodeSeq`.
fn build_tree(infos: Vec<Info>) -> Vec<Contour> {
    let n = infos.len();
    let mut first_child: Vec<Option<usize>> = vec![None; n];
    let mut next: Vec<Option<usize>> = vec![None; n];
    let mut prev: Vec<Option<usize>> = vec![None; n];
    let mut roots: Option<usize> = None;
    for (index, info) in infos.iter().enumerate() {
        let head = match info.parent {
            Some(parent) => &mut first_child[parent],
            None => &mut roots,
        };
        next[index] = *head;
        if let Some(old) = *head {
            prev[old] = Some(index);
        }
        *head = Some(index);
    }

    let mut order = Vec::with_capacity(n);
    let mut stack: Vec<usize> = Vec::new();
    let mut cursor = roots;
    while let Some(node) = cursor {
        order.push(node);
        if let Some(child) = first_child[node] {
            stack.push(node);
            cursor = Some(child);
            continue;
        }
        let mut climb = Some(node);
        cursor = None;
        while let Some(at) = climb {
            if let Some(sibling) = next[at] {
                cursor = Some(sibling);
                break;
            }
            climb = stack.pop();
        }
    }

    let mut position = vec![0; n];
    for (out, &node) in order.iter().enumerate() {
        position[node] = out;
    }
    let mut infos: Vec<Option<Info>> = infos.into_iter().map(Some).collect();
    order
        .iter()
        .map(|&node| {
            let info = infos[node].take().expect("each contour emitted once");
            Contour {
                points: info.points,
                next: next[node].map(|i| position[i]),
                prev: prev[node].map(|i| position[i]),
                first_child: first_child[node].map(|i| position[i]),
                parent: info.parent.map(|i| position[i]),
            }
        })
        .collect()
}

/// `cv::contourArea(contour, oriented=false)`.
pub fn area(points: &[Point]) -> f64 {
    let Some(&last) = points.last() else { return 0.0 };
    let mut prev = last;
    let mut sum = 0.0;
    for &p in points {
        sum += f64::from(prev.0 as f32) * f64::from(p.1 as f32) - f64::from(prev.1 as f32) * f64::from(p.0 as f32);
        prev = p;
    }
    (sum * 0.5).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(rows: &[&str]) -> Gray {
        let mut img = Gray::new(rows[0].len(), rows.len());
        for (y, row) in rows.iter().enumerate() {
            for (x, c) in row.bytes().enumerate() {
                img.data[y * img.width + x] = if c == b'#' { 255 } else { 0 };
            }
        }
        img
    }

    // Expected values in these tests come from cv2 4.8.1.
    #[test]
    fn square_with_hole() {
        let contours = find_contours(&image(&["#####", "#...#", "#...#", "#...#", "#####"]));
        assert_eq!(contours.len(), 2);
        assert_eq!(contours[0].points, vec![(0, 0), (0, 4), (4, 4), (4, 0)]);
        assert_eq!(contours[0].first_child, Some(1));
        assert_eq!(contours[1].parent, Some(0));
        assert_eq!(contours[1].points, vec![(0, 1), (1, 0), (3, 0), (4, 1), (4, 3), (3, 4), (1, 4), (0, 3)]);
        assert_eq!(area(&contours[0].points), 16.0);
    }

    #[test]
    fn later_components_come_first() {
        let contours = find_contours(&image(&["#..", "...", "..#"]));
        assert_eq!(contours.iter().map(|c| c.points.clone()).collect::<Vec<_>>(), vec![vec![(2, 2)], vec![(0, 0)]]);
        assert_eq!(contours[0].next, Some(1));
        assert_eq!(contours[1].prev, Some(0));
    }
}
