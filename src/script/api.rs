//! The primitives available to board files.

// Each builtin takes its keyword arguments as Rust parameters, one-to-one.
#![allow(clippy::too_many_arguments)]

use std::path::PathBuf;

use anyhow::{Context as _, Result, anyhow, bail};
use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::starlark_module;
use starlark::values::dict::UnpackDictEntries;
use starlark::values::float::UnpackFloat;
use starlark::values::list::UnpackList;
use starlark::values::none::NoneType;
use starlark::values::structs::AllocStruct;
use starlark::values::{Heap, UnpackValue, Value};

use super::EvalContext;
use super::anchor::{Anchor, At};
use crate::design::{self, Footprint, GeneratedFootprint, Graphic, LibraryFootprint, Model, ModelEdit, Source};
use crate::units::{Nm, Pt};

/// Release settings for the board project.
#[derive(Clone, Debug, Default)]
pub struct Project {
    /// KiCad project stem: `<name>.kicad_pcb`, `<name>.kicad_pro`, ...
    pub name: String,
    /// Approved parts list, relative to the board directory.
    pub bom: String,
    /// MPN → LCSC part number for JLCPCB-assembled parts.
    pub lcsc: Vec<(String, String)>,
    /// Boards per DigiKey shopping-list order.
    pub order_quantity: u32,
    pub gerber_layers: String,
    /// Reference prefixes that never appear in the BOM (holes, logos, ...).
    pub unsourced: Vec<String>,
    /// External gate scripts: `<repo>/<name>/<name>.py --json <repo>/<name>/report.json`.
    pub checks: Vec<String>,
    /// The gate re-run with `--release` once the release exists.
    pub release_check: Option<String>,
    /// Repository-relative JSON with `manufacturing.cpl_rotation_offsets_deg`.
    pub contract: Option<String>,
    /// Board-directory files shipped in the complete release archive.
    pub release_files: Vec<String>,
    /// Where the KiCad project (`.kicad_pcb`, `.kicad_pro`, `.kicad_sch`)
    /// lives, relative to the board directory: `output` by default, or `.`
    /// for repositories that commit the routed board beside its sources.
    pub kicad_dir: String,
    pub schematic: SchematicConfig,
    /// Copper weight recorded in the review lock.
    pub copper_weight_oz: u32,
    /// Review decisions recorded in the lock, e.g. accepted risks.
    pub decisions: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct SchematicConfig {
    pub title: String,
    pub company: Option<String>,
    pub revision: String,
    pub comment: Option<String>,
    pub library: String,
}

/// A raster image to trace onto silkscreen as a generated footprint.
#[derive(Clone, Debug)]
pub struct Artwork {
    pub reference: String,
    pub value: String,
    pub library: String,
    pub name: String,
    pub image: PathBuf,
    pub center: (f64, f64),
    pub width: f64,
    pub layer: design::Layer,
    pub tone: crate::artwork::Tone,
}

/// A track segment the router must leave at least this wide (or recreate).
#[derive(Clone, Debug)]
pub struct Restore {
    pub net: String,
    pub layer: design::Layer,
    pub start: Pt,
    pub end: Pt,
    pub width: Nm,
}

#[derive(Default)]
pub(crate) struct State {
    pub board: design::Board,
    pub hide_reference_prefixes: Vec<String>,
    pub project: Option<Project>,
    pub artwork: Vec<Artwork>,
    pub restore: Vec<Restore>,
    pub unlink_after_route: Option<String>,
}

fn context<'a>(eval: &Evaluator<'_, 'a, '_>) -> &'a EvalContext {
    eval.extra.and_then(|extra| extra.downcast_ref::<EvalContext>()).expect("board evaluation context")
}

/// A point argument: `pin(...)` anchor or `(x, y)` mm tuple.
type XY = At;

fn mm(xy: XY) -> (f64, f64) {
    xy.mm()
}

