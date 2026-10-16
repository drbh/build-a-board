//! Resolve a board component's design into a [`design::Board`].
//!
//! Resolution is order-independent: every part is placed first (composing
//! frames), then pin anchors are resolved against the placed pads, then the
//! netlist is validated. Every problem is collected and reported together.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use anyhow::{Context as _, Result, anyhow, bail};

use super::runtime::wit;
use super::{Artwork, Project, Restore, SchematicConfig};
use crate::design::{self, Footprint, GeneratedFootprint, Graphic, Layer, LibraryFootprint, Model, ModelEdit, Source};
use crate::kicad::Libraries;
use crate::units::{Nm, Pt};

/// Everything a design resolves to.
pub struct Resolved {
    pub design: design::Board,
    pub project: Project,
    pub artwork: Vec<Artwork>,
    pub restore: Vec<Restore>,
    pub unlink_after_route: Option<String>,
    pub router_clearance: Option<f64>,
    /// Nets connected to fewer than two pads.
    pub warnings: Vec<String>,
}

/// A placement frame: items inside a group are offset and rotated by it.
#[derive(Clone, Copy, Debug, Default)]
struct Frame {
    x: f64,
    y: f64,
    /// Degrees, counter-clockwise on screen (KiCad's footprint convention).
    angle: f64,
}

impl Frame {
    /// Map a point from this frame into the board frame (mm, unrounded).
    fn apply(self, p: &wit::Point) -> (f64, f64) {
        let (x, y) = self.turn(p);
        (self.x + x, self.y + y)
    }

    /// Rotate a vector by this frame's angle; right angles are exact.
    fn turn(self, p: &wit::Point) -> (f64, f64) {
        match self.angle.rem_euclid(360.0) {
            0.0 => (p.x, p.y),
            90.0 => (p.y, -p.x),
            180.0 => (-p.x, -p.y),
            270.0 => (-p.y, p.x),
            angle => {
                let (s, c) = angle.to_radians().sin_cos();
                (p.x * c + p.y * s, -p.x * s + p.y * c)
            }
        }
    }

    /// Map a board-frame point back into this frame.
    fn unapply(self, (x, y): (f64, f64)) -> (f64, f64) {
        Frame { x: 0.0, y: 0.0, angle: -self.angle }.turn(&wit::Point { x: x - self.x, y: y - self.y })
    }

    fn compose(self, local: &wit::Frame) -> Frame {
        let (x, y) = self.apply(&local.at);
        Frame { x, y, angle: self.angle + local.rotate }
    }
}

/// Absolute frames, in table order (parents always precede children).
fn frames(table: &[wit::Frame]) -> Result<Vec<Frame>> {
    let mut resolved: Vec<Frame> = Vec::with_capacity(table.len());
    for (index, frame) in table.iter().enumerate() {
        let parent = match frame.parent {
            None => Frame::default(),
            Some(p) if (p as usize) < index => resolved[p as usize],
            Some(p) => bail!("frame {index} names parent {p}, which does not precede it"),
        };
        resolved.push(parent.compose(frame));
    }
    Ok(resolved)
}

/// A point as written: an exact pad anchor, a pad plus a board-frame offset,
/// or board-frame millimetres.
#[derive(Clone, Debug)]
enum Point {
    Pin(wit::PinRef),
    Offset(wit::PinRef, Pt),
    Mm(f64, f64),
}

fn point(anchor: &wit::Anchor, frame: Frame) -> Point {
    match anchor {
        wit::Anchor::Pin(pin) => Point::Pin(pin.clone()),
        wit::Anchor::Offset(o) => {
            let (dx, dy) = frame.turn(&o.by);
            Point::Offset(o.pad.clone(), Pt::mm(dx, dy))
        }
        wit::Anchor::Mm(p) => {
            let (x, y) = frame.apply(p);
            Point::Mm(x, y)
        }
    }
}

fn points(anchors: &[wit::Anchor], frame: Frame) -> Vec<Point> {
    anchors.iter().map(|a| point(a, frame)).collect()
}

fn layer(name: &str) -> Result<Layer> {
    Layer::parse(name).ok_or_else(|| anyhow!("unknown layer {name:?}"))
}

fn len(mm: f64) -> Nm {
    Nm::from_design_mm(mm)
}

fn lib_id(text: &str) -> Result<(String, String)> {
    text.split_once(':')
        .map(|(library, name)| (library.to_owned(), name.to_owned()))
        .ok_or_else(|| anyhow!("footprint {text:?} must be written Library:Name"))
}

