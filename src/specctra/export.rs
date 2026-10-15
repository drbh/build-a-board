//! `SPECCTRA_DB::FromBOARD` + `ExportPCB`: a board to Specctra DSN text.

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use super::board::{Board, Footprint, GraphicKind, NetClass, Pad, PadShape, ProjectRules, TrackKind};
use super::clip::{canonical_order, offset_polygon};
use super::fmt::{Out, double2str, fixed6, g6, quote, strip_useless};
use super::geom::{P, arc_polyline_inside, circle_polyline_split_error, kiround, normalize_deg, round_chamfered_rect};

/// KiCad's `ARC_HIGH_DEF` and the board/pad maximum polygonisation error (5 µm).
const MAX_ERROR: i64 = 5_000;
/// The DRC epsilon courtyards are shrunk by.
const DRC_EPSILON: f64 = 5_000.0;
/// The KiCad release whose exporter this reproduces.
pub const HOST_VERSION: &str = "10.0.5";

/// nm → DSN µm (`scale`).
fn scale(nm: i64) -> f64 {
    nm as f64 / (1e6 / 1000.0)
}

/// nm → µm as used in padstack names (`IU2um`).
fn iu2um(nm: i64) -> f64 {
    nm as f64 * (1000.0 / 1e6)
}

/// A DSN point (µm, Y up), with negative zero cleared.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Dp {
    x: f64,
    y: f64,
}

impl Dp {
    fn fixed(mut self) -> Self {
        if self.x == 0.0 {
            self.x = 0.0;
        }
        if self.y == 0.0 {
            self.y = 0.0;
        }
        self
    }

    fn add(self, o: Dp) -> Dp {
        Dp { x: self.x + o.x, y: self.y + o.y }
    }
}

/// `mapPt`: board nm → DSN µm with Y negated.
fn map_pt(p: P) -> Dp {
    Dp { x: scale(p.x), y: -scale(p.y) }.fixed()
}

// ----------------------------------------------------------------- shapes --

#[derive(Clone, Debug)]
enum Shape {
    Path { kind: &'static str, layer: String, aperture: f64, points: Vec<Dp> },
    Circle { layer: String, diameter: f64, vertex: Dp },
    Rect { layer: String, a: Dp, b: Dp },
}

impl Shape {
    fn format(&self, out: &mut Out, nest: usize) {
        let newline = if nest > 0 { "\n" } else { "" };
        match self {
            Shape::Path { kind, layer, aperture, points } => {
                let mut per_line = out.print(nest, &format!("({kind} {} {}", quote(layer), g6(*aperture)));
                let wrap_nest = (nest + 1).max(6);
                for p in points {
                    if per_line > 70 {
                        out.print(0, "\n");
                        per_line = out.print(wrap_nest, "");
                    } else {
                        per_line += out.print(0, "  ");
                    }
                    per_line += out.print(0, &format!("{} {}", g6(p.x), g6(p.y)));
                }
                out.print(0, &format!("){newline}"));
            }
            Shape::Circle { layer, diameter, vertex } => {
                out.print(nest, &format!("(circle {} {}", quote(layer), g6(*diameter)));
                if vertex.x != 0.0 || vertex.y != 0.0 {
                    out.print(0, &format!(" {} {}){newline}", g6(vertex.x), g6(vertex.y)));
                } else {
                    out.print(0, &format!("){newline}"));
                }
            }
            Shape::Rect { layer, a, b } => {
                out.print(
                    nest,
                    &format!("(rect {} {} {} {} {}){newline}", quote(layer), g6(a.x), g6(a.y), g6(b.x), g6(b.y)),
                );
            }
        }
    }
}

/// `(shape ...)` in a padstack or `(outline ...)` in an image.
fn format_wrapped(out: &mut Out, nest: usize, name: &str, shape: &Shape) {
    out.print(nest, &format!("({name} "));
    shape.format(out, 0);
    out.print(0, ")\n");
}

// -------------------------------------------------------------- padstacks --

#[derive(Clone, Debug)]
struct Padstack {
    id: String,
    shapes: Vec<Shape>,
    hash: String,
}

impl Padstack {
    fn new(id: String, shapes: Vec<Shape>) -> Self {
        let mut probe = Out::default();
        Self::format_contents(&shapes, &mut probe, 0);
        Self { id, shapes, hash: strip_useless(&probe.buf) }
    }

    fn format_contents(shapes: &[Shape], out: &mut Out, nest: usize) {
        for shape in shapes {
            format_wrapped(out, nest, "shape", shape);
        }
        out.print(nest, "");
        out.print(0, "(attach off)");
        out.print(0, "\n");
    }