fn pt(xy: XY) -> Pt {
    xy.pt()
}

fn len(value: f64) -> Nm {
    Nm::from_design_mm(value)
}

fn layer(name: &str) -> Result<design::Layer> {
    design::Layer::parse(name).ok_or_else(|| anyhow!("unknown layer {name:?}"))
}

/// Dict keys may be pad numbers written as ints or strings.
fn key(value: Value) -> String {
    value.unpack_str().map_or_else(|| value.to_str(), str::to_owned)
}

fn lib_id(text: &str) -> Result<(String, String)> {
    text.split_once(':')
        .map(|(lib, name)| (lib.to_owned(), name.to_owned()))
        .ok_or_else(|| anyhow!("footprint {text:?} must be written Library:Name"))
}

fn attr<'v>(value: Value<'v>, name: &str, heap: Heap<'v>) -> Result<Option<Value<'v>>> {
    let field = value.get_attr(name, heap).map_err(|e| anyhow!("{e}"))?;
    Ok(field.filter(|v| !v.is_none()))
}

fn unpack<'v, T: UnpackValue<'v>>(value: Value<'v>, what: &str) -> Result<T> {
    T::unpack_value_err(value).map_err(|e| anyhow!("{what}: {e}"))
}

/// Pad/graphic offsets resolve like pcbnew's `SetPosition`: round the
/// absolute position, then take the difference from the rounded origin.
fn local((ox, oy): (f64, f64), (dx, dy): (f64, f64)) -> Pt {
    Pt::new(len(ox + dx) - len(ox), len(oy + dy) - len(oy))
}

fn hidden(state: &State, reference: &str) -> bool {
    state.hide_reference_prefixes.iter().any(|prefix| reference.starts_with(prefix.as_str()))
}