/// Offsets inside a generated footprint resolve like pcbnew's `SetPosition`:
/// round the absolute position, then take the difference from the origin.
fn local((ox, oy): (f64, f64), d: &wit::Point) -> Pt {
    Pt::new(len(ox + d.x) - len(ox), len(oy + d.y) - len(oy))
}

fn models(models: &[wit::Model]) -> Vec<Model> {
    models.iter().map(|m| Model { path: m.path.clone(), offset: [m.offset.0, m.offset.1, m.offset.2] }).collect()
}

fn rules(r: &wit::Rules) -> design::Rules {
    design::Rules {
        track_width: len(r.track_width),
        clearance: len(r.clearance),
        via_diameter: len(r.via_diameter),
        via_drill: len(r.via_drill),
    }
}

/// Items waiting for anchors to resolve.
enum Pending {
    Track {
        net: String,
        layer: Layer,
        width: Nm,
        points: Vec<Point>,
        keep: wit::Keep,
    },
    Via {
        net: String,
        at: Point,
        diameter: Nm,
        drill: Nm,
    },
    Zone {
        zone: design::Zone,
        outline: Vec<Point>,
    },
    Outline {
        layer: Layer,
        width: Nm,
        points: Vec<Point>,
        closed: bool,
    },
    Legends {
        reference: String,
        legends: Vec<(String, String)>,
        row: bool,
        at: f64,
        size: Nm,
        angle: f64,
        layer: Layer,
        frame: Frame,
    },
}

struct Collector<'a> {
    libraries: &'a Libraries,
    dir: &'a Path,
    hide_prefixes: &'a [String],
    board: design::Board,
    pending: Vec<Pending>,
    artwork: Vec<Artwork>,
}

impl Collector<'_> {
    fn hidden(&self, explicit: bool, reference: &str) -> bool {
        explicit || self.hide_prefixes.iter().any(|p| reference.starts_with(p.as_str()))
    }

    fn item(&mut self, shape: &wit::Shape, frame: Frame) -> Result<()> {
        match shape {
            wit::Shape::Placement(p) => {
                let (x, y) = frame.apply(&p.at);
                let angle = frame.angle + p.rotate;
                let (reference, value, source) = match &p.footprint {
                    wit::Footprint::Library(part) => {
                        let (library, name) = lib_id(&part.footprint)?;
                        self.libraries.load(&library, &name)?;
                        let models = match &part.models {
                            wit::Models::Keep => ModelEdit::Keep,
                            wit::Models::Swap(path) => ModelEdit::RenameFirst(path.clone()),
                            wit::Models::Replace(list) => ModelEdit::Replace(models(list)),
                        };
                        let pins = part.pins.clone();
                        (
                            &part.reference,
                            &part.value,
                            Source::Library(LibraryFootprint { library, name, pins, models }),
                        )
                    }
                    wit::Footprint::Generated(g) => (&g.reference, &g.value, Source::Generated(generated(g, (x, y))?)),
                };
                self.board.footprints.push(Footprint {
                    hide_reference: self.hidden(p.hide_reference, reference),
                    reference: reference.clone(),
                    value: value.clone(),
                    at: Pt::mm(x, y),
                    angle,
                    source,
                });
            }
            wit::Shape::Track(t) => self.pending.push(Pending::Track {
                net: t.net.clone(),
                layer: layer(&t.layer)?,
                width: len(t.width),
                points: points(&t.points, frame),
                keep: t.keep,
            }),
            wit::Shape::Via(v) => self.pending.push(Pending::Via {
                net: v.net.clone(),
                at: point(&v.at, frame),
                diameter: len(v.diameter),
                drill: len(v.drill),
            }),
            wit::Shape::Zone(z) => self.pending.push(Pending::Zone {
                zone: design::Zone {
                    net: z.net.clone(),
                    layer: layer(&z.layer)?,
                    outline: Vec::new(),
                    priority: z.priority,
                    clearance: len(z.clearance),
                    min_thickness: len(z.min_thickness),
                },
                outline: points(&z.outline, frame),
            }),
            wit::Shape::Text(t) => {
                let layer = layer(&t.layer)?;
                let (x, y) = frame.apply(&t.at);
                self.board.texts.push(design::Text {
                    text: t.text.clone(),
                    at: Pt::mm(x, y),
                    layer,
                    size: len(t.size),
                    thickness: len(t.thickness),
                    angle: frame.angle + t.rotate,
                    mirrored: layer.is_back(),
                });
            }
            wit::Shape::Outline(o) => self.pending.push(Pending::Outline {
                layer: layer(&o.layer)?,
                width: len(o.width),
                points: points(&o.points, frame),
                closed: o.closed,
            }),
            wit::Shape::PinLegends(l) => self.pending.push(Pending::Legends {
                reference: l.reference.clone(),
                legends: l.legends.clone(),
                row: matches!(l.axis, wit::Axis::Row),
                at: l.at,
                size: len(l.size),
                angle: l.rotate,
                layer: layer(&l.layer)?,
                frame,
            }),
            wit::Shape::Artwork(a) => {
                let image = self.dir.join(&a.image);
                if !image.is_file() {
                    bail!("artwork image not found: {}", image.display());
                }
                let (library, name) =
                    lib_id(&a.footprint.clone().unwrap_or_else(|| format!("Artwork:{}", a.reference)))?;
                let file_name = image.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.artwork.push(Artwork {
                    reference: a.reference.clone(),
                    value: a.value.clone().unwrap_or(file_name),
                    library,
                    name,
                    image,
                    center: frame.apply(&a.center),
                    width: a.width,
                    layer: layer(&a.layer)?,
                    tone: match a.tone {
                        wit::Tone::Light => crate::artwork::Tone::Light,
                        wit::Tone::Dark => crate::artwork::Tone::Dark,
                    },
                });
            }
        }
        Ok(())
    }
}