    fn format(&self, out: &mut Out, nest: usize) {
        out.print(nest, &format!("(padstack {}\n", quote(&self.id)));
        Self::format_contents(&self.shapes, out, nest + 1);
        out.print(nest, ")\n");
    }

    /// `PADSTACK::Compare`: content hash, then name.
    fn key(&self) -> (String, String) {
        (self.hash.clone(), self.id.clone())
    }
}

fn make_via(diameter: i64, drill: i64, top: usize, bottom: usize, layer_ids: &[String]) -> Padstack {
    let d = scale(diameter);
    let shapes = (top..=bottom)
        .map(|l| Shape::Circle { layer: layer_ids[l].clone(), diameter: d, vertex: Dp::default() })
        .collect();
    Padstack::new(format!("Via[{top}-{bottom}]_{}:{}_um", g6(d), g6(iu2um(drill))), shapes)
}

fn is_round_keepout(board: &Board, pad: &Pad) -> bool {
    pad.shape == PadShape::Circle && (pad.drill.x >= pad.size.x || !board.copper_set(&pad.layers).iter().any(|&on| on))
}

fn make_padstack(board: &Board, pad: &Pad) -> Padstack {
    let on = board.copper_set(&pad.layers);
    let all = on.iter().all(|&b| b);
    let count = on.len();
    let mut uniq = String::from("[");
    if all {
        uniq.push('A');
    }
    let mut layer_names = Vec::new();
    for (i, &is_on) in on.iter().enumerate() {
        if all || is_on {
            layer_names.push(board.copper[i].0.clone());
            if !all {
                uniq.push(if i == 0 {
                    'T'
                } else if i == count - 1 {
                    'B'
                } else {
                    char::from(b'0' + i as u8)
                });
            }
        }
    }
    uniq.push(']');
    let mut offset = Dp::default();
    if pad.offset.x != 0 || pad.offset.y != 0 {
        offset = map_pt(pad.offset);
        uniq.push_str(&format!("[{},{}]", fixed6(offset.x), fixed6(offset.y)));
    }
    let size = pad.size;
    let shapes_on = |make: &dyn Fn(&str) -> Shape| layer_names.iter().map(|l| make(l)).collect::<Vec<_>>();
    match pad.shape {
        PadShape::Circle => {
            let d = scale(size.x);
            let shapes = shapes_on(&|l| Shape::Circle { layer: l.to_owned(), diameter: d, vertex: offset });
            Padstack::new(format!("Round{uniq}Pad_{}_um", fixed6(iu2um(size.x))), shapes)
        }
        PadShape::Rect => {
            let (dx, dy) = (scale(size.x) / 2.0, scale(size.y) / 2.0);
            let a = Dp { x: -dx, y: -dy }.add(offset).fixed();
            let b = Dp { x: dx, y: dy }.add(offset).fixed();
            let shapes = shapes_on(&|l| Shape::Rect { layer: l.to_owned(), a, b });
            Padstack::new(format!("Rect{uniq}Pad_{}x{}_um", fixed6(iu2um(size.x)), fixed6(iu2um(size.y))), shapes)
        }
        PadShape::Oval => {
            let (dx, dy) = (scale(size.x) / 2.0, scale(size.y) / 2.0);
            let mut dr = dx - dy;
            let (radius, start, stop) = if dr >= 0.0 {
                (dy, Dp { x: -dr, y: 0.0 }, Dp { x: dr, y: 0.0 })
            } else {
                dr = -dr;
                (dx, Dp { x: 0.0, y: -dr }, Dp { x: 0.0, y: dr })
            };
            let (start, stop) = (start.add(offset), stop.add(offset));
            let shapes = shapes_on(&|l| Shape::Path {
                kind: "path",
                layer: l.to_owned(),
                aperture: 2.0 * radius,
                points: vec![start, stop],
            });
            Padstack::new(format!("Oval{uniq}Pad_{}x{}_um", fixed6(iu2um(size.x)), fixed6(iu2um(size.y))), shapes)
        }
        PadShape::Trapezoid => {
            let (dx, dy) = (scale(size.x) / 2.0, scale(size.y) / 2.0);
            let (ddx, ddy) = (scale(pad.delta.x) / 2.0, scale(pad.delta.y) / 2.0);
            let corners = [
                Dp { x: -dx - ddy, y: -dy - ddx },
                Dp { x: -dx + ddy, y: dy + ddx },
                Dp { x: dx - ddy, y: dy - ddx },
                Dp { x: dx + ddy, y: -dy + ddx },
            ]
            .map(|c| c.add(offset));
            let shapes = shapes_on(&|l| Shape::Path {
                kind: "polygon",
                layer: l.to_owned(),
                aperture: 0.0,
                points: corners.to_vec(),
            });
            let sign = |v: i64| if v < 0 { "n" } else { "p" };
            Padstack::new(
                format!(
                    "Trapz{uniq}Pad_{}x{}_{}{}x{}{}_um",
                    fixed6(iu2um(size.x)),
                    fixed6(iu2um(size.y)),
                    sign(pad.delta.x),
                    fixed6(iu2um(pad.delta.x).abs()),
                    sign(pad.delta.y),
                    fixed6(iu2um(pad.delta.y).abs())
                ),
                shapes,
            )
        }
        PadShape::RoundRect | PadShape::ChamferedRect | PadShape::Custom => {
            // Custom pads are approximated by their rounded-rectangle anchor;
            // KiCad itself exports a convex hull of their primitives.
            let chamfer = pad.shape == PadShape::ChamferedRect;
            let mut radius = if pad.shape == PadShape::RoundRect || chamfer {
                kiround(size.x.min(size.y) as f64 * pad.rratio)
            } else {
                0
            };
            let correction = (std::f64::consts::PI / 36.0).cos();
            let extra = kiround(radius as f64 * (1.0 - correction));
            let psize = P::new(size.x + extra * 2, size.y + extra * 2);
            radius += extra;
            let corners = if chamfer { pad.chamfer_corners } else { 0 };
            let polygon = round_chamfered_rect(psize, radius, pad.chamfer_ratio, corners, MAX_ERROR);
            let mut points: Vec<Dp> =
                polygon.iter().map(|c| Dp { x: scale(c.x), y: scale(-c.y) }.add(offset)).collect();
            if let Some(&first) = points.first() {
                points.push(first);
            }
            let shapes = shapes_on(&|l| Shape::Path {
                kind: "polygon",
                layer: l.to_owned(),
                aperture: 0.0,
                points: points.clone(),
            });
            let ratio = if chamfer { pad.chamfer_ratio } else { 0.0 };
            Padstack::new(
                format!(
                    "RoundRect{uniq}Pad_{}x{}_{}_um_{}_{:X}",
                    fixed6(iu2um(size.x)),
                    fixed6(iu2um(size.y)),
                    fixed6(iu2um(radius)),
                    fixed6(ratio),
                    corners
                ),
                shapes,
            )
        }
    }
}

// ----------------------------------------------------------------- images --

#[derive(Clone, Debug)]
struct Pin {
    padstack: String,
    rotation: f64,
    id: String,
    vertex: Dp,
    net: Option<String>,
}

#[derive(Clone, Debug)]
struct Image {
    id: String,
    duplicated: usize,
    outlines: Vec<Shape>,
    pins: Vec<Pin>,
    keepouts: Vec<Shape>,
    hash: String,
}

impl Image {
    fn image_id(&self) -> String {
        if self.duplicated > 0 { format!("{}::{}", self.id, self.duplicated) } else { self.id.clone() }
    }