#[starlark_module]
pub(crate) fn primitives(builder: &mut GlobalsBuilder) {
    /// Declare the KiCad project and its release settings.
    fn project<'v>(
        name: &str,
        #[starlark(require = named, default = "bom/approved-parts.csv")] bom: &str,
        #[starlark(require = named)] lcsc: Option<UnpackDictEntries<String, String>>,
        #[starlark(require = named, default = 1)] order_quantity: u32,
        #[starlark(require = named, default = "F.Cu,B.Cu,F.Mask,B.Mask,F.Silkscreen,B.Silkscreen,Edge.Cuts")]
        gerber_layers: &str,
        #[starlark(require = named)] unsourced: Option<UnpackList<String>>,
        #[starlark(require = named)] checks: Option<UnpackList<String>>,
        #[starlark(require = named)] release_check: Option<String>,
        #[starlark(require = named)] contract: Option<String>,
        #[starlark(require = named)] release_files: Option<UnpackList<String>>,
        #[starlark(require = named, default = "output")] kicad_dir: &str,
        #[starlark(require = named)] schematic: Option<UnpackDictEntries<String, String>>,
        #[starlark(require = named, default = 1)] copper_weight_oz: u32,
        #[starlark(require = named)] decisions: Option<UnpackDictEntries<String, String>>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<NoneType> {
        let schematic: Vec<(String, String)> = schematic.map(|s| s.entries).unwrap_or_default();
        let get = |k: &str| schematic.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
        let project = Project {
            name: name.to_owned(),
            bom: bom.to_owned(),
            lcsc: lcsc.map(|l| l.entries).unwrap_or_default(),
            order_quantity,
            gerber_layers: gerber_layers.to_owned(),
            unsourced: unsourced.map_or_else(|| ["H", "LOGO", "TP", "FID"].map(String::from).into(), |u| u.items),
            checks: checks.map(|c| c.items).unwrap_or_default(),
            release_check,
            contract,
            release_files: release_files.map(|r| r.items).unwrap_or_default(),
            kicad_dir: kicad_dir.to_owned(),
            schematic: SchematicConfig {
                title: get("title").unwrap_or_else(|| name.to_owned()),
                company: get("company"),
                revision: get("revision").unwrap_or_else(|| "1.0".to_owned()),
                comment: get("comment"),
                library: get("library").unwrap_or_else(|| name.to_owned()),
            },
            copper_weight_oz,
            decisions: decisions.map(|d| d.entries).unwrap_or_default(),
        };
        context(eval).state.borrow_mut().project = Some(project);
        Ok(NoneType)
    }

    /// Board-wide settings. `unlink_after_route="Lib"` detaches generated
    /// footprints of that library (except artwork) and hides their
    /// designators once the board is routed.
    fn board<'v>(
        #[starlark(require = named, default = 2)] copper_layers: u32,
        #[starlark(require = named)] hide_references: Option<UnpackList<String>>,
        #[starlark(require = named)] finish: Option<UnpackDictEntries<String, String>>,
        #[starlark(require = named)] unlink_after_route: Option<String>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<NoneType> {
        let mut state = context(eval).state.borrow_mut();
        state.unlink_after_route = unlink_after_route;
        state.board.copper_layers = u8::try_from(copper_layers)?;
        state.hide_reference_prefixes = hide_references.map(|h| h.items).unwrap_or_default();
        if let Some(finish) = finish {
            let get = |k: &str| finish.entries.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
            state.board.finish = Some(design::Finish {
                mask: get("mask").unwrap_or_else(|| "Green".into()),
                silk: get("silk").unwrap_or_else(|| "White".into()),
                copper: get("copper").unwrap_or_else(|| "HASL lead-free".into()),
            });
        }
        Ok(NoneType)
    }

    /// Set a `.kicad_pro` DRC rule severity, e.g. `rule_severity("courtyards_overlap", "warning")`.
    fn rule_severity<'v>(rule: &str, severity: &str, eval: &mut Evaluator<'v, '_, '_>) -> anyhow::Result<NoneType> {
        context(eval).state.borrow_mut().board.rule_severities.push((rule.to_owned(), severity.to_owned()));
        Ok(NoneType)
    }

    /// Declare nets up front.
    fn nets<'v>(names: UnpackList<String>, eval: &mut Evaluator<'v, '_, '_>) -> anyhow::Result<NoneType> {
        context(eval).state.borrow_mut().board.nets.extend(names.items);
        Ok(NoneType)
    }

    /// A net class; its routing widths are honoured by the autorouter.
    fn netclass<'v>(
        name: &str,
        #[starlark(require = named)] track_width: UnpackFloat,
        #[starlark(require = named)] nets: UnpackList<String>,
        #[starlark(require = named, default = UnpackFloat(0.25))] clearance: UnpackFloat,
        #[starlark(require = named, default = UnpackFloat(0.8))] via_diameter: UnpackFloat,
        #[starlark(require = named, default = UnpackFloat(0.4))] via_drill: UnpackFloat,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<NoneType> {
        context(eval).state.borrow_mut().board.netclasses.push(design::NetClass {
            name: name.to_owned(),
            track_width: len(track_width.0),
            clearance: len(clearance.0),
            via_diameter: len(via_diameter.0),
            via_drill: len(via_drill.0),
            nets: nets.items,
        });
        Ok(NoneType)
    }

    /// A straight board graphic.
    fn line<'v>(
        layer: &str,
        start: XY,
        end: XY,
        #[starlark(require = named)] width: UnpackFloat,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<NoneType> {
        let graphic = Graphic::Line { layer: self::layer(layer)?, start: pt(start), end: pt(end), width: len(width.0) };
        context(eval).state.borrow_mut().board.graphics.push(graphic);
        Ok(NoneType)
    }

    /// A closed outline through `points` (board edge, silkscreen boxes).
    fn outline<'v>(
        layer: &str,
        points: UnpackList<XY>,
        #[starlark(require = named)] width: UnpackFloat,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<NoneType> {
        let layer = self::layer(layer)?;
        let points: Vec<Pt> = points.items.into_iter().map(pt).collect();
        let mut state = context(eval).state.borrow_mut();
        for (i, &start) in points.iter().enumerate() {
            let end = points[(i + 1) % points.len()];
            state.board.graphics.push(Graphic::Line { layer, start, end, width: len(width.0) });
        }
        Ok(NoneType)
    }

    /// A copper polyline: one segment between each pair of points.
    fn track<'v>(
        net: &str,
        points: UnpackList<XY>,
        #[starlark(require = named, default = UnpackFloat(0.4))] width: UnpackFloat,
        #[starlark(require = named, default = "F.Cu")] layer: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<NoneType> {
        let layer = self::layer(layer)?;
        let points: Vec<Pt> = points.items.into_iter().map(pt).collect();
        if points.len() < 2 {
            bail!("track on {net} needs at least two points");
        }
        let mut state = context(eval).state.borrow_mut();
        for pair in points.windows(2) {
            state.board.tracks.push(design::Track {
                net: net.to_owned(),
                layer,
                width: len(width.0),
                start: pair[0],
                end: pair[1],
            });
        }
        Ok(NoneType)
    }

    /// A through via.
    fn via<'v>(
        net: &str,
        at: XY,
        #[starlark(require = named, default = UnpackFloat(1.0))] diameter: UnpackFloat,
        #[starlark(require = named, default = UnpackFloat(0.5))] drill: UnpackFloat,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<NoneType> {
        let via = design::Via { net: net.to_owned(), at: pt(at), diameter: len(diameter.0), drill: len(drill.0) };
        context(eval).state.borrow_mut().board.vias.push(via);
        Ok(NoneType)
    }

    /// A copper pour, filled by KiCad.
    fn zone<'v>(
        net: &str,
        layer: &str,
        outline: UnpackList<XY>,
        #[starlark(require = named, default = 0)] priority: u32,
        #[starlark(require = named, default = UnpackFloat(0.3))] clearance: UnpackFloat,
        #[starlark(require = named, default = UnpackFloat(0.25))] min_thickness: UnpackFloat,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<NoneType> {
        let zone = design::Zone {
            net: net.to_owned(),
            layer: self::layer(layer)?,
            outline: outline.items.into_iter().map(pt).collect(),
            priority,
            clearance: len(clearance.0),
            min_thickness: len(min_thickness.0),
        };
        context(eval).state.borrow_mut().board.zones.push(zone);
        Ok(NoneType)
    }

    /// Board text; back-side text is mirrored.
    fn text<'v>(
        text: &str,
        at: XY,
        #[starlark(require = named, default = UnpackFloat(1.2))] size: UnpackFloat,
        #[starlark(require = named, default = UnpackFloat(0.0))] angle: UnpackFloat,
        #[starlark(require = named, default = "F.SilkS")] layer: &str,
        #[starlark(require = named, default = UnpackFloat(0.18))] thickness: UnpackFloat,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<NoneType> {
        let layer = self::layer(layer)?;
        let text = design::Text {
            text: text.to_owned(),
            at: pt(at),
            layer,
            size: len(size.0),
            thickness: len(thickness.0),
            angle: angle.0,
            mirrored: layer.is_back(),
        };
        context(eval).state.borrow_mut().board.texts.push(text);
        Ok(NoneType)
    }

    /// Place a stock KiCad library footprint.
    ///
    /// `models=[...]` replaces its 3D models (`[]` removes them);
    /// `swap_model="path"` keeps the library model's placement but changes its file.
    fn part<'v>(
        reference: &str,
        value: &str,
        footprint: &str,
        #[starlark(require = named)] at: XY,
        #[starlark(require = named, default = UnpackFloat(0.0))] angle: UnpackFloat,
        #[starlark(require = named)] pins: Option<UnpackDictEntries<Value<'v>, String>>,
        #[starlark(require = named)] models: Option<UnpackList<Value<'v>>>,
        #[starlark(require = named)] swap_model: Option<String>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<NoneType> {
        let (library, name) = lib_id(footprint)?;
        let heap = eval.heap();
        let models = match (models, swap_model) {
            (Some(_), Some(_)) => bail!("{reference}: use models= or swap_model=, not both"),
            (Some(models), None) => {
                ModelEdit::Replace(models.items.into_iter().map(|m| model_spec(m, heap)).collect::<Result<_>>()?)
            }
            (None, Some(path)) => ModelEdit::RenameFirst(path),
            (None, None) => ModelEdit::Keep,
        };
        let pins = pins.map(|p| p.entries.into_iter().map(|(k, net)| (key(k), net)).collect()).unwrap_or_default();
        let ctx = context(eval);
        // Fail at the call site if the footprint does not exist.
        ctx.libraries.load(&library, &name)?;
        let mut state = ctx.state.borrow_mut();
        let footprint = Footprint {
            reference: reference.to_owned(),
            value: value.to_owned(),
            at: pt(at),
            angle: angle.0,
            hide_reference: hidden(&state, reference),
            source: Source::Library(LibraryFootprint { library, name, pins, models }),
        };
        add_footprint(&mut state, footprint)
    }

    /// Generate a footprint from pads (`smd`, `tht`) and graphics (`fp_line`)
    /// in footprint-local mm. `body=(w, h)` adds a fab outline and a 0.25 mm courtyard.
    fn generated<'v>(
        reference: &str,
        value: &str,
        footprint: &str,
        #[starlark(require = named)] at: XY,
        #[starlark(require = named, default = UnpackFloat(0.0))] angle: UnpackFloat,
        #[starlark(require = named)] pads: Option<UnpackList<Value<'v>>>,
        #[starlark(require = named)] body: Option<XY>,
        #[starlark(require = named)] graphics: Option<UnpackList<Value<'v>>>,
        #[starlark(require = named)] models: Option<UnpackList<Value<'v>>>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<NoneType> {
        let (library, name) = lib_id(footprint)?;
        let origin = mm(at);
        let at = pt(at);
        let heap = eval.heap();
        let mut generated = GeneratedFootprint { library, name, ..Default::default() };
        for pad in pads.map(|p| p.items).unwrap_or_default() {
            generated.pads.push(pad_spec(pad, origin, heap).with_context(|| format!("{reference} pad"))?);
        }
        if let Some(body) = body {
            let (w, h) = mm(body);
            for (layer, margin, stroke) in [(design::Layer::FFab, 0.0, 0.1), (design::Layer::FCourtyard, 0.25, 0.05)] {
                let (w, h) = (w + 2.0 * margin, h + 2.0 * margin);
                let corners = [(-w / 2.0, -h / 2.0), (w / 2.0, -h / 2.0), (w / 2.0, h / 2.0), (-w / 2.0, h / 2.0)];
                for i in 0..4 {
                    generated.graphics.push(Graphic::Line {
                        layer,
                        start: local(origin, corners[i]),
                        end: local(origin, corners[(i + 1) % 4]),
                        width: len(stroke),
                    });
                }
            }
        }
        for graphic in graphics.map(|g| g.items).unwrap_or_default() {
            generated.graphics.push(line_spec(graphic, origin, heap).with_context(|| format!("{reference} graphic"))?);
        }
        for model in models.map(|m| m.items).unwrap_or_default() {
            generated.models.push(model_spec(model, heap)?);
        }
        let mut state = context(eval).state.borrow_mut();
        let footprint = Footprint {
            reference: reference.to_owned(),
            value: value.to_owned(),
            at,
            angle: angle.0,
            hide_reference: hidden(&state, reference),
            source: Source::Generated(generated),
        };
        add_footprint(&mut state, footprint)
    }

    /// An SMD pad for `generated(...)`.
    fn smd<'v>(
        number: Value<'v>,
        at: XY,
        size: XY,
        #[starlark(require = named)] net: Option<String>,
        #[starlark(require = named, default = "rect")] shape: &str,
        heap: Heap<'v>,
    ) -> anyhow::Result<Value<'v>> {
        pad_value(heap, key(number), at, size, None, net, shape)
    }

    /// A plated through-hole pad for `generated(...)`.
    fn tht<'v>(
        number: Value<'v>,
        at: XY,
        size: XY,
        #[starlark(require = named)] drill: UnpackFloat,
        #[starlark(require = named)] net: Option<String>,
        #[starlark(require = named, default = "rect")] shape: &str,
        heap: Heap<'v>,
    ) -> anyhow::Result<Value<'v>> {
        pad_value(heap, key(number), at, size, Some(drill.0), net, shape)
    }

    /// A footprint-local graphic segment for `generated(...)`.
    fn fp_line<'v>(
        layer: &str,
        start: XY,
        end: XY,
        #[starlark(require = named)] width: UnpackFloat,
        heap: Heap<'v>,
    ) -> anyhow::Result<Value<'v>> {
        self::layer(layer)?;
        Ok(heap.alloc(AllocStruct([
            ("layer", heap.alloc(layer)),
            ("start", heap.alloc(mm(start))),
            ("end", heap.alloc(mm(end))),
            ("width", heap.alloc(width.0)),
        ])))
    }

    /// A 3D model reference for `part(models=...)` or `generated(models=...)`.
    fn model<'v>(
        path: &str,
        #[starlark(require = named)] offset: Option<(UnpackFloat, UnpackFloat, UnpackFloat)>,
        heap: Heap<'v>,
    ) -> anyhow::Result<Value<'v>> {
        let (x, y, z) = offset.map_or((0.0, 0.0, 0.0), |(x, y, z)| (x.0, y.0, z.0));
        Ok(heap.alloc(AllocStruct([("path", heap.alloc(path)), ("offset", heap.alloc((x, y, z)))])))
    }

    /// The exact centre of a placed footprint's pad, for routing to it.
    fn pin<'v>(reference: &str, pad: Value<'v>, eval: &mut Evaluator<'v, '_, '_>) -> anyhow::Result<Anchor> {
        Ok(Anchor::new(pad_at(context(eval), reference, &key(pad))?))
    }

    /// Rotate `point`, given relative to `center`, by `angle` degrees
    /// (math-positive in the Y-down frame: clockwise on screen).
    fn rotate(center: XY, point: XY, angle: UnpackFloat) -> anyhow::Result<(f64, f64)> {
        let ((cx, cy), (x, y)) = (mm(center), mm(point));
        let r = angle.0.to_radians();
        Ok((cx + x * r.cos() - y * r.sin(), cy + x * r.sin() + y * r.cos()))
    }

    /// One legend per named pad, placed from the pad itself: in a `row` at a
    /// fixed Y (rotated 90° by default) or a `column` at a fixed X.
    fn pin_legends<'v>(
        reference: &str,
        legends: UnpackDictEntries<Value<'v>, String>,
        #[starlark(require = named)] row: Option<UnpackFloat>,
        #[starlark(require = named)] column: Option<UnpackFloat>,
        #[starlark(require = named, default = UnpackFloat(0.6))] size: UnpackFloat,
        #[starlark(require = named)] angle: Option<UnpackFloat>,
        #[starlark(require = named, default = "F.SilkS")] layer: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<NoneType> {
        let layer = self::layer(layer)?;
        let angle = angle.map(|a| a.0);
        let legends: Vec<(String, String)> = legends.entries.into_iter().map(|(k, v)| (key(k), v)).collect();
        let ctx = context(eval);
        let (fixed, default_angle) = match (row, column) {
            (Some(y), None) => ((None, Some(len(y.0))), 90.0),
            (None, Some(x)) => ((Some(len(x.0)), None), 0.0),
            _ => bail!("pin_legends({reference}): give exactly one of row= or column="),
        };
        let pads = pads_of(ctx, reference)?;
        let mut texts = Vec::new();
        for (number, at) in pads {
            let Some((_, legend)) = legends.iter().find(|(pad, _)| *pad == number) else { continue };
            texts.push(design::Text {
                text: legend.clone(),
                // Legends are placed from the pad position read back in mm.
                at: Pt::new(fixed.0.unwrap_or(at.x.via_design_mm()), fixed.1.unwrap_or(at.y.via_design_mm())),
                layer,
                size: len(size.0),
                thickness: len(0.18),
                angle: angle.unwrap_or(default_angle),
                mirrored: layer.is_back(),
            });
        }
        ctx.state.borrow_mut().board.texts.extend(texts);
        Ok(NoneType)
    }

    /// Trace a PNG onto silkscreen as a generated, BOM- and placement-excluded footprint.
    fn artwork<'v>(
        reference: &str,
        image: &str,
        #[starlark(require = named)] center: XY,
        #[starlark(require = named)] width: UnpackFloat,
        #[starlark(require = named, default = "F.SilkS")] layer: &str,
        #[starlark(require = named, default = "light")] tone: &str,
        #[starlark(require = named)] value: Option<String>,
        #[starlark(require = named)] footprint: Option<String>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<NoneType> {
        let tone = match tone {
            "light" => crate::artwork::Tone::Light,
            "dark" => crate::artwork::Tone::Dark,
            other => bail!("unknown artwork tone {other:?} (light or dark)"),
        };
        let ctx = context(eval);
        let image_path = ctx.dir.join(image);
        if !image_path.is_file() {
            bail!("artwork image not found: {}", image_path.display());
        }
        let (library, name) = lib_id(&footprint.unwrap_or_else(|| format!("Artwork:{reference}")))?;
        let file_name = image_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        ctx.state.borrow_mut().artwork.push(Artwork {
            reference: reference.to_owned(),
            value: value.unwrap_or(file_name),
            library,
            name,
            image: image_path,
            center: mm(center),
            width: width.0,
            layer: self::layer(layer)?,
            tone,
        });
        Ok(NoneType)
    }

    /// After routing, keep this segment at least `width` wide, adding it if
    /// the router removed it.
    fn keep_track<'v>(
        net: &str,
        start: XY,
        end: XY,
        #[starlark(require = named)] width: UnpackFloat,
        #[starlark(require = named, default = "F.Cu")] layer: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<NoneType> {
        let restore = Restore {
            net: net.to_owned(),
            layer: self::layer(layer)?,
            start: pt(start),
            end: pt(end),
            width: len(width.0),
        };
        context(eval).state.borrow_mut().restore.push(restore);
        Ok(NoneType)
    }
}

