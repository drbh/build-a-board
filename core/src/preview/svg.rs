//! Drawing a [`Scene`] as SVG, inside a small interactive page.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::time::Duration;

use super::check::Severity;
use super::{Footprint, Item, Kind, P, PadShape, Scene, Shape, Side};

/// Drawing order, bottom to top.
const ORDER: [Kind; 11] = [
    Kind::ZoneBack,
    Kind::ZoneFront,
    Kind::CopperBack,
    Kind::CopperFront,
    Kind::Pad,
    Kind::Via,
    Kind::Fab,
    Kind::SilkBack,
    Kind::Art,
    Kind::Silk,
    Kind::Edge,
];

/// The line shown above the board.
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub message: String,
}

impl Status {
    pub fn built(elapsed: Duration) -> Self {
        Self { message: format!("built in {} ms", elapsed.as_millis()) }
    }
}

fn f(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn esc(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn points(points: &[P]) -> String {
    points.iter().map(|(x, y)| format!("{},{}", f(*x), f(*y))).collect::<Vec<_>>().join(" ")
}

/// The SVG for one item; `extra` adds classes (change highlights).
fn item(out: &mut String, item: &Item, extra: &str) {
    let class = format!("k-{}{extra}", item.kind.name());
    let net = item.net.as_deref().map(|n| format!(" data-net=\"{}\"", esc(n))).unwrap_or_default();
    let title = item.label.as_deref().map(|l| format!("<title>{}</title>", esc(l))).unwrap_or_default();
    match &item.shape {
        Shape::Line { a, b, width } => {
            let _ = write!(
                out,
                r#"<line class="{class}"{net} x1="{}" y1="{}" x2="{}" y2="{}" stroke-width="{}">{title}</line>"#,
                f(a.0),
                f(a.1),
                f(b.0),
                f(b.1),
                f(width.max(0.05))
            );
        }
        Shape::Poly { points: pts, fill, width } => {
            let fill = if *fill { " filled" } else { "" };
            let _ = write!(
                out,
                r#"<polygon class="{class}{fill}"{net} points="{}" stroke-width="{}">{title}</polygon>"#,
                points(pts),
                f(*width)
            );
        }
        Shape::Circle { center, radius, width, fill } => {
            let fill = if *fill { " filled" } else { "" };
            let _ = write!(
                out,
                r#"<circle class="{class}{fill}"{net} cx="{}" cy="{}" r="{}" stroke-width="{}">{title}</circle>"#,
                f(center.0),
                f(center.1),
                f(*radius),
                f(*width)
            );
        }
        Shape::Arc { start, mid, end, width } => {
            let _ = write!(
                out,
                r#"<path class="{class}"{net} d="{}" stroke-width="{}">{title}</path>"#,
                arc(*start, *mid, *end),
                f(*width)
            );
        }
        Shape::Text { text, at, size, angle, mirrored } => {
            let mirror = if *mirrored { " scale(-1 1)" } else { "" };
            let _ = write!(
                out,
                r#"<text class="{class}" transform="translate({} {}) rotate({}){mirror}" font-size="{}">{}</text>"#,
                f(at.0),
                f(at.1),
                f(-angle),
                f(*size),
                esc(text)
            );
        }
        Shape::Pad { at, angle, size, shape, drill, side, .. } => {
            let side = match side {
                Side::Front => "pad-f",
                Side::Back => "pad-b",
                Side::Both => "pad-tht",
            };
            let _ = write!(
                out,
                r#"<g class="{class} {side}"{net} transform="translate({} {}) rotate({})">{title}"#,
                f(at.0),
                f(at.1),
                f(-angle)
            );
            match shape {
                PadShape::Circle => {
                    let _ = write!(out, r#"<circle r="{}"/>"#, f(size.0.max(size.1) / 2.0));
                }
                PadShape::Rect { radius } => {
                    let _ = write!(
                        out,
                        r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}"/>"#,
                        f(-size.0 / 2.0),
                        f(-size.1 / 2.0),
                        f(size.0),
                        f(size.1),
                        f(*radius)
                    );
                }
            }
            if let Some(drill) = drill {
                let _ = write!(out, r#"<circle class="hole" r="{}"/>"#, f(drill / 2.0));
            }
            out.push_str("</g>");
        }
        Shape::Via { at, diameter, drill } => {
            let _ = write!(
                out,
                r#"<g class="{class}"{net}>{title}<circle cx="{x}" cy="{y}" r="{}"/><circle class="hole" cx="{x}" cy="{y}" r="{}"/></g>"#,
                f(diameter / 2.0),
                f(drill / 2.0),
                x = f(at.0),
                y = f(at.1)
            );
        }
        Shape::Image { href, center, size } => {
            let _ = write!(
                out,
                r#"<image class="{class}" href="{href}" x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="none">{title}</image>"#,
                f(center.0 - size.0 / 2.0),
                f(center.1 - size.1 / 2.0),
                f(size.0),
                f(size.1)
            );
        }
    }
}

/// An SVG arc path through three points.
fn arc(start: P, mid: P, end: P) -> String {
    let (ax, ay, bx, by, cx, cy) = (start.0, start.1, mid.0, mid.1, end.0, end.1);
    let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    if d.abs() < 1e-12 {
        return format!("M{},{} L{},{}", f(ax), f(ay), f(cx), f(cy));
    }
    let sq = |x: f64, y: f64| x * x + y * y;
    let ux = (sq(ax, ay) * (by - cy) + sq(bx, by) * (cy - ay) + sq(cx, cy) * (ay - by)) / d;
    let uy = (sq(ax, ay) * (cx - bx) + sq(bx, by) * (ax - cx) + sq(cx, cy) * (bx - ax)) / d;
    let r = (ax - ux).hypot(ay - uy);
    // Positive cross product: the arc turns toward +y, SVG's positive sweep.
    let sweep = (bx - ax) * (cy - by) - (by - ay) * (cx - bx) > 0.0;
    let angle = |x: f64, y: f64| (y - uy).atan2(x - ux);
    let tau = std::f64::consts::TAU;
    let span = if sweep {
        (angle(cx, cy) - angle(ax, ay)).rem_euclid(tau)
    } else {
        (angle(ax, ay) - angle(cx, cy)).rem_euclid(tau)
    };
    format!(
        "M{},{} A{},{} 0 {} {} {},{}",
        f(ax),
        f(ay),
        f(r),
        f(r),
        u8::from(span > std::f64::consts::PI),
        u8::from(sweep),
        f(cx),
        f(cy)
    )
}

/// Join Edge.Cuts segments into closed outlines.
fn outlines(scene: &Scene) -> Vec<Vec<P>> {
    let mut segments: Vec<(P, P)> = scene
        .items
        .iter()
        .filter(|i| i.kind == Kind::Edge)
        .filter_map(|i| match i.shape {
            Shape::Line { a, b, .. } => Some((a, b)),
            _ => None,
        })
        .collect();
    let close = |a: P, b: P| (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4;
    let mut loops = Vec::new();
    while let Some((a, b)) = segments.pop() {
        let mut points = vec![a, b];
        while let Some(i) =
            segments.iter().position(|(s, e)| close(*s, *points.last().unwrap()) || close(*e, *points.last().unwrap()))
        {
            let (s, e) = segments.swap_remove(i);
            points.push(if close(s, *points.last().unwrap()) { e } else { s });
        }
        loops.push(points);
    }
    loops
}

/// The board, and anything placed off it, so that a part off the board is
/// still in view.
fn view_box(scene: &Scene) -> (P, P) {
    let outline = outlines(scene).into_iter().flatten().map(|p| (p, p));
    let marks = scene.findings.iter().flat_map(|f| f.marks.iter().copied());
    let (lo, hi) = union(outline.chain(scene.footprints.iter().map(board_bounds)).chain(marks));
    if !lo.0.is_finite() {
        return ((0.0, 0.0), (10.0, 10.0));
    }
    let margin = 3.0;
    ((lo.0 - margin, lo.1 - margin), (hi.0 - lo.0 + 2.0 * margin, hi.1 - lo.1 + 2.0 * margin))
}

fn transform(fp: &Footprint) -> String {
    format!("translate({} {}) rotate({})", f(fp.at.0), f(fp.at.1), f(-fp.angle))
}

/// The summary of a rebuild that changed nothing visible.
pub const NO_CHANGE: &str = "no visible change";

/// A coordinate rounded to 0.1 µm, so a translated item matches its moved copy.
fn q(v: f64) -> i64 {
    (v * 1e4).round() as i64
}

/// A board-item key for change detection, as if moved by `d` (artwork
/// excluded: its data is large and it never moves with a block).
fn key(item: &Item, d: P) -> Option<String> {
    let p = |(x, y): P| (q(x + d.0), q(y + d.1));
    let shape = match &item.shape {
        Shape::Image { .. } => return None,
        Shape::Line { a, b, width } => format!("L{:?}{:?}{}", p(*a), p(*b), q(*width)),
        Shape::Poly { points, fill, width } => {
            format!("P{:?}{fill}{}", points.iter().map(|x| p(*x)).collect::<Vec<_>>(), q(*width))
        }
        Shape::Circle { center, radius, width, fill } => format!("C{:?}{}{}{fill}", p(*center), q(*radius), q(*width)),
        Shape::Arc { start, mid, end, width } => format!("A{:?}{:?}{:?}{}", p(*start), p(*mid), p(*end), q(*width)),
        Shape::Text { text, at, size, angle, mirrored } => {
            format!("T{text:?}{:?}{}{}{mirrored}", p(*at), q(*size), q(*angle))
        }
        Shape::Pad { number, at, angle, size, .. } => {
            format!("D{number}{:?}{}{:?}", p(*at), q(*angle), (q(size.0), q(size.1)))
        }
        Shape::Via { at, diameter, drill } => format!("V{:?}{}{}", p(*at), q(*diameter), q(*drill)),
    };
    Some(format!("{:?}|{:?}|{shape}", item.kind, item.net))
}

/// A part's move: offset in x and y, and turn, each rounded by [`q`].
type Offset = (i64, i64, i64);

/// A footprint's bounding box in the board frame.
fn board_bounds(fp: &Footprint) -> (P, P) {
    let ((x1, y1), (x2, y2)) = fp.bounds;
    let (s, c) = (-fp.angle).to_radians().sin_cos();
    let corners =
        [(x1, y1), (x2, y1), (x2, y2), (x1, y2)].map(|(x, y)| (fp.at.0 + x * c - y * s, fp.at.1 + x * s + y * c));
    let lo = corners.iter().fold((f64::INFINITY, f64::INFINITY), |a, p| (a.0.min(p.0), a.1.min(p.1)));
    let hi = corners.iter().fold((f64::NEG_INFINITY, f64::NEG_INFINITY), |a, p| (a.0.max(p.0), a.1.max(p.1)));
    (lo, hi)
}

fn union(boxes: impl Iterator<Item = (P, P)>) -> (P, P) {
    boxes.fold(((f64::INFINITY, f64::INFINITY), (f64::NEG_INFINITY, f64::NEG_INFINITY)), |(lo, hi), (a, b)| {
        ((lo.0.min(a.0), lo.1.min(a.1)), (hi.0.max(b.0), hi.1.max(b.1)))
    })
}

fn rect(out: &mut String, (lo, hi): (P, P), class: &str, title: &str) {
    let pad = 0.4;
    let _ = write!(
        out,
        r#"<rect class="{class}" x="{}" y="{}" width="{}" height="{}" rx=".4"><title>{}</title></rect>"#,
        f(lo.0 - pad),
        f(lo.1 - pad),
        f(hi.0 - lo.0 + 2.0 * pad),
        f(hi.1 - lo.1 + 2.0 * pad),
        esc(title)
    );
}

/// A glow beneath an added item, at its real size.
fn halo(out: &mut String, item: &Item) {
    let grow = 0.5;
    match &item.shape {
        Shape::Line { a, b, width } => {
            let _ = write!(
                out,
                r#"<line class="chg-halo" x1="{}" y1="{}" x2="{}" y2="{}" stroke-width="{}"/>"#,
                f(a.0),
                f(a.1),
                f(b.0),
                f(b.1),
                f(width + grow)
            );
        }
        Shape::Arc { start, mid, end, width } => {
            let _ = write!(
                out,
                r#"<path class="chg-halo" d="{}" stroke-width="{}"/>"#,
                arc(*start, *mid, *end),
                f(width + grow)
            );
        }
        Shape::Poly { points: pts, width, .. } => {
            let _ = write!(
                out,
                r#"<polygon class="chg-halo" points="{}" stroke-width="{}"/>"#,
                points(pts),
                f(width + grow)
            );
        }
        Shape::Via { at, diameter, .. } => {
            let _ = write!(
                out,
                r#"<circle class="chg-halo filled" cx="{}" cy="{}" r="{}"/>"#,
                f(at.0),
                f(at.1),
                f(diameter / 2.0 + grow / 2.0)
            );
        }
        Shape::Circle { center, radius, width, .. } => {
            let _ = write!(
                out,
                r#"<circle class="chg-halo filled" cx="{}" cy="{}" r="{}"/>"#,
                f(center.0),
                f(center.1),
                f(radius + width / 2.0 + grow / 2.0)
            );
        }
        Shape::Text { .. } => item_with(out, item, " chg-text"),
        Shape::Pad { .. } | Shape::Image { .. } => {}
    }
}

fn item_with(out: &mut String, i: &Item, extra: &str) {
    item(out, i, extra);
}

/// What changed since `previous`, as highlights drawn beneath the board
/// (`under`: glows and ghosts) and above it (`over`: moved blocks), plus a
/// summary. Parts that moved together by the same offset are one block, and
/// copper that moved with them is not a change.
fn changes(under: &mut String, over: &mut String, scene: &Scene, previous: &Scene) -> String {
    let (now, before) = (scene.placements(), previous.placements());
    // Moved parts, grouped by their offset (and turn).
    let mut moves: BTreeMap<Offset, Vec<(&Footprint, &Footprint)>> = BTreeMap::new();
    let mut summary = Vec::new();
    for (reference, fp) in &now {
        match before.get(reference) {
            None => {
                rect(over, board_bounds(fp), "chg-added-box", reference);
                summary.push(format!("+{reference}"));
            }
            Some(old) if old.at != fp.at || old.angle != fp.angle => {
                let delta = (q(fp.at.0 - old.at.0), q(fp.at.1 - old.at.1), q(fp.angle - old.angle));
                moves.entry(delta).or_default().push((old, fp));
            }
            Some(old) if **old != **fp => {
                rect(over, board_bounds(fp), "chg-edited-box", reference);
                summary.push(format!("~{reference}"));
            }
            Some(_) => {}
        }
    }
    for (reference, fp) in &before {
        if !now.contains_key(reference) {
            rect(under, board_bounds(fp), "chg-removed-box", reference);
            summary.push(format!("−{reference}"));
        }
    }
    let mut offsets = vec![(0.0, 0.0)];
    for ((dx, dy, turn), parts) in &moves {
        let refs: Vec<&str> = parts.iter().map(|(_, fp)| fp.reference.as_str()).collect();
        let names = if refs.len() > 4 {
            format!("{}, … ({} parts)", refs[..3].join(", "), refs.len())
        } else {
            refs.join(", ")
        };
        let (old_box, new_box) = (
            union(parts.iter().map(|(old, _)| board_bounds(old))),
            union(parts.iter().map(|(_, fp)| board_bounds(fp))),
        );
        rect(over, old_box, "chg-ghost", &format!("{names} (before)"));
        rect(over, new_box, "chg-moved", &names);
        let centre = |(lo, hi): (P, P)| ((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0);
        let (a, b) = (centre(old_box), centre(new_box));
        let _ = write!(
            over,
            r#"<line class="chg-arrow" x1="{}" y1="{}" x2="{}" y2="{}"/>"#,
            f(a.0),
            f(a.1),
            f(b.0),
            f(b.1)
        );
        let (dx, dy) = (*dx as f64 / 1e4, *dy as f64 / 1e4);
        let turn = *turn as f64 / 1e4;
        let shifted = dx != 0.0 || dy != 0.0;
        let text = match (shifted, turn != 0.0) {
            (true, true) => format!("moved {names} by ({dx:+.2}, {dy:+.2}) and turned {turn:+}°"),
            (false, true) => format!("turned {names} {turn:+}°"),
            _ => format!("moved {names} by ({dx:+.2}, {dy:+.2})"),
        };
        if turn == 0.0 {
            offsets.push((dx, dy));
        }
        summary.insert(0, text);
    }
    // Board items: unchanged if the same item exists before, as is or moved
    // by one of the blocks' offsets.
    let before_keys: BTreeSet<String> = previous.items.iter().filter_map(|i| key(i, (0.0, 0.0))).collect();
    let now_keys: BTreeSet<String> = scene.items.iter().filter_map(|i| key(i, (0.0, 0.0))).collect();
    let (mut added, mut removed) = (0, 0);
    for i in &scene.items {
        let kept = offsets.iter().any(|&(dx, dy)| key(i, (-dx, -dy)).is_some_and(|k| before_keys.contains(&k)));
        if !kept && key(i, (0.0, 0.0)).is_some() {
            halo(under, i);
            added += 1;
        }
    }
    for i in &previous.items {
        let kept = offsets.iter().any(|&d| key(i, d).is_some_and(|k| now_keys.contains(&k)));
        if !kept && key(i, (0.0, 0.0)).is_some() {
            item(under, i, " chg-removed");
            removed += 1;
        }
    }
    if added + removed > 0 {
        summary.push(format!("+{added} −{removed} copper/graphics"));
    }
    if summary.is_empty() { NO_CHANGE.into() } else { summary.join(" · ") }
}

/// The board as one `<svg>` element, with changes since `previous`
/// highlighted; returns the SVG and a change summary.
pub fn svg(scene: &Scene, previous: Option<&Scene>) -> (String, Option<String>) {
    let ((x, y), (w, h)) = view_box(scene);
    let mut out = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}" data-home="{} {} {} {}">"#,
        f(x),
        f(y),
        f(w),
        f(h),
        f(x),
        f(y),
        f(w),
        f(h)
    );
    for outline in outlines(scene) {
        let _ = write!(out, r#"<polygon class="board" points="{}"/>"#, points(&outline));
    }
    let (mut under, mut over) = (String::new(), String::new());
    let summary = previous.map(|previous| changes(&mut under, &mut over, scene, previous));
    let _ = write!(out, r#"<g class="changes">{under}</g>"#);
    for kind in ORDER {
        for i in scene.items.iter().filter(|i| i.kind == kind) {
            item(&mut out, i, "");
        }
        for fp in &scene.footprints {
            let mine: Vec<&Item> = fp.items.iter().filter(|i| i.kind == kind).collect();
            if mine.is_empty() {
                continue;
            }
            let _ = write!(out, r#"<g data-ref="{}" transform="{}">"#, esc(&fp.reference), transform(fp));
            for i in mine {
                item(&mut out, i, "");
            }
            out.push_str("</g>");
        }
    }
    for (net, a, b) in &scene.ratsnest {
        let _ = write!(
            out,
            r#"<line class="k-rats" data-net="{}" x1="{}" y1="{}" x2="{}" y2="{}"><title>unrouted: {}</title></line>"#,
            esc(net),
            f(a.0),
            f(a.1),
            f(b.0),
            f(b.1),
            esc(net)
        );
    }
    out.push_str(r#"<g class="findings">"#);
    for finding in &scene.findings {
        for mark in &finding.marks {
            rect(&mut out, *mark, &format!("finding {}", finding.severity.name()), &finding.message);
        }
    }
    out.push_str("</g>");
    let _ = write!(out, r#"<g class="changes">{over}</g>"#);
    out.push_str("</svg>");
    (out, summary)
}

/// A self-contained page; `live` makes it poll `bb preview --watch` for rebuilds.
pub fn page(scene: &Scene, previous: Option<&Scene>, status: &Status, live: bool) -> String {
    let (svg, _) = svg(scene, previous);
    page_with(scene, &svg, status, live)
}

/// A page around an already drawn `svg`.
pub fn page_with(scene: &Scene, svg: &str, status: &Status, live: bool) -> String {
    PAGE.replace("{{TITLE}}", &esc(&scene.title))
        .replace("{{STATUS}}", &esc(&status_line(scene, status)))
        .replace("{{LIVE}}", if live { "true" } else { "false" })
        .replace("{{SVG}}", svg)
}

/// The status line for a scene.
pub fn status_line(scene: &Scene, status: &Status) -> String {
    let copper = if scene.routed { "routed copper" } else { "design copper" };
    let mut line =
        format!("{} · {} parts · {} unrouted · {copper}", status.message, scene.footprints.len(), scene.ratsnest.len());
    for severity in [Severity::Error, Severity::Warning] {
        let n = scene.findings.iter().filter(|f| f.severity == severity).count();
        if n > 0 {
            let _ = write!(line, " · {n} {}{}", severity.name(), if n == 1 { "" } else { "s" });
        }
    }
    line
}

const PAGE: &str = include_str!("page.html");

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(dx: f64, stretch: f64) -> Scene {
        let line = |a: P, b: P| Item::new(Kind::CopperFront, Shape::Line { a, b, width: 0.25 }).net(Some("A"));
        let part = |reference: &str, x: f64| Footprint {
            reference: reference.into(),
            value: "1k".into(),
            at: (x + dx, 0.0),
            angle: 0.0,
            items: Vec::new(),
            bounds: ((-1.0, -0.5), (1.0, 0.5)),
            courtyard: Vec::new(),
        };
        Scene {
            footprints: vec![part("R1", 0.0), part("R2", 5.0)],
            // A track inside the block, and one from the block to a fixed point.
            items: vec![line((0.1 + dx, 0.0), (4.9 + dx, 0.0)), line((5.0 + dx, 0.0), (20.0 + stretch, 0.0))],
            ..Scene::default()
        }
    }

    #[test]
    fn a_block_move_is_one_change_and_only_stretched_copper_counts() {
        let (mut under, mut over) = (String::new(), String::new());
        let summary = changes(&mut under, &mut over, &scene(2.0, 0.0), &scene(0.0, 0.0));
        assert_eq!(summary, "moved R1, R2 by (+2.00, +0.00) · +1 −1 copper/graphics");
        assert_eq!(over.matches("chg-moved").count(), 1);
    }

    #[test]
    fn an_unchanged_rebuild_reports_no_change() {
        let (mut under, mut over) = (String::new(), String::new());
        assert_eq!(changes(&mut under, &mut over, &scene(0.0, 0.0), &scene(0.0, 0.0)), NO_CHANGE);
    }
}