    fn format_contents(&self, out: &mut Out, nest: usize) {
        out.print(0, "\n");
        for outline in &self.outlines {
            format_wrapped(out, nest, "outline", outline);
        }
        for pin in &self.pins {
            if pin.rotation != 0.0 {
                out.print(nest, &format!("(pin {} (rotate {})", quote(&pin.padstack), g6(pin.rotation)));
            } else {
                out.print(nest, &format!("(pin {}", quote(&pin.padstack)));
            }
            out.print(0, &format!(" {} {} {})\n", quote(&pin.id), g6(pin.vertex.x), g6(pin.vertex.y)));
        }
        for keepout in &self.keepouts {
            out.print(nest, "(keepout \"\" ");
            keepout.format(out, 0);
            out.print(0, ")\n");
        }
    }

    fn format(&self, out: &mut Out, nest: usize) {
        out.print(nest, &format!("(image {}", quote(&self.image_id())));
        self.format_contents(out, nest + 1);
        out.print(nest, ")\n");
    }

    fn rehash(&mut self) {
        let mut probe = Out::default();
        self.format_contents(&mut probe, 0);
        self.hash = strip_useless(&probe.buf);
    }
}

/// Close a chain of segment shapes into one polygon (`ConvertOutlineToPolygon`).
fn chain_contours(segments: &[(P, P)], epsilon: i64) -> Vec<Vec<P>> {
    let close = |a: P, b: P| ((a.x - b.x) as f64).hypot((a.y - b.y) as f64) <= epsilon as f64;
    let mut used = vec![false; segments.len()];
    let mut contours = Vec::new();
    for start in 0..segments.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let (first, mut end) = segments[start];
        let mut contour = vec![first, end];
        loop {
            let next =
                (0..segments.len()).find(|&i| !used[i] && (close(segments[i].0, end) || close(segments[i].1, end)));
            let Some(i) = next else { break };
            used[i] = true;
            end = if close(segments[i].0, end) { segments[i].1 } else { segments[i].0 };
            contour.push(end);
        }
        if contour.len() > 2 && close(*contour.last().unwrap(), first) {
            contour.pop();
        }
        contours.push(contour);
    }
    contours
}