/// A generated footprint's pads and graphics, relative to its board-frame origin.
fn generated(g: &wit::Generated, origin: (f64, f64)) -> Result<GeneratedFootprint> {
    let (library, name) = lib_id(&g.footprint)?;
    let mut generated = GeneratedFootprint { library, name, ..Default::default() };
    for pad in &g.pads {
        generated.pads.push(design::Pad {
            number: pad.number.clone(),
            at: local(origin, &pad.at),
            size: (len(pad.size.x), len(pad.size.y)),
            drill: pad.drill.map(len),
            shape: match pad.shape {
                wit::PadShape::Rect => design::PadShape::Rect,
                wit::PadShape::Circle => design::PadShape::Circle,
                wit::PadShape::Oval => design::PadShape::Oval,
                wit::PadShape::Roundrect => design::PadShape::Roundrect,
            },
            net: pad.net.clone(),
        });
    }
    if let Some(body) = &g.body {
        for (layer, margin, stroke) in [(Layer::FFab, 0.0, 0.1), (Layer::FCourtyard, 0.25, 0.05)] {
            let (w, h) = (body.x + 2.0 * margin, body.y + 2.0 * margin);
            let corners = [(-w / 2.0, -h / 2.0), (w / 2.0, -h / 2.0), (w / 2.0, h / 2.0), (-w / 2.0, h / 2.0)]
                .map(|(x, y)| wit::Point { x, y });
            for i in 0..4 {
                generated.graphics.push(Graphic::Line {
                    layer,
                    start: local(origin, &corners[i]),
                    end: local(origin, &corners[(i + 1) % 4]),
                    width: len(stroke),
                });
            }
        }
    }
    for line in &g.graphics {
        generated.graphics.push(Graphic::Line {
            layer: layer(&line.layer)?,
            start: local(origin, &line.start),
            end: local(origin, &line.end),
            width: len(line.width),
        });
    }
    generated.models = models(&g.models);
    Ok(generated)
}

fn label(shape: &wit::Shape) -> String {
    match shape {
        wit::Shape::Placement(p) => match &p.footprint {
            wit::Footprint::Library(part) => format!("part {}", part.reference),
            wit::Footprint::Generated(g) => format!("part {}", g.reference),
        },
        wit::Shape::Track(t) => format!("track on {}", t.net),
        wit::Shape::Via(v) => format!("via on {}", v.net),
        wit::Shape::Zone(z) => format!("zone on {}", z.net),
        wit::Shape::Text(t) => format!("text {:?}", t.text),
        wit::Shape::Outline(o) => format!("outline on {}", o.layer),
        wit::Shape::PinLegends(l) => format!("pin legends for {}", l.reference),
        wit::Shape::Artwork(a) => format!("artwork {}", a.reference),
    }
}

