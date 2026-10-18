//! A fast visual preview of a board, drawn straight from the resolved design
//! without KiCad: footprints (pads and silkscreen from the library files),
//! tracks, vias, zone outlines, text, artwork, and a ratsnest of the
//! connections still to route. `bb preview --watch` rebuilds on every save
//! and highlights what the edit changed (see [`svg`]).
//!
//! It is a viewing aid, not an output: zone fills, KiCad's stroke font, and
//! exact reference placement are approximated.

pub mod check;
mod ratsnest;
pub mod svg;

use std::collections::BTreeMap;

use anyhow::Result;

use crate::artwork::ArtworkSource;
use crate::design::{self, Graphic, Layer, Source};
use crate::footprint::FootprintSource;
use crate::resolve::Artwork;
use crate::sexpr::Sexp;
use crate::units::Pt;

/// Board-frame millimetres.
pub type P = (f64, f64);

fn mm(p: Pt) -> P {
    (p.x.to_mm(), p.y.to_mm())
}

/// What a drawn item belongs to, for layer toggles and colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    ZoneBack,
    ZoneFront,
    CopperBack,
    CopperFront,
    Pad,
    Via,
    Fab,
    SilkBack,
    Silk,
    Art,
    Edge,
}

impl Kind {
    fn copper(layer: Layer) -> Self {
        if layer.is_back() { Kind::CopperBack } else { Kind::CopperFront }
    }

    fn graphic(layer: Layer) -> Option<Self> {
        Some(match layer {
            Layer::FCu => Kind::CopperFront,
            Layer::BCu => Kind::CopperBack,
            Layer::FSilk => Kind::Silk,
            Layer::BSilk => Kind::SilkBack,
            Layer::FFab => Kind::Fab,
            Layer::EdgeCuts => Kind::Edge,
            _ => return None,
        })
    }

    /// The CSS class and layer-toggle name.
    pub fn name(self) -> &'static str {
        match self {
            Kind::ZoneBack => "zone-b",
            Kind::ZoneFront => "zone-f",
            Kind::CopperBack => "cu-b",
            Kind::CopperFront => "cu-f",
            Kind::Pad => "pad",
            Kind::Via => "via",
            Kind::Fab => "fab",
            Kind::SilkBack => "silk-b",
            Kind::Silk => "silk-f",
            Kind::Art => "art",
            Kind::Edge => "edge",
        }
    }
}

/// Which copper a pad sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Front,
    Back,
    Both,
}