/// The footprint courtyard as `GetCourtyard( F_CrtYd )` + `B_CrtYd` after
/// the exporter's `Simplify()`, in board coordinates.
pub(super) fn courtyard(fp: &Footprint) -> Vec<Vec<P>> {
    let mut polygons = Vec::new();
    for layer in ["F.CrtYd", "B.CrtYd"] {
        let mut segments = Vec::new();
        for g in fp.graphics.iter().filter(|g| g.layer == layer) {
            match &g.kind {
                GraphicKind::Segment { start, end } => segments.push((*start, *end)),
                GraphicKind::Arc { start, end, .. } => segments.push((*start, *end)),
                GraphicKind::Rect { start, end } => {
                    polygons.push(vec![*start, P::new(end.x, start.y), *end, P::new(start.x, end.y)])
                }
                GraphicKind::Circle { center, radius } => {
                    polygons.push(circle_polyline_split_error(*center, *radius, MAX_ERROR))
                }
                GraphicKind::Poly { points } => polygons.push(points.clone()),
                GraphicKind::Other => {}
            }
        }
        polygons.extend(chain_contours(&segments, 20_000));
    }
    polygons
        .iter()
        .filter(|p| p.len() >= 3)
        .map(|p| canonical_order(&offset_polygon(p, -DRC_EPSILON)))
        .filter(|p| p.len() >= 3)
        .collect()
}

fn make_image(board: &Board, fp: &Footprint, rules: &ProjectRules) -> (Image, Vec<Padstack>) {
    let mut pins = Vec::new();
    let mut keepouts = Vec::new();
    let mut pin_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut padstacks = Vec::new();
    for pad in &fp.pads {
        if is_round_keepout(board, pad) {
            let diameter = scale(pad.drill.x) + scale(rules.hole_clearance * 2);
            let vertex = map_pt(pad.local);
            for (layer, _) in &board.copper {
                keepouts.push(Shape::Circle { layer: layer.clone(), diameter, vertex });
            }
            continue;
        }
        if !board.copper_set(&pad.layers).iter().any(|&b| b) {
            continue;
        }
        let padstack = make_padstack(board, pad);
        let mut id = pad.number.clone();
        if !pad.number.is_empty() && !pin_counts.contains_key(&pad.number) {
            pin_counts.insert(pad.number.clone(), 0);
        } else {
            let dups = pin_counts.entry(pad.number.clone()).or_insert(0);
            *dups += 1;
            id = format!("{id}@{dups}");
        }
        pins.push(Pin {
            padstack: padstack.id.clone(),
            rotation: normalize_deg(pad.orientation - fp.orientation),
            id,
            vertex: map_pt(pad.local),
            net: pad.net.clone(),
        });
        padstacks.push(padstack);
    }

    let mut outlines = Vec::new();
    for polygon in courtyard(fp) {
        let points = polygon.iter().map(|&c| map_pt(fp.to_local(c))).collect();
        outlines.push(Shape::Path { kind: "polygon", layer: "signal".into(), aperture: 0.0, points });
    }
    for g in fp.graphics.iter().filter(|g| g.layer != "F.CrtYd" && g.layer != "B.CrtYd") {
        let local = |p: P| map_pt(fp.to_local(p));
        let points = match &g.kind {
            GraphicKind::Segment { start, end } => vec![local(*start), local(*end)],
            GraphicKind::Circle { center, radius } => {
                arc_polyline_inside(*center, *radius, 0.0, 360.0, MAX_ERROR).into_iter().map(local).collect()
            }
            GraphicKind::Rect { start, end } => vec![
                local(*start),
                local(P::new(end.x, start.y)),
                local(*end),
                local(P::new(start.x, end.y)),
                local(*start),
            ],
            // KiCad exports arcs as thick-arc polygons; a centreline
            // polyline keeps the outline without that detail.
            GraphicKind::Arc { start, mid, end } => vec![local(*start), local(*mid), local(*end)],
            GraphicKind::Poly { .. } | GraphicKind::Other => continue,
        };
        let kind = if matches!(g.kind, GraphicKind::Arc { .. }) { "polygon" } else { "path" };
        let aperture = if kind == "polygon" { 0.0 } else { scale(g.width) };
        outlines.push(Shape::Path { kind, layer: "signal".into(), aperture, points });
    }

    let mut image = Image { id: fp.lib_id.clone(), duplicated: 0, outlines, pins, keepouts, hash: String::new() };
    image.rehash();
    (image, padstacks)
}