/// Board-frame pad centres of every placed footprint, in pad order.
fn pad_map(board: &design::Board, libraries: &Libraries) -> Result<HashMap<String, Vec<(String, Pt)>>> {
    let mut pads = HashMap::new();
    for fp in &board.footprints {
        let placed = match &fp.source {
            Source::Library(lib) => libraries.pad_positions(&lib.library, &lib.name, fp.at, fp.angle)?,
            Source::Generated(generated) => {
                generated.pads.iter().map(|pad| (pad.number.clone(), fp.at + pad.at.rotate(fp.angle))).collect()
            }
        };
        if pads.insert(fp.reference.clone(), placed).is_some() {
            bail!("duplicate reference {}", fp.reference);
        }
    }
    Ok(pads)
}

fn project(p: wit::Project) -> Project {
    let wit::Schematic { title, company, revision, comment, library } = p.schematic;
    Project {
        schematic: SchematicConfig {
            title: title.unwrap_or_else(|| p.name.clone()),
            company,
            revision: revision.unwrap_or_else(|| "1.0".into()),
            comment,
            library: library.unwrap_or_else(|| p.name.clone()),
        },
        bom: p.bom,
        lcsc: p.lcsc,
        order_quantity: p.order_quantity,
        gerber_layers: p.gerber_layers,
        unsourced: p.unsourced.unwrap_or_else(|| ["H", "LOGO", "TP", "FID"].map(String::from).into()),
        checks: p.checks,
        release_check: p.release_check,
        contract: p.contract,
        release_files: p.release_files,
        kicad_dir: p.kicad_dir,
        copper_weight_oz: p.copper_weight_oz,
        decisions: p.decisions,
        name: p.name,
    }
}

/// Resolve a design returned by a board component; `dir` is where its
/// artwork lives.
pub fn resolve(design: wit::Design, libraries: &Libraries, dir: &Path) -> Result<Resolved> {
    let frames = frames(&design.frames)?;
    let mut collector = Collector {
        libraries,
        dir,
        hide_prefixes: &design.hide_references,
        board: design::Board {
            copper_layers: u8::try_from(design.copper_layers).context("copper layer count")?,
            rules: design.rules.as_ref().map(rules),
            rule_severities: design.severities.clone(),
            finish: design.finish.as_ref().map(|f| design::Finish {
                mask: f.mask.clone(),
                silk: f.silk.clone(),
                copper: f.copper.clone(),
            }),
            netclasses: design
                .netclasses
                .iter()
                .map(|c| design::NetClass { name: c.name.clone(), rules: rules(&c.rules), nets: c.nets.clone() })
                .collect(),
            ..Default::default()
        },
        pending: Vec::new(),
        artwork: Vec::new(),
    };

    // Collect every problem rather than stopping at the first, so one run
    // reports all broken anchors and missing parts.
    let mut errors: Vec<String> = Vec::new();
    for item in &design.items {
        let frame = match item.frame {
            None => Frame::default(),
            Some(i) => *frames.get(i as usize).ok_or_else(|| anyhow!("item names missing frame {i}"))?,
        };
        if let Err(err) = collector.item(&item.shape, frame) {
            errors.push(format!("{}: {err:#}", label(&item.shape)));
        }
    }

    // Anchors resolve against every placed pad, wherever it was declared.
    let pads = pad_map(&collector.board, libraries)?;
    let pad = |pin: &wit::PinRef| -> Result<Pt> {
        pads.get(&pin.reference)
            .ok_or_else(|| anyhow!("pin({}, {}): there is no part {}", pin.reference, pin.pad, pin.reference))?
            .iter()
            .find(|(number, _)| *number == pin.pad)
            .map(|(_, at)| *at)
            .ok_or_else(|| anyhow!("pin({}, {}): {} has no pad {}", pin.reference, pin.pad, pin.reference, pin.pad))
    };
    let resolve = |p: &Point| -> Result<Pt> {
        match p {
            Point::Mm(x, y) => Ok(Pt::mm(*x, *y)),
            Point::Pin(pin) => pad(pin),
            Point::Offset(pin, by) => Ok(pad(pin)? + *by),
        }
    };
    let all = |points: &[Point]| points.iter().map(&resolve).collect::<Result<Vec<_>>>();
    let mut board = std::mem::take(&mut collector.board);
    let mut restore = Vec::new();
    for pending in collector.pending {
        let outcome: Result<()> = match pending {
            Pending::Track { net, layer, width, points, keep } => all(&points)
                .map(|points| {
                    for pair in points.windows(2) {
                        let (start, end) = (pair[0], pair[1]);
                        if keep != wit::Keep::Routed {
                            board.tracks.push(design::Track { net: net.clone(), layer, width, start, end });
                        }
                        if keep != wit::Keep::No {
                            restore.push(Restore { net: net.clone(), layer, start, end, width });
                        }
                    }
                })
                .with_context(|| format!("track on {net}")),
            Pending::Via { net, at, diameter, drill } => resolve(&at)
                .map(|at| board.vias.push(design::Via { net: net.clone(), at, diameter, drill }))
                .with_context(|| format!("via on {net}")),
            Pending::Zone { mut zone, outline } => all(&outline).map(|outline| {
                zone.outline = outline;
                board.zones.push(zone);
            }),
            Pending::Outline { layer, width, points, closed } => all(&points).map(|points| {
                let segments = if closed { points.len() } else { points.len().saturating_sub(1) };
                for i in 0..segments {
                    let (start, end) = (points[i], points[(i + 1) % points.len()]);
                    board.graphics.push(Graphic::Line { layer, start, end, width });
                }
            }),
            Pending::Legends { reference, legends, row, at, size, angle, layer, frame } => match pads.get(&reference) {
                None => Err(anyhow!("pin legends: there is no part {reference}")),
                Some(placed) => {
                    for (number, pad) in placed {
                        let Some((_, legend)) = legends.iter().find(|(n, _)| n == number) else { continue };
                        board.texts.push(design::Text {
                            text: legend.clone(),
                            at: legend_at(frame, row, at, *pad),
                            layer,
                            size,
                            thickness: len(0.18),
                            angle: frame.angle + angle,
                            mirrored: layer.is_back(),
                        });
                    }
                    Ok(())
                }
            },
        };
        if let Err(err) = outcome {
            errors.push(format!("{err:#}"));
        }
    }

    board.nets = board.all_nets().into_iter().map(String::from).collect();
    let warnings = match validate_nets(&board, &design.no_connect) {
        Ok(warnings) => warnings,
        Err(err) => {
            errors.push(format!("{err:#}"));
            Vec::new()
        }
    };
    if !errors.is_empty() {
        bail!("{} problem(s):\n  {}", errors.len(), errors.join("\n  "));
    }
    Ok(Resolved {
        design: board,
        project: project(design.project),
        artwork: collector.artwork,
        restore,
        unlink_after_route: design.unlink_after_route,
        router_clearance: design.router_clearance,
        warnings,
    })
}