impl Side {
    fn has(self, layer: Layer) -> bool {
        match self {
            Side::Both => true,
            Side::Front => !layer.is_back(),
            Side::Back => layer.is_back(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PadShape {
    Rect { radius: f64 },
    Circle,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Line { a: P, b: P, width: f64 },
    Poly { points: Vec<P>, fill: bool, width: f64 },
    Circle { center: P, radius: f64, width: f64, fill: bool },
    Arc { start: P, mid: P, end: P, width: f64 },
    Text { text: String, at: P, size: f64, angle: f64, mirrored: bool },
    Pad { number: String, at: P, angle: f64, size: P, shape: PadShape, drill: Option<f64>, side: Side },
    Via { at: P, diameter: f64, drill: f64 },
    Image { href: String, center: P, size: P },
}

/// One drawn thing: its shape, layer, net, and hover label.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub kind: Kind,
    pub net: Option<String>,
    pub label: Option<String>,
    pub shape: Shape,
}

impl Item {
    fn new(kind: Kind, shape: Shape) -> Self {
        Self { kind, net: None, label: None, shape }
    }

    fn net(mut self, net: Option<&str>) -> Self {
        self.net = net.map(str::to_owned);
        self
    }

    fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// A placed footprint: items in its own frame, plus where it sits.
#[derive(Clone, Debug, PartialEq)]
pub struct Footprint {
    pub reference: String,
    pub value: String,
    pub at: P,
    /// Degrees, counter-clockwise on screen.
    pub angle: f64,
    pub items: Vec<Item>,
    /// Footprint-local bounding box, for change highlights.
    pub bounds: (P, P),
    /// Footprint-local courtyard outlines (not drawn), for overlap checks.
    pub courtyard: Vec<(Side, Shape)>,
}

impl Footprint {
    /// A footprint-local point in the board frame.
    pub fn place(&self, (x, y): P) -> P {
        let (s, c) = (-self.angle).to_radians().sin_cos();
        (self.at.0 + x * c - y * s, self.at.1 + x * s + y * c)
    }
}

/// Everything the preview draws.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub title: String,
    pub footprints: Vec<Footprint>,
    pub items: Vec<Item>,
    /// Unrouted connections, per net.
    pub ratsnest: Vec<(String, P, P)>,
    /// Whether tracks came from the routed KiCad board rather than the design.
    pub routed: bool,
    /// What the placement checks found (see [`check`]).
    pub findings: Vec<check::Finding>,
}

impl Scene {
    /// Build the scene for a resolved board titled `title`, drawing library
    /// footprints from `libraries` and artwork from `images`; with `routed`,
    /// the KiCad board file, copper (tracks, vias, zone fills) comes from it.
    pub fn new(
        title: &str,
        design: &design::Board,
        artwork: &[Artwork],
        libraries: &dyn FootprintSource,
        images: &dyn ArtworkSource,
        routed: Option<&Sexp>,
    ) -> Result<Self> {
        let mut scene = Scene { title: title.to_owned(), ..Scene::default() };
        let mut pads = Vec::new();
        for fp in &design.footprints {
            let (footprint, placed) = footprint(fp, libraries)?;
            pads.extend(placed);
            scene.footprints.push(footprint);
        }
        for graphic in &design.graphics {
            if let Some(item) = graphic_item(graphic, |p| p) {
                scene.items.push(item);
            }
        }
        for text in &design.texts {
            let kind = if text.layer.is_back() { Kind::SilkBack } else { Kind::Silk };
            scene.items.push(Item::new(
                kind,
                Shape::Text {
                    text: text.text.clone(),
                    at: mm(text.at),
                    size: text.size.to_mm(),
                    angle: text.angle,
                    mirrored: text.mirrored,
                },
            ));
        }
        for art in artwork {
            if let Some(image) = image_item(art, images) {
                scene.items.push(image);
            }
        }
        let copper = match routed {
            Some(pcb) => {
                scene.routed = true;
                Copper::from_pcb(pcb)
            }
            None => Copper::from_design(design),
        };
        for zone in &design.zones {
            let kind = if zone.layer.is_back() { Kind::ZoneBack } else { Kind::ZoneFront };
            let points = zone.outline.iter().map(|p| mm(*p)).collect();
            let label = format!("zone {} on {}", zone.net, zone.layer.name());
            scene.items.push(
                Item::new(kind, Shape::Poly { points, fill: false, width: 0.15 }).net(Some(&zone.net)).label(label),
            );
        }
        for (layer, net, points) in &copper.fills {
            let kind = if layer.is_back() { Kind::ZoneBack } else { Kind::ZoneFront };
            scene
                .items
                .push(Item::new(kind, Shape::Poly { points: points.clone(), fill: true, width: 0.0 }).net(Some(net)));
        }
        for t in &copper.tracks {
            let label = format!("{} · {:.2} mm · {}", t.net, t.width, t.layer.name());
            scene.items.push(
                Item::new(Kind::copper(t.layer), Shape::Line { a: t.a, b: t.b, width: t.width })
                    .net(Some(&t.net))
                    .label(label),
            );
        }
        for v in &copper.vias {
            let label = format!("via {} · {:.2}/{:.2}", v.net, v.diameter, v.drill);
            scene.items.push(
                Item::new(Kind::Via, Shape::Via { at: v.at, diameter: v.diameter, drill: v.drill })
                    .net(Some(&v.net))
                    .label(label),
            );
        }
        // With KiCad's fills present, the zones themselves connect; without
        // them, a zone's outline stands in for its fill.
        let zones: Vec<_> = if routed.is_some() {
            copper.fills.clone()
        } else {
            design.zones.iter().map(|z| (z.layer, z.net.clone(), z.outline.iter().map(|p| mm(*p)).collect())).collect()
        };
        scene.ratsnest = ratsnest::unrouted(&pads, &copper.tracks, &copper.vias, &zones);
        scene.findings = check::check(&scene, &design.rule_severities);
        Ok(scene)
    }

    /// Footprints by reference, for change detection.
    fn placements(&self) -> BTreeMap<&str, &Footprint> {
        self.footprints.iter().map(|fp| (fp.reference.as_str(), fp)).collect()
    }
}

/// A pad in the board frame, for connectivity.
#[derive(Clone, Debug)]
pub struct PlacedPad {
    pub reference: String,
    pub number: String,
    pub net: String,
    pub at: P,
    /// Degrees, counter-clockwise on screen.
    pub angle: f64,
    pub size: P,
    pub circle: bool,
    pub side: Side,
}

#[derive(Clone, Debug)]
pub struct Track {
    pub net: String,
    pub layer: Layer,
    pub a: P,
    pub b: P,
    pub width: f64,
}

#[derive(Clone, Debug)]
pub struct Via {
    pub net: String,
    pub at: P,
    pub diameter: f64,
    pub drill: f64,
}

/// Copper to draw and connect: from the design, or from a KiCad board.
struct Copper {
    tracks: Vec<Track>,
    vias: Vec<Via>,
    fills: Vec<(Layer, String, Vec<P>)>,
}

impl Copper {
    fn from_design(design: &design::Board) -> Self {
        Copper {
            tracks: design
                .tracks
                .iter()
                .map(|t| Track {
                    net: t.net.clone(),
                    layer: t.layer,
                    a: mm(t.start),
                    b: mm(t.end),
                    width: t.width.to_mm(),
                })
                .collect(),
            vias: design
                .vias
                .iter()
                .map(|v| Via { net: v.net.clone(), at: mm(v.at), diameter: v.diameter.to_mm(), drill: v.drill.to_mm() })
                .collect(),
            fills: Vec::new(),
        }
    }

    fn from_pcb(pcb: &Sexp) -> Self {
        let net = |node: &Sexp| node.child("net").and_then(|n| n.arg(0)).and_then(Sexp::text).unwrap_or("").to_owned();
        let layer = |node: &Sexp| node.value("layer").and_then(Layer::parse);
        let mut copper = Copper { tracks: Vec::new(), vias: Vec::new(), fills: Vec::new() };
        for seg in pcb.children("segment") {
            if let (Some(a), Some(b), Some(layer)) = (xy(seg.child("start")), xy(seg.child("end")), layer(seg)) {
                let width = num(seg, "width").unwrap_or(0.2);
                copper.tracks.push(Track { net: net(seg), layer, a, b, width });
            }
        }
        for via in pcb.children("via") {
            if let Some(at) = xy(via.child("at")) {
                let diameter = num(via, "size").unwrap_or(0.6);
                let drill = num(via, "drill").unwrap_or(0.3);
                copper.vias.push(Via { net: net(via), at, diameter, drill });
            }
        }
        for zone in pcb.children("zone") {
            let zone_net = net(zone);
            for fill in zone.children("filled_polygon") {
                let (Some(layer), Some(pts)) = (layer(fill), fill.child("pts")) else { continue };
                let points = pts.children("xy").filter_map(|p| xy(Some(p))).collect();
                copper.fills.push((layer, zone_net.clone(), points));
            }
        }
        copper
    }
}

fn xy(node: Option<&Sexp>) -> Option<P> {
    let node = node?;
    Some((node.arg(0)?.as_f64()?, node.arg(1)?.as_f64()?))
}

fn num(node: &Sexp, head: &str) -> Option<f64> {
    node.child(head)?.arg(0)?.as_f64()
}

/// A graphic in the board frame, or in a footprint frame via `place`.
fn graphic_item(graphic: &Graphic, place: impl Fn(P) -> P) -> Option<Item> {
    Some(match graphic {
        Graphic::Line { layer, start, end, width } => Item::new(
            Kind::graphic(*layer)?,
            Shape::Line { a: place(mm(*start)), b: place(mm(*end)), width: width.to_mm() },
        ),
        Graphic::Poly { layer, points } => Item::new(
            Kind::graphic(*layer)?,
            Shape::Poly { points: points.iter().map(|p| place(mm(*p))).collect(), fill: true, width: 0.0 },
        ),
    })
}

/// A footprint's local drawing and its board-frame pads.
fn footprint(fp: &design::Footprint, libraries: &dyn FootprintSource) -> Result<(Footprint, Vec<PlacedPad>)> {
    let at = mm(fp.at);
    let mut items = Vec::new();
    let mut courtyard = Vec::new();
    let mut pads = Vec::new();
    let mut place_pad = |number: &str, local: P, angle: f64, size: P, circle: bool, side: Side, net: Option<&str>| {
        if let Some(net) = net {
            let (s, c) = (-fp.angle).to_radians().sin_cos();
            let board = (at.0 + local.0 * c - local.1 * s, at.1 + local.0 * s + local.1 * c);
            pads.push(PlacedPad {
                reference: fp.reference.clone(),
                number: number.to_owned(),
                net: net.to_owned(),
                at: board,
                angle: fp.angle + angle,
                size,
                circle,
                side,
            });
        }
    };
    match &fp.source {
        Source::Library(lib) => {
            let node = libraries.load(&lib.library, &lib.name)?;
            for pad in node.children("pad") {
                let number = pad.arg(0).and_then(Sexp::text).unwrap_or("");
                let kind = pad.arg(1).and_then(Sexp::text).unwrap_or("smd");
                let shape = pad.arg(2).and_then(Sexp::text).unwrap_or("rect");
                let Some(local) = xy(pad.child("at")) else { continue };
                let angle = pad.child("at").and_then(|a| a.arg(2)).and_then(Sexp::as_f64).unwrap_or(0.0);
                let size = xy(pad.child("size")).unwrap_or((0.0, 0.0));
                let drill = pad.child("drill").and_then(|d| d.args().iter().find_map(Sexp::as_f64));
                let layers: Vec<&str> =
                    pad.child("layers").map_or(Vec::new(), |l| l.args().iter().filter_map(Sexp::text).collect());
                let side = if layers.iter().any(|l| l.starts_with('*')) {
                    Side::Both
                } else if layers.contains(&"B.Cu") {
                    Side::Back
                } else {
                    Side::Front
                };
                let net = lib.pins.iter().find(|(pad, _)| pad == number).map(|(_, net)| net.as_str());
                let pad_shape = match shape {
                    "circle" => PadShape::Circle,
                    "oval" => PadShape::Rect { radius: size.0.min(size.1) / 2.0 },
                    "roundrect" => {
                        PadShape::Rect { radius: num(pad, "roundrect_rratio").unwrap_or(0.25) * size.0.min(size.1) }
                    }
                    _ => PadShape::Rect { radius: 0.0 },
                };
                if kind == "np_thru_hole" {
                    items.push(Item::new(
                        Kind::Pad,
                        Shape::Circle { center: local, radius: drill.unwrap_or(size.0) / 2.0, width: 0.0, fill: true },
                    ));
                    continue;
                }
                let label = match net {
                    Some(net) => format!("{} pad {number} · {net}", fp.reference),
                    None => format!("{} pad {number}", fp.reference),
                };
                items.push(
                    Item::new(
                        Kind::Pad,
                        Shape::Pad {
                            number: number.to_owned(),
                            at: local,
                            angle,
                            size,
                            shape: pad_shape.clone(),
                            drill,
                            side,
                        },
                    )
                    .net(net)
                    .label(label),
                );
                place_pad(number, local, angle, size, pad_shape == PadShape::Circle, side, net);
            }
            for child in node.args() {
                match library_shape(child) {
                    Some(("F.CrtYd" | "F.Courtyard", shape)) => courtyard.push((Side::Front, shape)),
                    Some(("B.CrtYd" | "B.Courtyard", shape)) => courtyard.push((Side::Back, shape)),
                    _ => items.extend(library_graphic(child)),
                }
            }
            if !fp.hide_reference
                && let Some(prop) =
                    node.children("property").find(|p| p.arg(0).and_then(Sexp::text) == Some("Reference"))
                && let Some(local) = xy(prop.child("at"))
            {
                let size = prop
                    .child("effects")
                    .and_then(|e| e.child("font"))
                    .and_then(|f| xy(f.child("size")))
                    .map_or(1.0, |s| s.1);
                let back = prop.value("layer").is_some_and(|l| l.starts_with("B."));
                let kind = if back { Kind::SilkBack } else { Kind::Silk };
                items.push(Item::new(
                    kind,
                    Shape::Text {
                        text: fp.reference.clone(),
                        at: local,
                        size,
                        angle: readable(fp.angle),
                        mirrored: back,
                    },
                ));
            }
        }
        Source::Generated(generated) => {
            for pad in &generated.pads {
                let local = mm(pad.at);
                let size = (pad.size.0.to_mm(), pad.size.1.to_mm());
                let circle = pad.shape == design::PadShape::Circle;
                let side = if pad.drill.is_some() { Side::Both } else { Side::Front };
                let shape = if circle { PadShape::Circle } else { PadShape::Rect { radius: 0.0 } };
                let label = match &pad.net {
                    Some(net) => format!("{} pad {} · {net}", fp.reference, pad.number),
                    None => format!("{} pad {}", fp.reference, pad.number),
                };
                items.push(
                    Item::new(
                        Kind::Pad,
                        Shape::Pad {
                            number: pad.number.clone(),
                            at: local,
                            angle: 0.0,
                            size,
                            shape,
                            drill: pad.drill.map(|d| d.to_mm()),
                            side,
                        },
                    )
                    .net(pad.net.as_deref())
                    .label(label),
                );
                place_pad(&pad.number, local, 0.0, size, circle, side, pad.net.as_deref());
            }
            for graphic in &generated.graphics {
                if let Some(item) = graphic_item(graphic, |p| p) {
                    items.push(item);
                }
                if let Some(c) = generated_courtyard(graphic) {
                    courtyard.push(c);
                }
            }
            if !fp.hide_reference {
                items.push(Item::new(
                    Kind::Silk,
                    Shape::Text {
                        text: fp.reference.clone(),
                        at: (0.0, 0.0),
                        size: 0.8,
                        angle: readable(fp.angle),
                        mirrored: false,
                    },
                ));
            }
        }
    }
    let bounds = bounds(&items);
    let footprint = Footprint {
        reference: fp.reference.clone(),
        value: fp.value.clone(),
        at,
        angle: fp.angle,
        items,
        bounds,
        courtyard,
    };
    Ok((footprint, pads))
}

/// A text angle, relative to its footprint, that reads level on the board.
fn readable(footprint_angle: f64) -> f64 {
    -footprint_angle
}

/// A generated footprint's courtyard graphic, footprint-local.
fn generated_courtyard(graphic: &Graphic) -> Option<(Side, Shape)> {
    let (Graphic::Line { layer, .. } | Graphic::Poly { layer, .. }) = graphic;
    let side = match layer {
        Layer::FCourtyard => Side::Front,
        Layer::BCourtyard => Side::Back,
        _ => return None,
    };
    let shape = match graphic {
        Graphic::Line { start, end, width, .. } => Shape::Line { a: mm(*start), b: mm(*end), width: width.to_mm() },
        Graphic::Poly { points, .. } => {
            Shape::Poly { points: points.iter().map(|p| mm(*p)).collect(), fill: false, width: 0.0 }
        }
    };
    Some((side, shape))
}

/// Silkscreen and fab graphics from a library footprint.
fn library_graphic(node: &Sexp) -> Option<Item> {
    let (layer, shape) = library_shape(node)?;
    let kind = match layer {
        "F.SilkS" | "F.Silkscreen" => Kind::Silk,
        "B.SilkS" | "B.Silkscreen" => Kind::SilkBack,
        "F.Fab" => Kind::Fab,
        _ => return None,
    };
    Some(Item::new(kind, shape))
}

/// A library footprint's graphic and the layer it is on.
fn library_shape(node: &Sexp) -> Option<(&str, Shape)> {
    let layer = node.value("layer")?;
    let width = node.child("stroke").and_then(|s| num(s, "width")).or_else(|| num(node, "width")).unwrap_or(0.12);
    let fill =
        node.child("fill").and_then(|f| f.arg(0)).and_then(Sexp::text).is_some_and(|f| f == "yes" || f == "solid");
    let shape = match node.head()? {
        "fp_line" => Shape::Line { a: xy(node.child("start"))?, b: xy(node.child("end"))?, width },
        "fp_rect" => {
            let ((x1, y1), (x2, y2)) = (xy(node.child("start"))?, xy(node.child("end"))?);
            Shape::Poly { points: vec![(x1, y1), (x2, y1), (x2, y2), (x1, y2)], fill, width }
        }
        "fp_circle" => {
            let (center, end) = (xy(node.child("center"))?, xy(node.child("end"))?);
            Shape::Circle { center, radius: (end.0 - center.0).hypot(end.1 - center.1), width, fill }
        }
        "fp_arc" => Shape::Arc {
            start: xy(node.child("start"))?,
            mid: xy(node.child("mid"))?,
            end: xy(node.child("end"))?,
            width,
        },
        "fp_poly" => {
            let points = node.child("pts")?.children("xy").filter_map(|p| xy(Some(p))).collect();
            Shape::Poly { points, fill, width }
        }
        _ => return None,
    };
    Some((layer, shape))
}

/// The local bounding box of a footprint's pads and graphics.
fn bounds(items: &[Item]) -> (P, P) {
    let mut lo = (f64::INFINITY, f64::INFINITY);
    let mut hi = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut grow = |(x, y): P, r: f64| {
        lo = (lo.0.min(x - r), lo.1.min(y - r));
        hi = (hi.0.max(x + r), hi.1.max(y + r));
    };
    for item in items {
        match &item.shape {
            Shape::Line { a, b, .. } => {
                grow(*a, 0.0);
                grow(*b, 0.0);
            }
            Shape::Poly { points, .. } => points.iter().for_each(|p| grow(*p, 0.0)),
            Shape::Circle { center, radius, .. } => grow(*center, *radius),
            Shape::Arc { start, mid, end, .. } => [start, mid, end].into_iter().for_each(|p| grow(*p, 0.0)),
            Shape::Pad { at, size, .. } => grow(*at, size.0.max(size.1) / 2.0),
            Shape::Text { .. } | Shape::Via { .. } | Shape::Image { .. } => {}
        }
    }
    if lo.0.is_finite() { (lo, hi) } else { ((-0.5, -0.5), (0.5, 0.5)) }
}

/// Artwork shows as its source image, scaled to its silkscreen width.
fn image_item(art: &Artwork, images: &dyn ArtworkSource) -> Option<Item> {
    let image = images.image(&art.image)?;
    let (w, h) = (f64::from(image.width), f64::from(image.height));
    let size = (art.width, art.width * h / w);
    let href = format!("data:image/png;base64,{}", base64(&image.png));
    let kind = if art.layer.is_back() { Kind::SilkBack } else { Kind::Art };
    Some(Item::new(kind, Shape::Image { href, center: art.center, size }).label(art.reference.clone()))
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> (18 - 6 * i)) as usize & 63]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn reference_text_reads_level() {
        for angle in [0.0, 90.0, 180.0, 270.0] {
            assert_eq!(angle + readable(angle), 0.0);
        }
    }
}