// ------------------------------------------------------------------ export --

struct Net {
    id: String,
    pins: Vec<(String, String)>,
}

fn format_net(out: &mut Out, nest: usize, net: &Net) {
    out.print(nest, &format!("(net {}", quote(&net.id)));
    out.print(0, "\n");
    if !net.pins.is_empty() {
        let mut per_line = out.print(nest + 1, "(pins");
        for (component, pin) in &net.pins {
            if per_line > 80 {
                out.print(0, "\n");
                per_line = out.print(nest + 2, "");
            } else {
                per_line += out.print(0, " ");
            }
            per_line += out.print(0, &format!("{}-{}", quote(component), quote(pin)));
        }
        out.print(0, ")\n");
    }
    out.print(nest, ")\n");
}

fn format_rule(out: &mut Out, nest: usize, rules: &[String]) {
    out.print(nest, "(rule");
    if rules.len() == 1 {
        out.print(0, &format!(" {})", rules[0]));
        if nest > 0 {
            out.print(0, "\n");
        }
    } else {
        out.print(0, "\n");
        for rule in rules {
            out.print(nest + 1, &format!("{rule}\n"));
        }
        out.print(nest, ")");
        out.print(0, "\n");
    }
}

fn class_rules(class: &NetClass) -> Vec<String> {
    vec![format!("(width {})", g6(scale(class.track_width))), format!("(clearance {})", g6(scale(class.clearance)))]
}

