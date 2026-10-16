//! Serialise a [`design::Board`] as a `.kicad_pcb`.
//!
//! Items are written the way pcbnew writes freshly created objects. The
//! result is then filled and re-saved by KiCad itself, which settles item
//! order and number formatting, so only the semantics here must be exact.

use anyhow::{Result, bail};
use bb_core::footprint::FootprintSource as _;
use uuid::Uuid;

use super::library::Libraries;
use crate::design::{self, Board, Footprint, GeneratedFootprint, Graphic, LibraryFootprint, ModelEdit, Source};
use crate::sexpr::{self, Sexp, num};
use crate::units::{Nm, Pt};

const NEW_BOARD: &str = include_str!("new_board.kicad_pcb");

fn uuid() -> Sexp {
    Sexp::node("uuid", [Sexp::str(Uuid::new_v4().to_string())])
}

fn mm(value: Nm) -> Sexp {
    sexpr::mm_atom(value.0)
}

fn xy(head: &str, p: Pt) -> Sexp {
    Sexp::node(head, [mm(p.x), mm(p.y)])
}

fn layer(layer: design::Layer) -> Sexp {
    Sexp::node("layer", [Sexp::str(layer.name())])
}

fn yes(head: &str) -> Sexp {
    Sexp::node(head, [Sexp::atom("yes")])
}

fn stroke(width: Nm) -> Sexp {
    Sexp::node("stroke", [Sexp::node("width", [mm(width)]), Sexp::node("type", [Sexp::atom("default")])])
}

fn font(size: Nm, thickness: Option<Nm>) -> Sexp {
    let mut items = vec![Sexp::node("size", [mm(size), mm(size)])];
    items.extend(thickness.map(|t| Sexp::node("thickness", [mm(t)])));
    Sexp::node("font", items)
}

/// Footprint orientation: KiCad keeps it in (-180, 180].
fn normalize_180(degrees: f64) -> f64 {
    let a = degrees.rem_euclid(360.0);
    if a > 180.0 { a - 360.0 } else { a }
}

/// Text and pad orientation: KiCad keeps it in [0, 360).
fn normalize_360(degrees: f64) -> f64 {
    degrees.rem_euclid(360.0)
}

/// Write the board as `.kicad_pcb` text.
pub fn write(board: &Board, libraries: &Libraries) -> Result<String> {
    if board.copper_layers != 2 {
        bail!("only two-layer boards are supported so far (got {} copper layers)", board.copper_layers);
    }
    let mut root = sexpr::parse(NEW_BOARD)?;
    let items = root.as_list_mut().expect("board root is a list");
    let fonts = items.pop().expect("embedded_fonts terminator");
    for footprint in &board.footprints {
        items.push(footprint_node(footprint, libraries)?);
    }
    items.extend(board.graphics.iter().map(|g| graphic_node(g, "gr")));
    items.extend(board.texts.iter().map(text_node));
    items.extend(board.tracks.iter().map(|t| {
        Sexp::node(
            "segment",
            [
                xy("start", t.start),
                xy("end", t.end),
                Sexp::node("width", [mm(t.width)]),
                layer(t.layer),
                Sexp::node("net", [Sexp::str(&t.net)]),
                uuid(),
            ],
        )
    }));
    items.extend(board.vias.iter().map(|v| {
        Sexp::node(
            "via",
            [
                xy("at", v.at),
                Sexp::node("size", [mm(v.diameter)]),
                Sexp::node("drill", [mm(v.drill)]),
                Sexp::node("layers", [Sexp::str("F.Cu"), Sexp::str("B.Cu")]),
                Sexp::node("net", [Sexp::str(&v.net)]),
                uuid(),
            ],
        )
    }));
    items.extend(board.zones.iter().map(zone_node));
    items.push(fonts);
    Ok(root.to_kicad())
}

fn text_node(text: &design::Text) -> Sexp {
    let mut effects = vec![font(text.size, Some(text.thickness))];
    if text.mirrored {
        effects.push(Sexp::node("justify", [Sexp::atom("mirror")]));
    }
    Sexp::node(
        "gr_text",
        [
            Sexp::str(&text.text),
            Sexp::node("at", [mm(text.at.x), mm(text.at.y), num(normalize_360(text.angle))]),
            layer(text.layer),
            uuid(),
            Sexp::node("effects", effects),
        ],
    )
}

/// A graphic as `gr_*` (board) or `fp_*` (footprint) item.
fn graphic_node(graphic: &Graphic, prefix: &str) -> Sexp {
    match graphic {
        Graphic::Line { layer: l, start, end, width } => Sexp::node(
            &format!("{prefix}_line"),
            [xy("start", *start), xy("end", *end), stroke(*width), layer(*l), uuid()],
        ),
        Graphic::Poly { layer: l, points } => Sexp::node(
            &format!("{prefix}_poly"),
            [Sexp::node("pts", points.iter().map(|p| xy("xy", *p))), stroke(Nm(0)), yes("fill"), layer(*l), uuid()],
        ),
    }
}

