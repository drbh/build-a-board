//! The design as a board program returns it: a plain mirror of the `types`
//! interface in `wit/board.wit`, one Rust type per WIT type, named as
//! `wit-bindgen` names them.
//!
//! `bb` fills these from its Wasmtime bindings; a browser deserializes them
//! from JSON in serde's default shape for these very names: records as
//! objects with snake_case fields, enums as `"Case"`, variants as `"Case"`
//! or `{"Case": payload}`, tuples as arrays, and options as `null` or the
//! value.
//!
//! Lengths are millimetres in KiCad's board frame (Y down); angles are
//! degrees. See the WIT for what each field means.

use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A pad of a placed part; resolved after every part is placed.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PinRef {
    pub reference: String,
    pub pad: String,
}

/// A pad plus an offset written in the item's frame.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PadOffset {
    pub pad: PinRef,
    pub by: Point,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum Anchor {
    /// A point in the item's frame.
    Mm(Point),
    /// The exact centre of a pad.
    Pin(PinRef),
    /// A point relative to a pad.
    Offset(PadOffset),
}

/// A placement frame; `parent` indexes `Design::frames`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Frame {
    pub parent: Option<u32>,
    pub at: Point,
    /// Counter-clockwise on screen, as KiCad rotates footprints.
    pub rotate: f64,
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Model {
    pub path: String,
    pub offset: (f64, f64, f64),
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum Models {
    /// Keep the library footprint's 3D models.
    Keep,
    /// Keep the first model's placement, swap its file.
    Swap(String),
    /// Replace every model; an empty list removes them.
    Replace(Vec<Model>),
}

/// A stock KiCad library footprint (`Library:Name`).
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Part {
    pub reference: String,
    pub value: String,
    pub footprint: String,
    /// Pad number to net.
    pub pins: Vec<(String, String)>,
    pub models: Models,
    /// Degrees added to this part's rotation in JLCPCB's placement file.
    pub assembly_rotation: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum PadShape {
    Rect,
    Circle,
    Oval,
    Roundrect,
}

/// A pad of a generated footprint, in footprint-local mm.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Pad {
    pub number: String,
    pub at: Point,
    pub size: Point,
    /// Plated drill diameter; none for an SMD pad.
    pub drill: Option<f64>,
    pub shape: PadShape,
    pub net: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct FpLine {
    pub layer: String,
    pub start: Point,
    pub end: Point,
    pub width: f64,
}

/// A footprint built from pads and graphics.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Generated {
    pub reference: String,
    pub value: String,
    pub footprint: String,
    pub pads: Vec<Pad>,
    /// Fab outline plus a 0.25 mm courtyard around a centred body.
    pub body: Option<Point>,
    pub graphics: Vec<FpLine>,
    pub models: Vec<Model>,
    /// Degrees added to this part's rotation in JLCPCB's placement file.
    pub assembly_rotation: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum Footprint {
    Library(Part),
    Generated(Generated),
}

/// A footprint at a position in the item's frame.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Placement {
    pub footprint: Footprint,
    pub at: Point,
    /// Counter-clockwise on screen.
    pub rotate: f64,
    /// Hide the designator whatever `Design::hide_references` says.
    pub hide_reference: bool,
}

/// What becomes of a fixed track when the board is routed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Keep {
    /// Laid before routing; the router may thin or drop it.
    No,
    /// Laid before routing and put back at least as wide afterwards.
    Laid,
    /// Left to the router, then made at least as wide afterwards.
    Routed,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Track {
    pub net: String,
    pub points: Vec<Anchor>,
    pub width: f64,
    pub layer: String,
    pub keep: Keep,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Via {
    pub net: String,
    pub at: Anchor,
    pub diameter: f64,
    pub drill: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Zone {
    pub net: String,
    pub layer: String,
    pub outline: Vec<Anchor>,
    pub priority: u32,
    pub clearance: f64,
    pub min_thickness: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Text {
    pub text: String,
    pub at: Point,
    pub size: f64,
    pub rotate: f64,
    pub layer: String,
    pub thickness: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Outline {
    pub layer: String,
    pub points: Vec<Anchor>,
    pub width: f64,
    pub closed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Axis {
    /// Legends in a row at a fixed Y, each at its pad's X.
    Row,
    /// Legends in a column at a fixed X, each at its pad's Y.
    Column,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PinLegends {
    pub reference: String,
    pub legends: Vec<(String, String)>,
    pub axis: Axis,
    pub at: f64,
    pub size: f64,
    pub rotate: f64,
    pub layer: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Tone {
    Light,
    Dark,
}

/// A PNG in the board directory, traced onto silkscreen.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Artwork {
    pub reference: String,
    pub image: String,
    pub center: Point,
    pub width: f64,
    pub layer: String,
    pub tone: Tone,
    pub value: Option<String>,
    pub footprint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum Shape {
    Placement(Placement),
    Track(Track),
    Via(Via),
    Zone(Zone),
    Text(Text),
    Outline(Outline),
    PinLegends(PinLegends),
    Artwork(Artwork),
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Item {
    /// Index into `Design::frames`; none for the board frame.
    pub frame: Option<u32>,
    pub shape: Shape,
}

/// Track width, clearance, and via size, in mm.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Rules {
    pub track_width: f64,
    pub clearance: f64,
    pub via_diameter: f64,
    pub via_drill: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Netclass {
    pub name: String,
    pub nets: Vec<String>,
    pub rules: Rules,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Schematic {
    pub title: Option<String>,
    pub company: Option<String>,
    pub revision: Option<String>,
    pub comment: Option<String>,
    pub library: Option<String>,
}

/// The KiCad project and its release settings.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Project {
    pub name: String,
    pub bom: String,
    pub lcsc: Vec<(String, String)>,
    pub order_quantity: u32,
    pub gerber_layers: String,
    pub unsourced: Option<Vec<String>>,
    pub release_files: Vec<String>,
    pub kicad_dir: String,
    pub schematic: Schematic,
    pub copper_weight_oz: u32,
    pub decisions: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Finish {
    pub mask: String,
    pub silk: String,
    pub copper: String,
}

/// The design. Its nets are those its parts and copper use.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Design {
    pub project: Project,
    /// Nets deliberately connected to fewer than two pads.
    pub no_connect: Vec<String>,
    /// Board-wide rules: the Default net class.
    pub rules: Option<Rules>,
    pub netclasses: Vec<Netclass>,
    /// `.kicad_pro` DRC rule severities.
    pub severities: Vec<(String, String)>,
    pub copper_layers: u32,
    pub hide_references: Vec<String>,
    pub finish: Option<Finish>,
    pub unlink_after_route: Option<String>,
    /// The clearance FreeRouting is given (mm) in place of the board's own;
    /// `None` means 0.7.
    pub router_clearance: Option<f64>,
    pub frames: Vec<Frame>,
    pub items: Vec<Item>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_serde_default_shape() {
        let design: Design = serde_json::from_str(
            r#"{
              "project": {"name": "probe", "bom": "bom.csv", "lcsc": [["MPN", "C1"]], "order_quantity": 1,
                          "gerber_layers": "", "unsourced": null, "release_files": [], "kicad_dir": "output",
                          "schematic": {"title": null, "company": null, "revision": null, "comment": null,
                                        "library": null},
                          "copper_weight_oz": 1, "decisions": []},
              "no_connect": [], "rules": null, "netclasses": [], "severities": [], "copper_layers": 2,
              "hide_references": [], "finish": null, "unlink_after_route": null, "router_clearance": null,
              "frames": [{"parent": null, "at": {"x": 10, "y": 10}, "rotate": 90, "name": null}],
              "items": [
                {"frame": 0, "shape": {"Track": {"net": "A", "width": 0.2, "layer": "F.Cu", "keep": "Laid",
                  "points": [{"Mm": {"x": 0, "y": 0}}, {"Pin": {"reference": "R1", "pad": "1"}}]}}},
                {"frame": null, "shape": {"PinLegends": {"reference": "J1", "legends": [["1", "+"]], "axis": "Row",
                  "at": 1.5, "size": 0.8, "rotate": 0, "layer": "F.SilkS"}}}
              ]
            }"#,
        )
        .unwrap();
        assert_eq!(design.project.lcsc, [("MPN".to_owned(), "C1".to_owned())]);
        assert_eq!(design.project.schematic.title, None);
        assert_eq!(design.frames[0].parent, None);
        let Shape::Track(track) = &design.items[0].shape else { panic!("a track") };
        assert_eq!(track.keep, Keep::Laid);
        assert_eq!(track.points[1], Anchor::Pin(PinRef { reference: "R1".into(), pad: "1".into() }));
        let Shape::PinLegends(legends) = &design.items[1].shape else { panic!("pin legends") };
        assert_eq!(legends.axis, Axis::Row);
    }
}