/// Write `board` as a Specctra DSN design named `pcb_name`, exactly as
/// KiCad 10's `ExportSpecctraDSN` would (see the module docs for caveats).
pub fn export_dsn(board: &Board, rules: &ProjectRules, pcb_name: &str) -> Result<String> {
    let mut refs = std::collections::HashSet::new();
    for fp in &board.footprints {
        if fp.reference.is_empty() {
            bail!("Footprint with value of '{}' has an empty reference designator.", fp.value);
        }
        if !refs.insert(fp.reference.as_str()) {
            bail!("Multiple footprints have the reference designator '{}'.", fp.reference);
        }
    }
    if let Some(fp) = board.footprints.iter().find(|fp| fp.back) {
        // KiCad flips these into the top view first; not implemented yet.
        bail!("{}: back-side footprints are not supported by the DSN exporter", fp.reference);
    }
    let layer_ids: Vec<String> = board.copper.iter().map(|(n, _)| n.clone()).collect();
    let mut out = Out::default();
    out.print(0, &format!("(pcb {}\n", quote(pcb_name)));
    out.print(1, "(parser\n");
    out.print(2, "(string_quote \")\n");
    out.print(2, "(space_in_quoted_tokens on)\n");
    out.print(2, "(host_cad \"KiCad's Pcbnew\")\n");
    out.print(2, &format!("(host_version \"{HOST_VERSION}\")\n"));
    out.print(1, ")\n");
    out.print(1, "(resolution um 10)\n");
    out.print(1, "(unit um)\n");

    // ---- structure
    out.print(1, "(structure\n");
    for (index, (name, kind)) in board.copper.iter().enumerate() {
        let kind = match kind.as_str() {
            "power" => "power",
            "jumper" => "jumper",
            _ => "signal",
        };
        out.print(2, &format!("(layer {}\n", quote(name)));
        out.print(3, &format!("(type {kind})\n"));
        out.print(3, "(property\n");
        out.print(4, &format!("(index {index})\n"));
        out.print(3, ")\n");
        out.print(2, ")\n");
    }
    let outlines = board_outlines(board)?;
    out.print(2, "(boundary\n");
    let mut structure_keepouts = Vec::new();
    for (outline, holes) in &outlines {
        let mut points: Vec<Dp> = outline.iter().map(|&p| map_pt(p)).collect();
        points.push(map_pt(outline[0]));
        Shape::Path { kind: "path", layer: "pcb".into(), aperture: 0.0, points }.format(&mut out, 3);
        for hole in holes {
            let mut points: Vec<Dp> = hole.iter().map(|&p| map_pt(p)).collect();
            points.push(map_pt(hole[0]));
            structure_keepouts.push(Shape::Path { kind: "polygon", layer: "signal".into(), aperture: 0.0, points });
        }
    }
    out.print(2, ")\n");

    let mut bogus_nets = Vec::new();
    for zone in board.zones.iter().filter(|z| z.keepout.is_none()) {
        for layer in zone_layers(board, &zone.layers) {
            let name = if zone.net.is_empty() {
                let bogus = format!("@:no_net_{}", bogus_nets.len());
                bogus_nets.push(bogus.clone());
                bogus
            } else {
                zone.net.clone()
            };
            format_area(&mut out, 2, "plane", &name, &layer, &zone.outlines);
        }
    }
    for keepout in &structure_keepouts {
        out.print(2, "(keepout \"\" ");
        keepout.format(&mut out, 0);
        out.print(0, ")\n");
    }
    for zone in board.zones.iter().filter(|z| z.keepout.is_some()) {
        let (no_tracks, no_vias) = zone.keepout.unwrap_or_default();
        let kind = match (no_tracks, no_vias) {
            (false, true) => "via_keepout",
            (true, false) => "wire_keepout",
            _ => "keepout",
        };
        for layer in zone_layers(board, &zone.layers) {
            format_area(&mut out, 2, kind, "", &layer, &zone.outlines);
        }
    }

    // ---- images, components, netlist
    let mut nets: Vec<Net> = board.nets.iter().map(|n| Net { id: n.clone(), pins: Vec::new() }).collect();
    let mut images: Vec<Image> = Vec::new();
    let mut components: Vec<(String, Vec<String>)> = Vec::new();
    let mut padstack_set: BTreeMap<(String, String), Padstack> = BTreeMap::new();
    for fp in &board.footprints {
        let (mut image, padstacks) = make_image(board, fp, rules);
        for ps in padstacks {
            padstack_set.entry(ps.key()).or_insert(ps);
        }
        for pin in &image.pins {
            if let Some(net) = &pin.net {
                let code = board.net_code(net);
                if code > 0 {
                    nets[code - 1].pins.push((fp.reference.clone(), pin.id.clone()));
                }
            }
        }
        let registered = match images.iter().position(|i| i.hash == image.hash) {
            Some(index) => index,
            None => {
                image.duplicated = images.iter().filter(|i| i.id == image.id).count();
                images.push(image);
                images.len() - 1
            }
        };
        let image_id = images[registered].image_id();
        let rotation = fp_orientation_degrees(fp.orientation);
        let vertex = map_pt(fp.position);
        let mut place = format!(
            "(place {} {} {} front {}",
            quote(&fp.reference),
            double2str(vertex.x),
            double2str(vertex.y),
            double2str(rotation)
        );
        if !fp.value.is_empty() {
            place.push_str(&format!(" (PN {})", quote(&fp.value)));
        }
        place.push_str(")\n");
        match components.iter_mut().find(|(id, _)| *id == image_id) {
            Some((_, places)) => places.push(place),
            None => components.push((image_id, vec![place])),
        }
    }

    // ---- net classes in use (name order; libc++ unordered_map iteration
    // yields them in reverse insertion order)
    let mut names: Vec<&String> = board.nets.iter().collect();
    names.sort();
    let mut in_use: Vec<&NetClass> = Vec::new();
    for net in &names {
        let class = rules.class_of(net);
        if class.name != "Default" && !in_use.iter().any(|c| c.name == class.name) {
            in_use.push(class);
        }
    }
    in_use.reverse();

    // ---- vias
    let top = 0;
    let bottom = board.copper.len().saturating_sub(1);
    let mut vias: Vec<Padstack> =
        vec![make_via(rules.default.via_diameter, rules.default.via_drill, top, bottom, &layer_ids)];
    let lookup_via = |vias: &mut Vec<Padstack>, via: Padstack| -> String {
        match vias.iter().find(|v| v.key() == via.key()) {
            Some(existing) => existing.id.clone(),
            None => {
                let id = via.id.clone();
                vias.push(via);
                id
            }
        }
    };
    for class in &in_use {
        lookup_via(&mut vias, make_via(class.via_diameter, class.via_drill, top, bottom, &layer_ids));
    }

    // ---- wiring
    let mut wires = String::new();
    {
        let mut wire_out = Out::default();
        struct Open {
            net: String,
            layer: String,
            width: i64,
            points: Vec<Dp>,
            fixed: bool,
        }
        let mut current: Option<Open> = None;
        let mut flushed: Vec<Open> = Vec::new();
        let (mut old_net, mut old_width, mut old_layer) = (String::from("\u{0}"), -1i64, String::from("\u{0}"));
        for track in &board.tracks {
            if matches!(track.kind, TrackKind::Via { .. }) || track.net.is_empty() {
                continue;
            }
            let start = map_pt(track.start);
            let continues = current.as_ref().is_some_and(|c| c.points.last() == Some(&start));
            if old_net != track.net || old_width != track.width || old_layer != track.layer || !continues {
                old_width = track.width;
                old_layer = track.layer.clone();
                old_net = track.net.clone();
                if let Some(done) = current.take() {
                    flushed.push(done);
                }
                current = Some(Open {
                    net: track.net.clone(),
                    layer: track.layer.clone(),
                    width: track.width,
                    points: vec![start],
                    fixed: track.locked,
                });
            }
            if let Some(open) = current.as_mut() {
                open.points.push(map_pt(track.end));
            }
        }
        flushed.extend(current);
        for wire in flushed {
            wire_out.print(2, "(wire ");
            Shape::Path { kind: "path", layer: wire.layer, aperture: scale(wire.width), points: wire.points }
                .format(&mut wire_out, 0);
            wire_out.print(0, &format!("(net {})", quote(&wire.net)));
            wire_out.print(0, if wire.fixed { "(type fix)" } else { "(type route)" });
            wire_out.print(0, ")\n");
        }
        for track in &board.tracks {
            let TrackKind::Via { diameter, drill, layers } = &track.kind else { continue };
            if track.net.is_empty() {
                continue;
            }
            let indices: Vec<usize> = layers.iter().filter_map(|l| board.copper_index(l)).collect();
            let (a, b) = (indices.first().copied().unwrap_or(top), indices.last().copied().unwrap_or(bottom));
            let (lo, hi) = (a.min(b), a.max(b));
            let id = lookup_via(&mut vias, make_via(*diameter, *drill, lo, hi, &layer_ids));
            let v = map_pt(track.start);
            let mut per_line = wire_out.print(2, &format!("(via {}", quote(&id)));
            if per_line > 80 {
                wire_out.print(0, "\n");
                per_line = wire_out.print(3, "");
            } else {
                per_line += wire_out.print(0, "  ");
            }
            per_line += wire_out.print(0, &format!("{} {}", g6(v.x), g6(v.y)));
            wire_out.print(0, " ");
            if per_line > 80 {
                wire_out.print(0, "\n");
                per_line = wire_out.print(3, "");
            }
            per_line += wire_out.print(0, &format!("(net {})", quote(&track.net)));
            if per_line > 80 {
                wire_out.print(0, "\n");
                wire_out.print(3, "");
            }
            wire_out.print(0, if track.locked { "(type fix)" } else { "(type route)" });
            wire_out.print(0, ")\n");
        }
        wires.push_str(&wire_out.buf);
    }

    // structure: via list and global rules
    let mut per_line = out.print(2, "(via");
    for via in &vias {
        if per_line > 80 {
            out.print(0, "\n");
            per_line = out.print(3, "");
        }
        per_line += out.print(0, &format!(" {}", quote(&via.id)));
    }
    out.print(0, ")\n");
    let clearance = scale(rules.default.clearance);
    format_rule(
        &mut out,
        2,
        &[
            format!("(width {})", g6(scale(rules.default.track_width))),
            format!("(clearance {})", g6(clearance)),
            format!("(clearance {} (type smd_smd))", g6(clearance / 4.0)),
        ],
    );
    out.print(1, ")\n");

    // ---- placement
    out.print(1, "(placement\n");
    for (image_id, places) in &components {
        out.print(2, &format!("(component {}\n", quote(image_id)));
        for place in places {
            out.print(3, place);
        }
        out.print(2, ")\n");
    }
    out.print(1, ")\n");

    // ---- library
    out.print(1, "(library\n");
    for image in &images {
        image.format(&mut out, 2);
    }
    for padstack in padstack_set.values() {
        padstack.format(&mut out, 2);
    }
    for via in &vias {
        via.format(&mut out, 2);
    }
    out.print(1, ")\n");

    // ---- network
    out.print(1, "(network\n");
    for bogus in &bogus_nets {
        format_net(&mut out, 2, &Net { id: bogus.clone(), pins: Vec::new() });
    }
    for net in nets.iter().filter(|n| !n.pins.is_empty()) {
        format_net(&mut out, 2, net);
    }
    let mut classes: Vec<(&NetClass, String)> = vec![(&rules.default, "kicad_default".to_owned())];
    classes.extend(in_use.iter().map(|c| (*c, c.name.clone())));
    for (class, class_id) in classes {
        let mut per_line = out.print(2, &format!("(class {}", quote(&class_id)));
        for net in names.iter().filter(|n| rules.class_of(n).name == class.name) {
            let mut space = " ";
            if per_line > 72 {
                out.print(0, "\n");
                per_line = out.print(3, "");
                space = "";
            }
            per_line += out.print(0, &format!("{space}{}", quote(net)));
        }
        out.print(0, "\n");
        out.print(3, "(circuit\n");
        let via = make_via(class.via_diameter, class.via_drill, top, bottom, &layer_ids);
        out.print(4, &format!("(use_via \"{}\")\n", via.id));
        out.print(3, ")\n");
        format_rule(&mut out, 3, &class_rules(class));
        out.print(2, ")\n");
    }
    out.print(1, ")\n");

    // ---- wiring
    out.print(1, "(wiring\n");
    out.buf.push_str(&wires);
    out.print(1, ")\n");
    out.print(0, ")\n");
    Ok(out.buf)
}