fn zone_node(zone: &design::Zone) -> Sexp {
    let mut items = vec![
        Sexp::node("net", [Sexp::str(&zone.net)]),
        layer(zone.layer),
        uuid(),
        Sexp::node("hatch", [Sexp::atom("edge"), Sexp::atom("0.5")]),
    ];
    if zone.priority > 0 {
        items.push(Sexp::node("priority", [num(zone.priority as f64)]));
    }
    items.extend([
        Sexp::node("connect_pads", [Sexp::atom("yes"), Sexp::node("clearance", [mm(zone.clearance)])]),
        Sexp::node("min_thickness", [mm(zone.min_thickness)]),
        Sexp::node(
            "fill",
            [
                Sexp::node("thermal_gap", [Sexp::atom("0.5")]),
                Sexp::node("thermal_bridge_width", [Sexp::atom("0.5")]),
                Sexp::node("island_removal_mode", [Sexp::atom("0")]),
            ],
        ),
        Sexp::node("polygon", [Sexp::node("pts", zone.outline.iter().map(|p| xy("xy", *p)))]),
    ]);
    Sexp::node("zone", items)
}

/// `(property "Name" "value" (at x y angle) (layer ..) [(hide yes)] (uuid ..) (effects ..))`
/// as FOOTPRINT's constructor creates the mandatory fields.
fn default_property(name: &str, value: &str, layer_name: &str, angle: f64, hidden: bool) -> Sexp {
    let mut items = vec![
        Sexp::str(name),
        Sexp::str(value),
        Sexp::node("at", [num(0.0), num(0.0), num(angle)]),
        Sexp::node("layer", [Sexp::str(layer_name)]),
    ];
    if hidden {
        items.push(yes("hide"));
    }
    items.push(uuid());
    items.push(Sexp::node("effects", [font(Nm::from_mm(1.27), None)]));
    Sexp::node("property", items)
}

fn footprint_header(name: String, footprint: &Footprint) -> Vec<Sexp> {
    let mut at = vec![mm(footprint.at.x), mm(footprint.at.y)];
    let angle = normalize_180(footprint.angle);
    if angle != 0.0 {
        at.push(num(angle));
    }
    vec![
        Sexp::atom("footprint"),
        Sexp::str(name),
        Sexp::node("layer", [Sexp::str("F.Cu")]),
        uuid(),
        Sexp::node("at", at),
    ]
}

fn model_node(model: &design::Model) -> Sexp {
    let xyz = |values: [f64; 3]| Sexp::node("xyz", values.map(num));
    Sexp::node(
        "model",
        [
            Sexp::str(&model.path),
            Sexp::node("offset", [xyz(model.offset)]),
            Sexp::node("scale", [xyz([1.0; 3])]),
            Sexp::node("rotate", [xyz([0.0; 3])]),
        ],
    )
}

fn footprint_node(footprint: &Footprint, libraries: &Libraries) -> Result<Sexp> {
    match &footprint.source {
        Source::Generated(generated) => Ok(generated_node(footprint, generated)),
        Source::Library(library) => library_node(footprint, library, libraries),
    }
}

fn generated_node(footprint: &Footprint, generated: &GeneratedFootprint) -> Sexp {
    let angle = normalize_360(footprint.angle);
    let mut items = footprint_header(format!("{}:{}", generated.library, generated.name), footprint);
    items.extend([
        default_property("Reference", &footprint.reference, "F.SilkS", angle, footprint.hide_reference),
        default_property("Value", &footprint.value, "F.Fab", angle, true),
        default_property("Datasheet", "", "F.Fab", angle, true),
        default_property("Description", "", "F.Fab", angle, true),
    ]);
    let attrs: Vec<Sexp> =
        [(generated.exclude_from_pos, "exclude_from_pos_files"), (generated.exclude_from_bom, "exclude_from_bom")]
            .into_iter()
            .filter(|(on, _)| *on)
            .map(|(_, name)| Sexp::atom(name))
            .collect();
    if !attrs.is_empty() {
        items.push(Sexp::node("attr", attrs));
    }
    items.push(Sexp::node("duplicate_pad_numbers_are_jumpers", [Sexp::atom("no")]));
    items.extend(generated.graphics.iter().map(|g| graphic_node(g, "fp")));
    for pad in &generated.pads {
        let mut at = vec![mm(pad.at.x), mm(pad.at.y)];
        if angle != 0.0 {
            at.push(num(angle));
        }
        let mut node = vec![
            Sexp::str(&pad.number),
            Sexp::atom(if pad.drill.is_some() { "thru_hole" } else { "smd" }),
            Sexp::atom(pad.shape.name()),
            Sexp::node("at", at),
            Sexp::node("size", [mm(pad.size.0), mm(pad.size.1)]),
        ];
        match pad.drill {
            Some(drill) => node.extend([
                Sexp::node("drill", [mm(drill)]),
                Sexp::node("layers", [Sexp::str("*.Cu"), Sexp::str("*.Mask")]),
                Sexp::node("remove_unused_layers", [Sexp::atom("no")]),
            ]),
            None => node.push(Sexp::node("layers", ["F.Cu", "F.Mask", "F.Paste"].map(Sexp::str))),
        }
        node.extend(pad.net.as_ref().map(|net| Sexp::node("net", [Sexp::str(net)])));
        // PAD's constructor sets a 45° spoke angle, which KiCad saves for non-round pads.
        if pad.shape != design::PadShape::Circle {
            node.push(Sexp::node("thermal_bridge_angle", [num(45.0)]));
        }
        node.push(uuid());
        items.push(Sexp::node("pad", node));
    }
    items.push(Sexp::node("embedded_fonts", [Sexp::atom("no")]));
    items.extend(generated.models.iter().map(model_node));
    Sexp::List(items)
}