fn add_footprint(state: &mut State, footprint: Footprint) -> Result<NoneType> {
    if state.board.footprint(&footprint.reference).is_some() {
        bail!("duplicate footprint reference {}", footprint.reference);
    }
    state.board.footprints.push(footprint);
    Ok(NoneType)
}

fn pad_value<'v>(
    heap: Heap<'v>,
    number: String,
    at: XY,
    size: XY,
    drill: Option<f64>,
    net: Option<String>,
    shape: &str,
) -> Result<Value<'v>> {
    parse_shape(shape)?;
    Ok(heap.alloc(AllocStruct([
        ("number", heap.alloc(number)),
        ("at", heap.alloc(mm(at))),
        ("size", heap.alloc(mm(size))),
        ("drill", drill.map_or(Value::new_none(), |d| heap.alloc(d))),
        ("net", net.map_or(Value::new_none(), |n| heap.alloc(n))),
        ("shape", heap.alloc(shape)),
    ])))
}

fn parse_shape(shape: &str) -> Result<design::PadShape> {
    Ok(match shape {
        "rect" => design::PadShape::Rect,
        "circle" => design::PadShape::Circle,
        "oval" => design::PadShape::Oval,
        "roundrect" => design::PadShape::Roundrect,
        other => bail!("unknown pad shape {other:?}"),
    })
}

