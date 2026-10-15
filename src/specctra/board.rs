//! The parts of a `.kicad_pcb` the Specctra exporter needs, read into plain
//! structs with geometry already in KiCad's absolute board frame.

use std::collections::HashSet;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

use super::geom::{P, kiround, rotate};
use crate::sexpr::Sexp;

/// KiCad's parser: `KiROUND( strtod( text ) * IU_PER_MM )`.
pub fn mm(node: &Sexp) -> Result<i64> {
    let v = node.as_f64().ok_or_else(|| anyhow!("expected a number, found {node}"))?;
    Ok(kiround(v * 1e6))
}

fn pt(node: &Sexp) -> Result<P> {
    Ok(P::new(mm(node.arg(0).context("missing x")?)?, mm(node.arg(1).context("missing y")?)?))
}

fn opt_angle(node: &Sexp) -> f64 {
    node.arg(2).and_then(Sexp::as_f64).unwrap_or(0.0)
}

fn layers_of(node: &Sexp) -> Vec<String> {
    node.child("layers")
        .map(|l| l.args().iter().filter_map(|a| a.text().map(str::to_owned)).collect())
        .unwrap_or_default()
}

fn is_locked(node: &Sexp) -> bool {
    node.args().iter().any(|a| matches!(a, Sexp::Atom(t) if t == "locked")) || node.value("locked") == Some("yes")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadShape {
    Circle,
    Rect,
    Oval,
    Trapezoid,
    RoundRect,
    ChamferedRect,
    Custom,
}

#[derive(Clone, Debug)]
pub struct Pad {
    pub number: String,
    pub shape: PadShape,
    /// Footprint-local position (unrotated frame), as stored in the file.
    pub local: P,
    /// Board-absolute pad centre.
    pub position: P,
    /// Absolute pad orientation in degrees.
    pub orientation: f64,
    pub size: P,
    pub drill: P,
    pub offset: P,
    pub delta: P,
    pub rratio: f64,
    pub chamfer_ratio: f64,
    pub chamfer_corners: u32,
    /// Copper layer names the pad is on (`*.Cu` expanded later).
    pub layers: Vec<String>,
    pub net: Option<String>,
}

#[derive(Clone, Debug)]
pub enum GraphicKind {
    Segment { start: P, end: P },
    Rect { start: P, end: P },
    Circle { center: P, radius: i64 },
    Arc { start: P, mid: P, end: P },
    Poly { points: Vec<P> },
    Other,
}

#[derive(Clone, Debug)]
pub struct Graphic {
    pub layer: String,
    pub width: i64,
    pub kind: GraphicKind,
}

#[derive(Clone, Debug)]
pub struct Footprint {
    pub lib_id: String,
    pub reference: String,
    pub value: String,
    pub position: P,
    pub orientation: f64,
    pub back: bool,
    pub pads: Vec<Pad>,
    pub graphics: Vec<Graphic>,
}

impl Footprint {
    /// `mapPt( pt, footprint )`: absolute → footprint-local, unrotated.
    pub fn to_local(&self, abs: P) -> P {
        rotate(abs - self.position, -self.orientation)
    }
}

#[derive(Clone, Debug)]
pub enum TrackKind {
    Segment,
    Arc,
    Via { diameter: i64, drill: i64, layers: Vec<String> },
}

#[derive(Clone, Debug)]
pub struct Track {
    pub kind: TrackKind,
    pub start: P,
    pub end: P,
    pub width: i64,
    pub layer: String,
    pub net: String,
    pub locked: bool,
}

#[derive(Clone, Debug)]
pub struct Zone {
    pub net: String,
    pub layers: Vec<String>,
    pub keepout: Option<(bool, bool)>,
    /// Main outline followed by holes, as `IterateWithHoles` walks them.
    pub outlines: Vec<Vec<P>>,
}

#[derive(Clone, Debug)]
pub struct Board {
    /// Copper layers top to bottom, with their Specctra layer type.
    pub copper: Vec<(String, String)>,
    /// Net names in net-code order (code 1 first), as KiCad numbers them on load.
    pub nets: Vec<String>,
    pub footprints: Vec<Footprint>,
    pub tracks: Vec<Track>,
    pub zones: Vec<Zone>,
    pub edges: Vec<Graphic>,
}

impl Board {
    pub fn net_code(&self, name: &str) -> usize {
        self.nets.iter().position(|n| n == name).map_or(0, |i| i + 1)
    }

    pub fn copper_index(&self, layer: &str) -> Option<usize> {
        self.copper.iter().position(|(name, _)| name == layer)
    }

    /// Copper layer indices a pad or via layer list covers.
    pub fn copper_set(&self, layers: &[String]) -> Vec<bool> {
        self.copper
            .iter()
            .map(|(name, _)| {
                layers.iter().any(|l| l == name || l == "*.Cu" || (l == "F&B.Cu" && (name == "F.Cu" || name == "B.Cu")))
            })
            .collect()
    }
}

fn parse_graphic(node: &Sexp, transform: &dyn Fn(P) -> P) -> Result<Graphic> {
    let layer = node.value("layer").unwrap_or_default().to_owned();
    let width = node
        .child("stroke")
        .and_then(|s| s.child("width"))
        .or_else(|| node.child("width"))
        .and_then(|w| w.arg(0))
        .map(mm)
        .transpose()?
        .unwrap_or(0);
    let get =
        |head: &str| -> Result<P> { Ok(transform(pt(node.child(head).with_context(|| format!("{head} missing"))?)?)) };
    let kind = match node.head().unwrap_or_default() {
        "fp_line" | "gr_line" => GraphicKind::Segment { start: get("start")?, end: get("end")? },
        "fp_rect" | "gr_rect" => GraphicKind::Rect { start: get("start")?, end: get("end")? },
        "fp_circle" | "gr_circle" => {
            let center = get("center")?;
            let end = get("end")?;
            let (dx, dy) = ((end.x - center.x) as f64, (end.y - center.y) as f64);
            // EDA_SHAPE::GetRadius truncates the double length to int.
            GraphicKind::Circle { center, radius: dx.hypot(dy) as i64 }
        }
        "fp_arc" | "gr_arc" => GraphicKind::Arc { start: get("start")?, mid: get("mid")?, end: get("end")? },
        "fp_poly" | "gr_poly" => {
            let points = node
                .child("pts")
                .map(|p| p.children("xy").map(|xy| pt(xy).map(transform)).collect::<Result<Vec<_>>>())
                .transpose()?
                .unwrap_or_default();
            GraphicKind::Poly { points }
        }
        _ => GraphicKind::Other,
    };
    Ok(Graphic { layer, width, kind })
}

fn parse_pad(node: &Sexp, fp_pos: P, fp_angle: f64) -> Result<Pad> {
    let number = node.arg(0).and_then(Sexp::text).unwrap_or_default().to_owned();
    let shape = match node.arg(2).and_then(Sexp::text).unwrap_or_default() {
        "circle" => PadShape::Circle,
        "rect" => PadShape::Rect,
        "oval" => PadShape::Oval,
        "trapezoid" => PadShape::Trapezoid,
        "roundrect" => PadShape::RoundRect,
        "chamfered_rect" => PadShape::ChamferedRect,
        _ => PadShape::Custom,
    };
    let at = node.child("at").context("pad without (at)")?;
    let local = pt(at)?;
    let orientation = opt_angle(at);
    let size_node = node.child("size").context("pad without (size)")?;
    let size = pt(size_node)?;
    let (mut drill, mut offset) = (P::default(), P::default());
    if let Some(d) = node.child("drill") {
        let nums: Vec<&Sexp> = d.args().iter().filter(|a| a.as_f64().is_some()).collect();
        let oval = d.args().iter().any(|a| a.text() == Some("oval"));
        if let Some(first) = nums.first() {
            let x = mm(first)?;
            let y = if oval { nums.get(1).map(|n| mm(n)).transpose()?.unwrap_or(x) } else { x };
            drill = P::new(x, y);
        }
        if let Some(o) = d.child("offset") {
            offset = pt(o)?;
        }
    }
    let delta = node.child("rect_delta").map(pt).transpose()?.unwrap_or_default();
    let rratio = node.value("roundrect_rratio").and_then(|v| v.parse().ok()).unwrap_or(0.25);
    let chamfer_ratio = node.value("chamfer_ratio").and_then(|v| v.parse().ok()).unwrap_or(0.2);
    let chamfer_corners = node.child("chamfer").map_or(0, |c| {
        c.args().iter().filter_map(Sexp::text).fold(0, |bits, corner| {
            bits | match corner {
                "top_left" => 1,
                "top_right" => 2,
                "bottom_left" => 4,
                "bottom_right" => 8,
                _ => 0,
            }
        })
    });
    let net = node.child("net").and_then(|n| n.arg(n.args().len().saturating_sub(1))).and_then(Sexp::text);
    Ok(Pad {
        number,
        shape,
        local,
        position: fp_pos + rotate(local, fp_angle),
        orientation,
        size,
        drill,
        offset,
        delta,
        rratio,
        chamfer_ratio,
        chamfer_corners,
        layers: layers_of(node),
        net: net.filter(|n| !n.is_empty()).map(str::to_owned),
    })
}

fn parse_footprint(node: &Sexp) -> Result<Footprint> {
    let lib_id = node.arg(0).and_then(Sexp::text).unwrap_or_default().to_owned();
    let at = node.child("at").context("footprint without (at)")?;
    let position = pt(at)?;
    let orientation = opt_angle(at);
    let property = |key: &str| {
        node.children("property")
            .find(|p| p.arg(0).and_then(Sexp::text) == Some(key))
            .and_then(|p| p.arg(1))
            .and_then(Sexp::text)
            .unwrap_or_default()
            .to_owned()
    };
    let transform = |local: P| position + rotate(local, orientation);
    let mut pads = Vec::new();
    let mut graphics = Vec::new();
    for child in node.args() {
        match child.head() {
            Some("pad") => pads.push(parse_pad(child, position, orientation)?),
            Some(h) if h.starts_with("fp_") && h != "fp_text" => graphics.push(parse_graphic(child, &transform)?),
            _ => {}
        }
    }
    Ok(Footprint {
        lib_id,
        reference: property("Reference"),
        value: property("Value"),
        position,
        orientation,
        back: node.value("layer") == Some("B.Cu"),
        pads,
        graphics,
    })
}

fn parse_track(node: &Sexp) -> Result<Track> {
    let head = node.head().unwrap_or_default();
    let net = node.child("net").and_then(|n| n.arg(n.args().len().saturating_sub(1))).and_then(Sexp::text);
    let net = net.unwrap_or_default().to_owned();
    let locked = is_locked(node);
    if head == "via" {
        let at = pt(node.child("at").context("via without (at)")?)?;
        let diameter = mm(node.child("size").and_then(|s| s.arg(0)).context("via without (size)")?)?;
        let drill = mm(node.child("drill").and_then(|s| s.arg(0)).context("via without (drill)")?)?;
        let layers = layers_of(node);
        return Ok(Track {
            kind: TrackKind::Via { diameter, drill, layers },
            start: at,
            end: at,
            width: diameter,
            layer: String::new(),
            net,
            locked,
        });
    }
    Ok(Track {
        kind: if head == "arc" { TrackKind::Arc } else { TrackKind::Segment },
        start: pt(node.child("start").context("track without (start)")?)?,
        end: pt(node.child("end").context("track without (end)")?)?,
        width: mm(node.child("width").and_then(|w| w.arg(0)).context("track without (width)")?)?,
        layer: node.value("layer").unwrap_or_default().to_owned(),
        net,
        locked,
    })
}

fn parse_zone(node: &Sexp) -> Result<Zone> {
    let net = node
        .child("net_name")
        .or_else(|| node.child("net"))
        .and_then(|n| n.arg(n.args().len().saturating_sub(1)))
        .and_then(Sexp::text)
        .unwrap_or_default()
        .to_owned();
    let mut layers = layers_of(node);
    if let Some(l) = node.value("layer") {
        layers = vec![l.to_owned()];
    }
    let keepout = node
        .child("keepout")
        .map(|k| (k.value("tracks") == Some("not_allowed"), k.value("vias") == Some("not_allowed")));
    let outlines = node
        .children("polygon")
        .map(|poly| {
            poly.child("pts")
                .map(|p| p.children("xy").map(pt).collect::<Result<Vec<_>>>())
                .transpose()
                .map(Option::unwrap_or_default)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Zone { net, layers, keepout, outlines })
}

impl Board {
    pub fn parse(root: &Sexp) -> Result<Self> {
        if !root.is("kicad_pcb") {
            bail!("not a kicad_pcb file");
        }
        let mut copper: Vec<(i64, String, String)> = root
            .child("layers")
            .map(|layers| {
                layers
                    .args()
                    .iter()
                    .filter_map(|l| {
                        let id = l.head()?.parse::<i64>().ok()?;
                        let name = l.arg(0)?.text()?.to_owned();
                        let kind = l.arg(1)?.text()?.to_owned();
                        name.ends_with(".Cu").then_some((id, name, kind))
                    })
                    .collect()
            })
            .unwrap_or_default();
        // KiCad 10 copper ids: F.Cu = 0, B.Cu = 2, inner layers 4, 6, ... top to bottom.
        let rank = |id: i64| {
            if id == 0 {
                -1
            } else if id == 2 {
                i64::MAX
            } else {
                id
            }
        };
        copper.sort_by_key(|(id, ..)| rank(*id));
        let copper = copper.into_iter().map(|(_, name, kind)| (name, kind)).collect();

        let mut nets: Vec<String> = Vec::new();
        let mut seen = HashSet::new();
        collect_nets(root, &mut nets, &mut seen);

        let mut footprints = Vec::new();
        let mut tracks = Vec::new();
        let mut zones = Vec::new();
        let mut edges = Vec::new();
        for item in root.args() {
            match item.head() {
                Some("footprint") => footprints.push(parse_footprint(item)?),
                Some("segment" | "arc" | "via") => tracks.push(parse_track(item)?),
                Some("zone") => zones.push(parse_zone(item)?),
                Some(h) if h.starts_with("gr_") && item.value("layer") == Some("Edge.Cuts") => {
                    edges.push(parse_graphic(item, &|p| p)?)
                }
                _ => {}
            }
        }
        for fp in &footprints {
            edges.extend(fp.graphics.iter().filter(|g| g.layer == "Edge.Cuts").cloned());
        }
        Ok(Self { copper, nets, footprints, tracks, zones, edges })
    }
}

/// Net names in first-appearance order: KiCad 10 boards name nets inline and
/// the loader numbers them as it meets them.
fn collect_nets(node: &Sexp, nets: &mut Vec<String>, seen: &mut HashSet<String>) {
    if node.is("net") || node.is("net_name") {
        if let Some(name) = node.arg(node.args().len().saturating_sub(1)).and_then(Sexp::text)
            && !name.is_empty()
            && seen.insert(name.to_owned())
        {
            nets.push(name.to_owned());
        }
        return;
    }
    if node.is("filled_polygon") {
        return;
    }
    for child in node.args() {
        if child.as_list().is_some() {
            collect_nets(child, nets, seen);
        }
    }
}

/// One net class from the project file, in internal units.
#[derive(Clone, Debug, PartialEq)]
pub struct NetClass {
    pub name: String,
    pub track_width: i64,
    pub clearance: i64,
    pub via_diameter: i64,
    pub via_drill: i64,
}

/// Net classes, their net patterns, and the board rules the exporter uses,
/// read from a `.kicad_pro`.
#[derive(Clone, Debug)]
pub struct ProjectRules {
    pub default: NetClass,
    pub classes: Vec<NetClass>,
    patterns: Vec<(String, String)>,
    /// `min_hole_clearance`, added around holes exported as keepouts.
    pub hole_clearance: i64,
}

impl ProjectRules {
    pub fn from_project(project: &Value) -> Result<Self> {
        let settings = &project["net_settings"];
        let to_nm = |v: &Value| v.as_f64().map(|mm| kiround(mm * 1e6));
        let parse = |c: &Value| -> Option<NetClass> {
            Some(NetClass {
                name: c["name"].as_str()?.to_owned(),
                track_width: to_nm(&c["track_width"])?,
                clearance: to_nm(&c["clearance"])?,
                via_diameter: to_nm(&c["via_diameter"])?,
                via_drill: to_nm(&c["via_drill"])?,
            })
        };
        let mut classes: Vec<NetClass> =
            settings["classes"].as_array().into_iter().flatten().filter_map(parse).collect();
        let default_index =
            classes.iter().position(|c| c.name == "Default").context("project has no Default net class")?;
        let default = classes.remove(default_index);
        let patterns = settings["netclass_patterns"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| Some((p["pattern"].as_str()?.to_owned(), p["netclass"].as_str()?.to_owned())))
            .collect();
        let hole_clearance =
            to_nm(&project["board"]["design_settings"]["rules"]["min_hole_clearance"]).unwrap_or(250_000);
        Ok(Self { default, classes, patterns, hole_clearance })
    }

    /// The net class a net resolves to (first matching pattern wins).
    pub fn class_of(&self, net: &str) -> &NetClass {
        self.patterns
            .iter()
            .find(|(pattern, _)| wildcard_match(pattern, net))
            .and_then(|(_, class)| self.classes.iter().find(|c| &c.name == class))
            .unwrap_or(&self.default)
    }
}

/// KiCad's net-class wildcard patterns: `*` and `?`.
fn wildcard_match(pattern: &str, text: &str) -> bool {
    fn go(p: &[char], t: &[char]) -> bool {
        match (p.first(), t.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], t) || (!t.is_empty() && go(p, &t[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &t[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &t[1..]),
            _ => false,
        }
    }
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    go(&p, &t)
}