/// Place a stock library footprint as `FootprintLoad` + `SetReference`,
/// `SetValue`, `SetPosition`, `SetOrientation`, and pad nets would.
fn library_node(footprint: &Footprint, library: &LibraryFootprint, libraries: &Libraries) -> Result<Sexp> {
    let module = libraries.load(&library.library, &library.name)?;
    let angle = footprint.angle;
    let rotate = |node: &mut Sexp, always: bool| {
        if let Some(at) = node.child_mut("at").and_then(Sexp::as_list_mut) {
            let existing = at.get(3).and_then(Sexp::as_f64).unwrap_or(0.0);
            let total = normalize_360(existing + angle);
            at.truncate(3);
            if always || total != 0.0 {
                at.push(num(total));
            }
        }
    };

    let mut items = footprint_header(library.name.clone(), footprint);
    let mut fields = Vec::new();
    let mut model_index = 0;
    for child in module.args().iter().skip(1) {
        let Some(head) = child.head() else { continue };
        let mut child = child.clone();
        match head {
            "version" | "generator" | "generator_version" | "layer" => continue,
            "property" => {
                let name = child.arg(0).and_then(Sexp::text).unwrap_or_default().to_owned();
                let list = child.as_list_mut().expect("property list");
                match name.as_str() {
                    "Reference" => list[2] = Sexp::str(&footprint.reference),
                    "Value" => list[2] = Sexp::str(&footprint.value),
                    _ => {}
                }
                rotate(&mut child, true);
                let hidden = name == "Value" || (name == "Reference" && footprint.hide_reference);
                insert_after_layer(&mut child, hidden);
                fields.push(name);
            }
            "fp_text" => {
                rotate(&mut child, true);
                insert_after_layer(&mut child, false);
            }
            "pad" => {
                rotate(&mut child, false);
                let number = child.arg(0).and_then(Sexp::text).unwrap_or_default().to_owned();
                let list = child.as_list_mut().expect("pad list");
                if let Some((_, net)) = library.pins.iter().find(|(pad, _)| *pad == number) {
                    list.push(Sexp::node("net", [Sexp::str(net)]));
                }
                list.push(uuid());
            }
            "model" => {
                match &library.models {
                    ModelEdit::Keep => {}
                    ModelEdit::RenameFirst(path) if model_index == 0 => {
                        child.as_list_mut().expect("model list")[1] = Sexp::str(path);
                    }
                    ModelEdit::RenameFirst(_) => {}
                    ModelEdit::Replace(_) => continue,
                }
                model_index += 1;
            }
            head if head.starts_with("fp_") => child.as_list_mut().expect("graphic list").push(uuid()),
            _ => {}
        }
        items.push(child);
    }
    insert_missing_fields(&mut items, &fields, normalize_360(angle));
    if let ModelEdit::Replace(models) = &library.models {
        items.extend(models.iter().map(model_node));
    }
    Ok(Sexp::List(items))
}

/// Add `(hide yes)` (when hidden) and a uuid right after a field's `(layer ..)`.
fn insert_after_layer(node: &mut Sexp, hidden: bool) {
    let list = node.as_list_mut().expect("field list");
    let position = list.iter().position(|item| item.is("layer")).map_or(list.len(), |i| i + 1);
    let has_hide = list.iter().any(|item| item.is("hide"));
    let mut insert = Vec::new();
    if hidden && !has_hide {
        insert.push(yes("hide"));
    }
    insert.push(uuid());
    let after_hide = if has_hide { position + 1 } else { position };
    list.splice(after_hide..after_hide, insert);
}

/// FOOTPRINT always carries Datasheet and Description; library files may omit them.
fn insert_missing_fields(items: &mut Vec<Sexp>, present: &[String], angle: f64) {
    let last_property = items.iter().rposition(|item| item.is("property")).map_or(items.len(), |i| i + 1);
    let missing: Vec<Sexp> = ["Datasheet", "Description"]
        .into_iter()
        .filter(|name| !present.iter().any(|p| p == name))
        .map(|name| default_property(name, "", "F.Fab", angle, true))
        .collect();
    items.splice(last_property..last_property, missing);
}