/// `FOOTPRINT::GetOrientationDegrees`: KiCad keeps footprint angles in (-180, 180].
fn fp_orientation_degrees(angle: f64) -> f64 {
    let mut a = normalize_deg(angle);
    if a > 180.0 {
        a -= 360.0;
    }
    a
}

/// Copper layers of a zone in `LSET` (layer id) order: F.Cu, B.Cu, inner...
fn zone_layers(board: &Board, layers: &[String]) -> Vec<String> {
    let on = board.copper_set(layers);
    let mut names: Vec<(usize, String)> = board
        .copper
        .iter()
        .enumerate()
        .filter(|(i, _)| on[*i])
        .map(|(i, (n, _))| {
            (
                if n == "F.Cu" {
                    0
                } else if n == "B.Cu" {
                    1
                } else {
                    i + 1
                },
                n.clone(),
            )
        })
        .collect();
    names.sort();
    names.into_iter().map(|(_, n)| n).collect()
}

fn format_area(out: &mut Out, nest: usize, kind: &str, name: &str, layer: &str, outlines: &[Vec<P>]) {
    let Some(main) = outlines.first() else { return };
    out.print(nest, &format!("({kind}"));
    if name.is_empty() {
        out.print(0, " \"\"");
    } else {
        out.print(0, &format!(" {}", quote(name)));
    }
    let close = |pts: &[P]| {
        let mut v: Vec<Dp> = pts.iter().map(|&p| map_pt(p)).collect();
        if let Some(&first) = pts.first() {
            v.push(map_pt(first));
        }
        v
    };
    out.print(0, " ");
    Shape::Path { kind: "polygon", layer: layer.into(), aperture: 0.0, points: close(main) }.format(out, 0);
    if outlines.len() > 1 {
        out.print(0, "\n");
        for hole in &outlines[1..] {
            out.print(nest + 1, "(window ");
            Shape::Path { kind: "polygon", layer: layer.into(), aperture: 0.0, points: close(hole) }.format(out, 0);
            out.print(0, ")\n");
        }
        out.print(nest, ")\n");
    } else {
        out.print(0, ")\n");
    }
}

