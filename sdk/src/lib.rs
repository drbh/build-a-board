//! Describe a KiCad board in Rust; `bb` runs it as a WebAssembly component.
//!
//! A board crate is a `cdylib` built for `wasm32-wasip2`. Its build function
//! returns a [`Board`] value; nothing is placed until `bb` resolves it, so
//! declaration order never matters:
//!
//! ```ignore
//! use bb_sdk::*;
//!
//! bb_sdk::nets! { VIN = "VIN", MID = "MID", GND = "GND" }
//!
//! /// A module: nets come in as fields, layout is in the module's own frame.
//! struct Divider { top: Net, mid: Net, bottom: Net }
//!
//! impl Module for Divider {
//!     fn layout(&self) -> Group {
//!         let r1 = resistor(kohm(10), self.top, self.mid).place("R1", (0, 0));
//!         let r2 = resistor(kohm(10), self.mid, self.bottom).place("R2", (0, 4)).rotate(270);
//!         group(items![
//!             track(self.mid, [r1.p2(), r1.p2() + (1.5, 2), r2.p1()]).width(0.25),
//!             r1,
//!             r2,
//!         ])
//!     }
//! }
//!
//! fn build() -> Board {
//!     Board::new(Project::new("my-board")).items(items![Divider { top: VIN, mid: MID, bottom: GND }.at(20, 10)])
//! }
//!
//! bb_sdk::export_board!(build);
//! ```
//!
//! Lengths are millimetres in KiCad's frame (Y down); angles are degrees,
//! counter-clockwise on screen, as KiCad rotates footprints.

mod component;
mod net;
pub mod units;
mod wit;

use std::fmt::Display;
use std::ops::Add;

#[doc(hidden)]
pub use wit::bindings;
#[doc(hidden)]
pub use wit::{from_wit_track, to_wit};

pub use component::*;
pub use net::{NC, Net, Scope};
pub use units::{Farads, Ohms, Volts, kohm, mohm, nf, ohm, pf, uf, v};

pub const R0603: &str = "Resistor_SMD:R_0603_1608Metric";
pub const C0603: &str = "Capacitor_SMD:C_0603_1608Metric";
pub const C0805: &str = "Capacitor_SMD:C_0805_2012Metric";
pub const C1206: &str = "Capacitor_SMD:C_1206_3216Metric";

/// Board layers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    FCu,
    BCu,
    FSilk,
    BSilk,
    FFab,
    BFab,
    FCourtyard,
    BCourtyard,
    FMask,
    BMask,
    EdgeCuts,
}

impl Layer {
    pub fn name(self) -> &'static str {
        match self {
            Self::FCu => "F.Cu",
            Self::BCu => "B.Cu",
            Self::FSilk => "F.SilkS",
            Self::BSilk => "B.SilkS",
            Self::FFab => "F.Fab",
            Self::BFab => "B.Fab",
            Self::FCourtyard => "F.CrtYd",
            Self::BCourtyard => "B.CrtYd",
            Self::FMask => "F.Mask",
            Self::BMask => "B.Mask",
            Self::EdgeCuts => "Edge.Cuts",
        }
    }
}

/// A point in millimetres, in the enclosing group's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl<A: Into<f64>, B: Into<f64>> From<(A, B)> for Point {
    fn from((x, y): (A, B)) -> Self {
        Self { x: x.into(), y: y.into() }
    }
}

/// `at(x, y)`: a point, for building anchor lists.
pub fn at(x: impl Into<f64>, y: impl Into<f64>) -> At {
    At::Mm(Point { x: x.into(), y: y.into() })
}

/// A pad of a placed part, resolved by `bb` once every part is placed.
#[derive(Clone, Debug, PartialEq)]
pub struct Pin {
    pub reference: String,
    pub pad: String,
}

/// A point, a pin, or a point relative to a pin.
#[derive(Clone, Debug, PartialEq)]
pub enum At {
    Mm(Point),
    Pin(Pin),
    /// A pad plus an offset in the enclosing frame; the offset turns with
    /// the frame, so a routed block keeps its shape wherever it goes.
    Offset(Pin, Point),
}