fn xy_attr<'v>(value: Value<'v>, name: &str, heap: Heap<'v>) -> Result<(f64, f64)> {
    let field = attr(value, name, heap)?.ok_or_else(|| anyhow!("missing {name}"))?;
    Ok(mm(unpack::<XY>(field, name)?))
}

fn str_attr<'v>(value: Value<'v>, name: &str, heap: Heap<'v>) -> Result<Option<String>> {
    attr(value, name, heap)?.map(|v| unpack::<String>(v, name)).transpose()
}

fn float_attr<'v>(value: Value<'v>, name: &str, heap: Heap<'v>) -> Result<Option<f64>> {
    attr(value, name, heap)?.map(|v| unpack::<UnpackFloat>(v, name).map(|f| f.0)).transpose()
}

fn pad_spec<'v>(value: Value<'v>, origin: (f64, f64), heap: Heap<'v>) -> Result<design::Pad> {
    let (sx, sy) = xy_attr(value, "size", heap)?;
    Ok(design::Pad {
        number: str_attr(value, "number", heap)?.ok_or_else(|| anyhow!("pad without a number"))?,
        at: local(origin, xy_attr(value, "at", heap)?),
        size: (len(sx), len(sy)),
        drill: float_attr(value, "drill", heap)?.map(len),
        shape: parse_shape(&str_attr(value, "shape", heap)?.unwrap_or_else(|| "rect".into()))?,
        net: str_attr(value, "net", heap)?,
    })
}