/// Where a pad's legend sits: on the row (or column) at `at` in the legends'
/// frame, level with the pad. Pad positions are read back in mm, as pcbnew
/// placed them.
fn legend_at(frame: Frame, row: bool, at: f64, pad: Pt) -> Pt {
    let (x, y) = (pad.x.via_design_mm(), pad.y.via_design_mm());
    if frame.angle == 0.0 {
        let (ax, ay) = frame.apply(&wit::Point { x: at, y: at });
        return if row { Pt::new(x, len(ay)) } else { Pt::new(len(ax), y) };
    }
    // In a turned frame the pad is taken back into it, so the row stays a
    // row of the group.
    let (px, py) = frame.unapply((x.to_mm(), y.to_mm()));
    let local = if row { wit::Point { x: px, y: at } } else { wit::Point { x: at, y: py } };
    let (bx, by) = frame.apply(&local);
    Pt::mm(bx, by)
}

/// Netlist checks: net classes name nets the design uses, and every net
/// should connect at least two pads unless marked no-connect.
fn validate_nets(design: &design::Board, no_connect: &[String]) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    for class in &design.netclasses {
        let unknown: Vec<&str> = class.nets.iter().map(String::as_str).filter(|n| !design.nets.contains(*n)).collect();
        if !unknown.is_empty() {
            problems.push(format!("net class {} names nets the board never uses: {}", class.name, unknown.join(", ")));
        }
    }
    if !problems.is_empty() {
        bail!(problems.join("\n  "));
    }
    let mut pins: BTreeMap<&str, usize> = BTreeMap::new();
    for fp in &design.footprints {
        match &fp.source {
            Source::Library(lib) => lib.pins.iter().for_each(|(_, net)| *pins.entry(net).or_default() += 1),
            Source::Generated(g) => {
                g.pads.iter().filter_map(|p| p.net.as_deref()).for_each(|net| *pins.entry(net).or_default() += 1)
            }
        }
    }
    Ok(design
        .nets
        .iter()
        .filter(|net| !no_connect.contains(net))
        .filter_map(|net| match pins.get(net.as_str()).copied().unwrap_or(0) {
            0 => Some(format!("net {net} connects no pads")),
            1 => Some(format!("net {net} connects only one pad")),
            _ => None,
        })
        .collect())
}