impl<A: Into<f64>, B: Into<f64>> From<(A, B)> for At {
    fn from(xy: (A, B)) -> Self {
        Self::Mm(xy.into())
    }
}

impl From<Point> for At {
    fn from(point: Point) -> Self {
        Self::Mm(point)
    }
}

/// `pad + (dx, dy)`: a point `dx` right of and `dy` below the anchor.
impl<A: Into<f64>, B: Into<f64>> Add<(A, B)> for At {
    type Output = At;

    fn add(self, (dx, dy): (A, B)) -> At {
        let (dx, dy) = (dx.into(), dy.into());
        match self {
            At::Mm(p) => At::Mm(Point { x: p.x + dx, y: p.y + dy }),
            At::Pin(pin) => At::Offset(pin, Point { x: dx, y: dy }),
            At::Offset(pin, by) => At::Offset(pin, Point { x: by.x + dx, y: by.y + dy }),
        }
    }
}

/// Rotate `point`, given relative to `center`, by `angle` degrees
/// counter-clockwise on screen (the direction footprints rotate).
pub fn rotate(center: impl Into<Point>, point: impl Into<Point>, angle: f64) -> Point {
    let (c, p) = (center.into(), point.into());
    let (s, co) = angle.to_radians().sin_cos();
    Point { x: c.x + p.x * co + p.y * s, y: c.y - p.x * s + p.y * co }
}

/// The point `radius` from `center` at `angle` degrees counter-clockwise
/// from +X.
pub fn polar(center: impl Into<Point>, radius: f64, angle: f64) -> Point {
    rotate(center, (radius, 0.0), angle)
}

fn points<P: Into<At>>(points: impl IntoIterator<Item = P>) -> Vec<At> {
    points.into_iter().map(Into::into).collect()
}

fn pairs<A: Display, B: Display>(pairs: impl IntoIterator<Item = (A, B)>) -> Vec<(String, String)> {
    pairs.into_iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

// --- Parts -------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub path: String,
    pub offset: (f64, f64, f64),
}

pub fn model(path: impl Into<String>) -> Model {
    Model { path: path.into(), offset: (0.0, 0.0, 0.0) }
}

impl Model {
    pub fn offset(mut self, x: f64, y: f64, z: f64) -> Self {
        self.offset = (x, y, z);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Models {
    Keep,
    Swap(String),
    Replace(Vec<Model>),
}

/// A stock KiCad library footprint, wired to nets; [`Component::place`]
/// gives it a position.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub reference: String,
    pub value: String,
    pub footprint: String,
    pub pins: Vec<(String, String)>,
    pub models: Models,
    pub assembly_rotation: f64,
}

/// A stock library footprint, written `Library:Name`.
pub fn part(reference: &str, value: impl Into<String>, footprint: &str) -> Part {
    Part {
        reference: reference.to_owned(),
        value: value.into(),
        footprint: footprint.to_owned(),
        pins: Vec::new(),
        models: Models::Keep,
        assembly_rotation: 0.0,
    }
}

impl Part {
    /// Degrees added to this part's rotation in JLCPCB's placement (CPL)
    /// file, for packages JLC's library orients differently from KiCad's.
    pub fn assembly_rotation(mut self, degrees: impl Into<f64>) -> Self {
        self.assembly_rotation = degrees.into();
        self
    }

    /// Pad number → net; `NC` pins are left out.
    pub fn pins<A: Display>(mut self, pins: impl IntoIterator<Item = (A, Net)>) -> Self {
        self.pins.extend(
            pins.into_iter().filter(|(_, net)| !net.is_nc()).map(|(pad, net)| (pad.to_string(), net.name().to_owned())),
        );
        self
    }

    /// Replace the 3D models (an empty list removes them).
    pub fn models(mut self, models: impl IntoIterator<Item = Model>) -> Self {
        self.models = Models::Replace(models.into_iter().collect());
        self
    }