/// A board outline and the holes inside it.
type Outline = (Vec<P>, Vec<Vec<P>>);

/// `BOARD::GetBoardPolygonOutlines`: closed Edge.Cuts contours, outer first,
/// each with the holes it contains.
fn board_outlines(board: &Board) -> Result<Vec<Outline>> {
    let mut contours = Vec::new();
    let mut segments = Vec::new();
    for g in &board.edges {
        match &g.kind {
            GraphicKind::Segment { start, end } | GraphicKind::Arc { start, end, .. } => segments.push((*start, *end)),
            GraphicKind::Rect { start, end } => {
                contours.push(vec![*start, P::new(end.x, start.y), *end, P::new(start.x, end.y)])
            }
            GraphicKind::Circle { center, radius } => {
                contours.push(circle_polyline_split_error(*center, *radius, MAX_ERROR))
            }
            GraphicKind::Poly { points } => contours.push(points.clone()),
            GraphicKind::Other => {}
        }
    }
    contours.extend(chain_contours(&segments, 20_000));
    let mut contours: Vec<Vec<P>> =
        contours.into_iter().filter(|c| c.len() >= 3).map(|c| canonical_order(&c)).collect();
    if contours.is_empty() {
        bail!("board has no closed Edge.Cuts outline");
    }
    contours.sort_by(|a, b| super::clip::area(b).abs().total_cmp(&super::clip::area(a).abs()));
    let outer = contours.remove(0);
    let holes = contours.into_iter().map(|mut h| {
        h.reverse();
        h
    });
    Ok(vec![(outer, holes.collect())])
}