fn line_spec<'v>(value: Value<'v>, origin: (f64, f64), heap: Heap<'v>) -> Result<Graphic> {
    Ok(Graphic::Line {
        layer: layer(&str_attr(value, "layer", heap)?.ok_or_else(|| anyhow!("graphic without a layer"))?)?,
        start: local(origin, xy_attr(value, "start", heap)?),
        end: local(origin, xy_attr(value, "end", heap)?),
        width: len(float_attr(value, "width", heap)?.ok_or_else(|| anyhow!("graphic without a width"))?),
    })
}

fn model_spec<'v>(value: Value<'v>, heap: Heap<'v>) -> Result<Model> {
    let offset = attr(value, "offset", heap)?.ok_or_else(|| anyhow!("model without an offset"))?;
    let (x, y, z) = unpack::<(UnpackFloat, UnpackFloat, UnpackFloat)>(offset, "offset")?;
    Ok(Model {
        path: str_attr(value, "path", heap)?.ok_or_else(|| anyhow!("model without a path"))?,
        offset: [x.0, y.0, z.0],
    })
}

/// Board-frame pad centres of a placed footprint, in pad order.
fn pads_of(ctx: &EvalContext, reference: &str) -> Result<Vec<(String, Pt)>> {
    let state = ctx.state.borrow();
    let footprint = state.board.footprint(reference).ok_or_else(|| anyhow!("no footprint {reference} placed yet"))?;
    match &footprint.source {
        Source::Library(lib) => ctx.libraries.pad_positions(&lib.library, &lib.name, footprint.at, footprint.angle),
        Source::Generated(generated) => Ok(generated
            .pads
            .iter()
            .map(|pad| (pad.number.clone(), footprint.at + pad.at.rotate(footprint.angle)))
            .collect()),
    }
}

fn pad_at(ctx: &EvalContext, reference: &str, pad: &str) -> Result<Pt> {
    pads_of(ctx, reference)?
        .into_iter()
        .find(|(number, _)| number == pad)
        .map(|(_, at)| at)
        .ok_or_else(|| anyhow!("{reference} has no pad {pad}"))
}