    /// Keep the library model's placement but use another file.
    pub fn swap_model(mut self, path: impl Into<String>) -> Self {
        self.models = Models::Swap(path.into());
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadShape {
    Rect,
    Circle,
    Oval,
    Roundrect,
}

/// A pad of a generated footprint, in footprint-local mm.
#[derive(Clone, Debug, PartialEq)]
pub struct Pad {
    pub number: String,
    pub at: Point,
    pub size: Point,
    pub drill: Option<f64>,
    pub shape: PadShape,
    pub net: Option<String>,
}

pub fn smd(number: impl Display, at: impl Into<Point>, size: impl Into<Point>) -> Pad {
    Pad { number: number.to_string(), at: at.into(), size: size.into(), drill: None, shape: PadShape::Rect, net: None }
}

pub fn tht(number: impl Display, at: impl Into<Point>, size: impl Into<Point>, drill: f64) -> Pad {
    Pad { drill: Some(drill), ..smd(number, at, size) }
}

impl Pad {
    /// Wire the pad; `NC` leaves it unwired.
    pub fn net(mut self, net: Net) -> Self {
        self.net = (!net.is_nc()).then(|| net.name().to_owned());
        self
    }

    pub fn shape(mut self, shape: PadShape) -> Self {
        self.shape = shape;
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FpLine {
    pub layer: Layer,
    pub start: Point,
    pub end: Point,
    pub width: f64,
}

pub fn fp_line(layer: Layer, start: impl Into<Point>, end: impl Into<Point>, width: f64) -> FpLine {
    FpLine { layer, start: start.into(), end: end.into(), width }
}

/// A footprint built from pads and graphics.
#[derive(Clone, Debug, PartialEq)]
pub struct Generated {
    pub reference: String,
    pub value: String,
    pub footprint: String,
    pub pads: Vec<Pad>,
    pub body: Option<Point>,
    pub graphics: Vec<FpLine>,
    pub models: Vec<Model>,
    pub assembly_rotation: f64,
}

pub fn generated(reference: &str, value: impl Into<String>, footprint: impl Into<String>) -> Generated {
    Generated {
        reference: reference.to_owned(),
        value: value.into(),
        footprint: footprint.into(),
        pads: Vec::new(),
        body: None,
        graphics: Vec::new(),
        models: Vec::new(),
        assembly_rotation: 0.0,
    }
}

impl Generated {
    /// Degrees added to this part's rotation in JLCPCB's placement (CPL)
    /// file, for packages JLC's library orients differently from KiCad's.
    pub fn assembly_rotation(mut self, degrees: impl Into<f64>) -> Self {
        self.assembly_rotation = degrees.into();
        self
    }

    pub fn pads(mut self, pads: impl IntoIterator<Item = Pad>) -> Self {
        self.pads.extend(pads);
        self
    }

    /// Fab outline plus a 0.25 mm courtyard around a centred body.
    pub fn body(mut self, width: f64, height: f64) -> Self {
        self.body = Some(Point { x: width, y: height });
        self
    }

    pub fn graphics(mut self, graphics: impl IntoIterator<Item = FpLine>) -> Self {
        self.graphics.extend(graphics);
        self
    }

    pub fn models(mut self, models: impl IntoIterator<Item = Model>) -> Self {
        self.models.extend(models);
        self
    }
}

// --- Copper and graphics -------------------------------------------------------

/// What becomes of a fixed track when the board is routed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keep {
    /// Laid before routing; the router may thin or drop it.
    No,
    /// Laid before routing and put back at least this wide afterwards.
    Laid,
    /// Left to the router, then made at least this wide afterwards.
    Routed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub net: Net,
    pub points: Vec<At>,
    pub width: f64,
    pub layer: Layer,
    pub keep: Keep,
}

/// A copper polyline, 0.4 mm on F.Cu unless refined.
#[track_caller]
pub fn track<P: Into<At>>(net: Net, points: impl IntoIterator<Item = P>) -> Track {
    assert!(!net.is_nc(), "a track needs a net");
    Track { net, points: self::points(points), width: 0.4, layer: Layer::FCu, keep: Keep::No }
}

impl Track {
    pub fn width(mut self, width: f64) -> Self {
        self.width = width;
        self
    }

    pub fn layer(mut self, layer: Layer) -> Self {
        self.layer = layer;
        self
    }

    /// After routing, put every segment back at least this wide.
    pub fn keep(mut self) -> Self {
        self.keep = Keep::Laid;
        self
    }

    /// Leave the track to the router, then make its segments at least this
    /// wide (adding any the router left out).
    pub fn keep_routed(mut self) -> Self {
        self.keep = Keep::Routed;
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Via {
    pub net: Net,
    pub at: At,
    pub diameter: f64,
    pub drill: f64,
}

/// A through via, 1.0 mm with a 0.5 mm drill unless refined.
#[track_caller]
pub fn via(net: Net, at: impl Into<At>) -> Via {
    assert!(!net.is_nc(), "a via needs a net");
    Via { net, at: at.into(), diameter: 1.0, drill: 0.5 }
}

impl Via {
    pub fn size(mut self, diameter: f64, drill: f64) -> Self {
        self.diameter = diameter;
        self.drill = drill;
        self
    }
}

/// A fixed route that changes layer through vias: `route(net).to(a).to(b)
/// .via(Layer::BCu).to(c)` lays `a-b` on F.Cu, a via at `b`, and `b-c` on
/// B.Cu. A route may end in a via (a stub into a plane).
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    net: Net,
    width: f64,
    via: (f64, f64),
    layer: Layer,
    /// The points on the current layer, from the last via.
    run: Vec<At>,
    items: Vec<Item>,
}

/// A multi-layer run on `net`: 0.4 mm tracks and 1.0/0.5 mm vias, starting
/// on F.Cu, unless refined.
#[track_caller]
pub fn route(net: Net) -> Route {
    assert!(!net.is_nc(), "a route needs a net");
    Route { net, width: 0.4, via: (1.0, 0.5), layer: Layer::FCu, run: Vec::new(), items: Vec::new() }
}

impl Route {
    pub fn width(mut self, width: f64) -> Self {
        self.width = width;
        self
    }

    pub fn via_size(mut self, diameter: f64, drill: f64) -> Self {
        self.via = (diameter, drill);
        self
    }

    /// Start on `layer` rather than F.Cu.
    #[track_caller]
    pub fn layer(mut self, layer: Layer) -> Self {
        assert!(self.run.is_empty(), "set the route's layer before its first point; change layers with .via()");
        self.layer = layer;
        self
    }

    /// Continue the current layer's track to `at`.
    pub fn to(mut self, at: impl Into<At>) -> Self {
        self.run.push(at.into());
        self
    }

    /// Drop through a via here and continue on `layer`.
    #[track_caller]
    pub fn via(mut self, layer: Layer) -> Self {
        let here = self.run.last().cloned().expect("a via needs a point to sit on; call .to() first");
        self.flush();
        self.items.push(via(self.net, here.clone()).size(self.via.0, self.via.1).into());
        self.run.push(here);
        self.layer = layer;
        self
    }

    fn flush(&mut self) {
        let run = std::mem::take(&mut self.run);
        if run.len() >= 2 {
            self.items.push(track(self.net, run).width(self.width).layer(self.layer).into());
        }
    }
}

impl IntoItems for Route {
    fn into_items(mut self, out: &mut Vec<Item>) {
        self.flush();
        out.extend(self.items);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Zone {
    pub net: Net,
    pub layer: Layer,
    pub outline: Vec<At>,
    pub priority: u32,
    pub clearance: f64,
    pub min_thickness: f64,
}

/// A copper pour, filled by KiCad.
#[track_caller]
pub fn zone<P: Into<At>>(net: Net, layer: Layer, outline: impl IntoIterator<Item = P>) -> Zone {
    assert!(!net.is_nc(), "a zone needs a net");
    Zone { net, layer, outline: points(outline), priority: 0, clearance: 0.3, min_thickness: 0.25 }
}

impl Zone {
    pub fn priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    pub fn clearance(mut self, clearance: f64) -> Self {
        self.clearance = clearance;
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub text: String,
    pub at: Point,
    pub size: f64,
    pub rotate: f64,
    pub layer: Layer,
    pub thickness: f64,
}

/// Front silkscreen text, 1.2 mm unless refined.
pub fn text(text: impl Into<String>, at: impl Into<Point>) -> Text {
    Text { text: text.into(), at: at.into(), size: 1.2, rotate: 0.0, layer: Layer::FSilk, thickness: 0.18 }
}

impl Text {
    pub fn size(mut self, size: f64) -> Self {
        self.size = size;
        self
    }

    pub fn rotate(mut self, degrees: impl Into<f64>) -> Self {
        self.rotate = degrees.into();
        self
    }

    pub fn layer(mut self, layer: Layer) -> Self {
        self.layer = layer;
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Outline {
    pub layer: Layer,
    pub points: Vec<At>,
    pub width: f64,
    pub closed: bool,
}

/// A closed graphic polyline: board edge, silkscreen boxes.
pub fn outline<P: Into<At>>(layer: Layer, points: impl IntoIterator<Item = P>, width: f64) -> Outline {
    Outline { layer, points: self::points(points), width, closed: true }
}

impl Outline {
    pub fn open(mut self) -> Self {
        self.closed = false;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Axis {
    Row(f64),
    Column(f64),
}

/// Legends beside a part's pads; made by [`Placed::legends_row`] and
/// [`Placed::legends_column`].
#[derive(Clone, Debug, PartialEq)]
pub struct PinLegends {
    pub reference: String,
    pub legends: Vec<(String, String)>,
    pub axis: Axis,
    pub size: f64,
    pub rotate: f64,
    pub layer: Layer,
}

impl PinLegends {
    pub fn size(mut self, size: f64) -> Self {
        self.size = size;
        self
    }

    pub fn rotate(mut self, degrees: impl Into<f64>) -> Self {
        self.rotate = degrees.into();
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Artwork {
    pub reference: String,
    pub image: String,
    pub center: Point,
    pub width: f64,
    pub layer: Layer,
    pub tone: Tone,
    pub value: Option<String>,
    pub footprint: Option<String>,
}

/// A PNG in the board directory traced onto silkscreen.
pub fn artwork(reference: &str, image: &str, center: impl Into<Point>, width: impl Into<f64>) -> Artwork {
    Artwork {
        reference: reference.to_owned(),
        image: image.to_owned(),
        center: center.into(),
        width: width.into(),
        layer: Layer::FSilk,
        tone: Tone::Light,
        value: None,
        footprint: None,
    }
}

impl Artwork {
    pub fn layer(mut self, layer: Layer) -> Self {
        self.layer = layer;
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn footprint(mut self, footprint: impl Into<String>) -> Self {
        self.footprint = Some(footprint.into());
        self
    }
}

// --- Composition ---------------------------------------------------------------

/// A functional block: its ports and parameters are fields, its layout is
/// a group in its own frame.
pub trait Module {
    fn layout(&self) -> Group;

    /// The layout with its origin at `(x, y)`.
    fn at(&self, x: impl Into<f64>, y: impl Into<f64>) -> Group {
        self.layout().at(x, y)
    }
}

/// Anything that can sit on a board.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Placement(Placement),
    Track(Track),
    Via(Via),
    Zone(Zone),
    Text(Text),
    Outline(Outline),
    PinLegends(PinLegends),
    Artwork(Artwork),
    Group(Group),
}

/// Items laid out in their own frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub items: Vec<Item>,
    pub at: Point,
    pub rotate: f64,
    pub name: Option<String>,
}

pub fn group(items: impl IntoItems) -> Group {
    let mut out = Vec::new();
    items.into_items(&mut out);
    Group { items: out, at: Point { x: 0.0, y: 0.0 }, rotate: 0.0, name: None }
}

impl Group {
    /// Place the group's origin at `(x, y)` in the enclosing frame.
    pub fn at(mut self, x: impl Into<f64>, y: impl Into<f64>) -> Self {
        self.at = Point { x: x.into(), y: y.into() };
        self
    }

    /// Rotate the group counter-clockwise (as KiCad rotates footprints).
    pub fn rotate(mut self, degrees: impl Into<f64>) -> Self {
        self.rotate = degrees.into();
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// Things that flatten into a list of items: single items, groups,
/// `Vec`s, arrays, and `Option`s of them.
pub trait IntoItems {
    fn into_items(self, out: &mut Vec<Item>);
}

macro_rules! item {
    ($($t:ident),*) => {$(
        impl From<$t> for Item {
            fn from(value: $t) -> Self {
                Item::$t(value)
            }
        }
        impl IntoItems for $t {
            fn into_items(self, out: &mut Vec<Item>) {
                out.push(Item::$t(self));
            }
        }
    )*};
}
item!(Placement, Track, Via, Zone, Text, Outline, PinLegends, Artwork, Group);

impl IntoItems for Item {
    fn into_items(self, out: &mut Vec<Item>) {
        out.push(self);
    }
}

impl<T: IntoItems> IntoItems for Vec<T> {
    fn into_items(self, out: &mut Vec<Item>) {
        self.into_iter().for_each(|item| item.into_items(out));
    }
}

impl<T: IntoItems, const N: usize> IntoItems for [T; N] {
    fn into_items(self, out: &mut Vec<Item>) {
        self.into_iter().for_each(|item| item.into_items(out));
    }
}

impl<T: IntoItems> IntoItems for Option<T> {
    fn into_items(self, out: &mut Vec<Item>) {
        if let Some(item) = self {
            item.into_items(out);
        }
    }
}

/// `items![a, b, c]`: a heterogeneous list of anything [`IntoItems`].
#[macro_export]
macro_rules! items {
    ($($item:expr),* $(,)?) => {{
        let mut out: ::std::vec::Vec<$crate::Item> = ::std::vec::Vec::new();
        $( $crate::IntoItems::into_items($item, &mut out); )*
        out
    }};
}

// --- The board -------------------------------------------------------------------

/// Track width, clearance, and via size for a set of nets.
#[derive(Clone, Debug, PartialEq)]
pub struct Rules {
    pub track_width: f64,
    pub clearance: f64,
    pub via_diameter: f64,
    pub via_drill: f64,
}

/// Rules with 0.8/0.4 mm vias unless refined.
pub fn rules(track_width: f64, clearance: f64) -> Rules {
    Rules { track_width, clearance, via_diameter: 0.8, via_drill: 0.4 }
}

impl Rules {
    pub fn via(mut self, diameter: f64, drill: f64) -> Self {
        self.via_diameter = diameter;
        self.via_drill = drill;
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NetClass {
    pub name: String,
    pub nets: Vec<Net>,
    pub rules: Rules,
}

/// A net class: `track_width` and 0.25 mm clearance for `nets`, refined
/// with [`NetClass::clearance`] and [`NetClass::via`].
pub fn netclass(name: &str, track_width: f64, nets: impl IntoIterator<Item = impl Into<Net>>) -> NetClass {
    NetClass {
        name: name.to_owned(),
        nets: nets.into_iter().map(Into::into).collect(),
        rules: rules(track_width, 0.25),
    }
}

impl NetClass {
    pub fn via(mut self, diameter: f64, drill: f64) -> Self {
        self.rules = self.rules.via(diameter, drill);
        self
    }

    pub fn clearance(mut self, clearance: f64) -> Self {
        self.rules.clearance = clearance;
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Schematic {
    pub title: Option<String>,
    pub company: Option<String>,
    pub revision: Option<String>,
    pub comment: Option<String>,
    pub library: Option<String>,
}

/// The KiCad project and its release settings.
#[derive(Clone, Debug, PartialEq)]
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

impl Project {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            bom: "bom/approved-parts.csv".into(),
            lcsc: Vec::new(),
            order_quantity: 1,
            gerber_layers: "F.Cu,B.Cu,F.Mask,B.Mask,F.Silkscreen,B.Silkscreen,Edge.Cuts".into(),
            unsourced: None,
            release_files: Vec::new(),
            kicad_dir: "output".into(),
            schematic: Schematic::default(),
            copper_weight_oz: 1,
            decisions: Vec::new(),
        }
    }

    pub fn lcsc<A: Display, B: Display>(mut self, parts: impl IntoIterator<Item = (A, B)>) -> Self {
        self.lcsc = pairs(parts);
        self
    }

    pub fn order_quantity(mut self, boards: u32) -> Self {
        self.order_quantity = boards;
        self
    }

    pub fn release_files<S: Display>(mut self, files: impl IntoIterator<Item = S>) -> Self {
        self.release_files = files.into_iter().map(|f| f.to_string()).collect();
        self
    }

    /// Where the KiCad project lives: `output` (default) or `.` to commit it
    /// beside the sources.
    pub fn kicad_dir(mut self, dir: &str) -> Self {
        self.kicad_dir = dir.to_owned();
        self
    }

    pub fn schematic(mut self, schematic: Schematic) -> Self {
        self.schematic = schematic;
        self
    }

    pub fn decisions<A: Display, B: Display>(mut self, decisions: impl IntoIterator<Item = (A, B)>) -> Self {
        self.decisions = pairs(decisions);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Finish {
    pub mask: String,
    pub silk: String,
    pub copper: String,
}

/// The whole board. Its nets are whatever its parts and copper use.
#[derive(Clone, Debug, PartialEq)]
pub struct Board {
    pub project: Project,
    pub no_connect: Vec<Net>,
    pub rules: Option<Rules>,
    pub netclasses: Vec<NetClass>,
    pub severities: Vec<(String, String)>,
    pub copper_layers: u32,
    pub hide_references: Vec<String>,
    pub finish: Option<Finish>,
    pub unlink_after_route: Option<String>,
    pub router_clearance: Option<f64>,
    pub items: Vec<Item>,
}

impl Board {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            no_connect: Vec::new(),
            rules: None,
            netclasses: Vec::new(),
            severities: Vec::new(),
            copper_layers: 2,
            hide_references: Vec::new(),
            finish: None,
            unlink_after_route: None,
            router_clearance: None,
            items: Vec::new(),
        }
    }

    /// Nets deliberately left with a single (or no) connection.
    pub fn no_connect(mut self, nets: impl IntoIterator<Item = impl Into<Net>>) -> Self {
        self.no_connect.extend(nets.into_iter().map(Into::into));
        self
    }

    /// Board-wide rules: KiCad's Default net class, for every net not in
    /// another class, and what the router works to.
    pub fn rules(mut self, rules: Rules) -> Self {
        self.rules = Some(rules);
        self
    }

    pub fn netclasses(mut self, classes: impl IntoIterator<Item = NetClass>) -> Self {
        self.netclasses.extend(classes);
        self
    }

    /// `.kicad_pro` DRC rule severities, e.g. `("courtyards_overlap", "warning")`.
    pub fn severities<A: Display, B: Display>(mut self, severities: impl IntoIterator<Item = (A, B)>) -> Self {
        self.severities.extend(pairs(severities));
        self
    }

    pub fn copper_layers(mut self, layers: u32) -> Self {
        self.copper_layers = layers;
        self
    }

    /// References with these prefixes get no silkscreen designator.
    pub fn hide_references<S: Display>(mut self, prefixes: impl IntoIterator<Item = S>) -> Self {
        self.hide_references.extend(prefixes.into_iter().map(|p| p.to_string()));
        self
    }

    pub fn finish(mut self, mask: &str, silk: &str, copper: &str) -> Self {
        self.finish = Some(Finish { mask: mask.into(), silk: silk.into(), copper: copper.into() });
        self
    }

    /// After routing, detach this library's generated footprints (artwork
    /// excepted) and hide their designators.
    pub fn unlink_after_route(mut self, library: &str) -> Self {
        self.unlink_after_route = Some(library.to_owned());
        self
    }

    /// The clearance FreeRouting routes with, in mm (default 0.7). It
    /// under-honours small clearances, so `bb` widens them; boards with
    /// fine-pitch parts (0.5 mm USB-C, QFNs) need a smaller value.
    pub fn router_clearance(mut self, mm: f64) -> Self {
        self.router_clearance = Some(mm);
        self
    }

    pub fn items(mut self, items: impl IntoItems) -> Self {
        items.into_items(&mut self.items);
        self
    }
}

/// A routed segment offered to the board's route-width function.
#[derive(Clone, Debug, PartialEq)]
pub struct RoutedTrack {
    pub net: String,
    pub layer: String,
    pub width: f64,
    pub start: Point,
    pub end: Point,
}

impl RoutedTrack {
    pub fn is_on(&self, net: Net) -> bool {
        self.net == net.name()
    }
}

/// The default route-width rule: keep every routed width.
pub fn keep_width(_: &RoutedTrack) -> Option<f64> {
    None
}

/// Export `build` (and optionally a route-width rule) as the board component.
///
/// ```ignore
/// bb_sdk::export_board!(build);
/// bb_sdk::export_board!(build, route_width);
/// ```
#[macro_export]
macro_rules! export_board {
    ($build:path) => {
        $crate::export_board!($build, $crate::keep_width);
    };
    ($build:path, $route_width:path) => {
        struct BbBoard;

        impl $crate::bindings::Guest for BbBoard {
            fn build() -> ::std::result::Result<$crate::bindings::bb::board::types::Design, ::std::string::String> {
                ::std::result::Result::Ok($crate::to_wit($build()))
            }

            fn route_width(track: $crate::bindings::bb::board::types::RoutedTrack) -> ::std::option::Option<f64> {
                $route_width(&$crate::from_wit_track(track))
            }
        }

        $crate::bindings::export!(BbBoard with_types_in $crate::bindings);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: Net = Net::new("N");

    fn pad(reference: &str, pad: &str) -> At {
        At::Pin(Pin { reference: reference.into(), pad: pad.into() })
    }

    #[test]
    fn offsets_accumulate_on_pins_and_points() {
        assert_eq!(
            pad("U1", "1") + (1, 0.5) + (1, 0),
            At::Offset(Pin { reference: "U1".into(), pad: "1".into() }, Point { x: 2.0, y: 0.5 })
        );
        assert_eq!(at(1, 2) + (0.5, 0.5), at(1.5, 2.5));
    }

    #[test]
    fn rotation_is_counter_clockwise_on_screen() {
        let p = rotate((0, 0), (1, 0), 90.0);
        assert!((p.x).abs() < 1e-12 && (p.y + 1.0).abs() < 1e-12, "{p:?}");
        assert_eq!(polar((10, 10), 2.0, 0.0), Point { x: 12.0, y: 10.0 });
    }

    #[test]
    fn a_route_lays_tracks_and_vias_in_order() {
        let (a, b, c) = (pad("R1", "1"), at(1, 1), pad("R2", "2"));
        let laid = items![
            route(N)
                .width(0.2)
                .via_size(0.6, 0.3)
                .to(a.clone())
                .to(b.clone())
                .via(Layer::BCu)
                .to(c.clone())
                .via(Layer::FCu)
        ];
        assert_eq!(
            laid,
            items![
                track(N, [a, b.clone()]).width(0.2),
                via(N, b.clone()).size(0.6, 0.3),
                track(N, [b, c.clone()]).width(0.2).layer(Layer::BCu),
                via(N, c).size(0.6, 0.3),
            ]
        );
    }
}
