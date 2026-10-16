//! Conversion from the builder types to the WIT design.

#[doc(hidden)]
#[allow(clippy::too_many_arguments)] // generated exports mirror the WIT signatures
pub mod bindings {
    wit_bindgen::generate!({
        path: "../wit",
        world: "board",
        pub_export_macro: true,
        default_bindings_module: "bb_sdk::bindings",
    });
}

use bindings::bb::board::types as w;

use crate::{At, Board, Footprint, Item, Keep, Layer, Model, Models, PadShape, Pin, Point, RoutedTrack, Rules, Tone};

fn point(p: Point) -> w::Point {
    w::Point { x: p.x, y: p.y }
}

fn pin_ref(pin: &Pin) -> w::PinRef {
    w::PinRef { reference: pin.reference.clone(), pad: pin.pad.clone() }
}

fn anchor(at: &At) -> w::Anchor {
    match at {
        At::Mm(p) => w::Anchor::Mm(point(*p)),
        At::Pin(pin) => w::Anchor::Pin(pin_ref(pin)),
        At::Offset(pin, by) => w::Anchor::Offset(w::PadOffset { pad: pin_ref(pin), by: point(*by) }),
    }
}

fn anchors(points: &[At]) -> Vec<w::Anchor> {
    points.iter().map(anchor).collect()
}

fn layer(layer: Layer) -> String {
    layer.name().to_owned()
}

fn model(m: &Model) -> w::Model {
    w::Model { path: m.path.clone(), offset: m.offset }
}

fn rules(r: &Rules) -> w::Rules {
    w::Rules {
        track_width: r.track_width,
        clearance: r.clearance,
        via_diameter: r.via_diameter,
        via_drill: r.via_drill,
    }
}

fn footprint(footprint: Footprint) -> w::Footprint {
    match footprint {
        Footprint::Library(p) => w::Footprint::Library(w::Part {
            reference: p.reference,
            value: p.value,
            footprint: p.footprint,
            pins: p.pins,
            models: match p.models {
                Models::Keep => w::Models::Keep,
                Models::Swap(path) => w::Models::Swap(path),
                Models::Replace(models) => w::Models::Replace(models.iter().map(model).collect()),
            },
            assembly_rotation: p.assembly_rotation,
        }),
        Footprint::Generated(g) => w::Footprint::Generated(w::Generated {
            reference: g.reference,
            value: g.value,
            footprint: g.footprint,
            pads: g
                .pads
                .into_iter()
                .map(|pad| w::Pad {
                    number: pad.number,
                    at: point(pad.at),
                    size: point(pad.size),
                    drill: pad.drill,
                    shape: match pad.shape {
                        PadShape::Rect => w::PadShape::Rect,
                        PadShape::Circle => w::PadShape::Circle,
                        PadShape::Oval => w::PadShape::Oval,
                        PadShape::Roundrect => w::PadShape::Roundrect,
                    },
                    net: pad.net,
                })
                .collect(),
            body: g.body.map(point),
            graphics: g
                .graphics
                .into_iter()
                .map(|line| w::FpLine {
                    layer: layer(line.layer),
                    start: point(line.start),
                    end: point(line.end),
                    width: line.width,
                })
                .collect(),
            models: g.models.iter().map(model).collect(),
            assembly_rotation: g.assembly_rotation,
        }),
    }
}

/// Flatten nested groups into the frame table.
struct Flattener {
    frames: Vec<w::Frame>,
    items: Vec<w::Item>,
}

impl Flattener {
    fn items(&mut self, items: Vec<Item>, frame: Option<u32>) {
        for item in items {
            self.item(item, frame);
        }
    }

    fn push(&mut self, frame: Option<u32>, shape: w::Shape) {
        self.items.push(w::Item { frame, shape });
    }

    fn item(&mut self, item: Item, frame: Option<u32>) {
        let shape = match item {
            Item::Group(g) => {
                self.frames.push(w::Frame { parent: frame, at: point(g.at), rotate: g.rotate, name: g.name });
                let index = u32::try_from(self.frames.len() - 1).expect("frame count fits u32");
                self.items(g.items, Some(index));
                return;
            }
            Item::Placement(p) => w::Shape::Placement(w::Placement {
                footprint: footprint(p.footprint),
                at: point(p.at),
                rotate: p.rotate,
                hide_reference: p.hide_reference,
            }),
            Item::Track(t) => w::Shape::Track(w::Track {
                net: t.net.name().to_owned(),
                points: anchors(&t.points),
                width: t.width,
                layer: layer(t.layer),
                keep: match t.keep {
                    Keep::No => w::Keep::No,
                    Keep::Laid => w::Keep::Laid,
                    Keep::Routed => w::Keep::Routed,
                },
            }),
            Item::Via(v) => w::Shape::Via(w::Via {
                net: v.net.name().to_owned(),
                at: anchor(&v.at),
                diameter: v.diameter,
                drill: v.drill,
            }),
            Item::Zone(z) => w::Shape::Zone(w::Zone {
                net: z.net.name().to_owned(),
                layer: layer(z.layer),
                outline: anchors(&z.outline),
                priority: z.priority,
                clearance: z.clearance,
                min_thickness: z.min_thickness,
            }),
            Item::Text(t) => w::Shape::Text(w::Text {
                text: t.text,
                at: point(t.at),
                size: t.size,
                rotate: t.rotate,
                layer: layer(t.layer),
                thickness: t.thickness,
            }),
            Item::Outline(o) => w::Shape::Outline(w::Outline {
                layer: layer(o.layer),
                points: anchors(&o.points),
                width: o.width,
                closed: o.closed,
            }),
            Item::PinLegends(l) => {
                let (axis, at) = match l.axis {
                    crate::Axis::Row(y) => (w::Axis::Row, y),
                    crate::Axis::Column(x) => (w::Axis::Column, x),
                };
                w::Shape::PinLegends(w::PinLegends {
                    reference: l.reference,
                    legends: l.legends,
                    axis,
                    at,
                    size: l.size,
                    rotate: l.rotate,
                    layer: layer(l.layer),
                })
            }
            Item::Artwork(a) => w::Shape::Artwork(w::Artwork {
                reference: a.reference,
                image: a.image,
                center: point(a.center),
                width: a.width,
                layer: layer(a.layer),
                tone: match a.tone {
                    Tone::Light => w::Tone::Light,
                    Tone::Dark => w::Tone::Dark,
                },
                value: a.value,
                footprint: a.footprint,
            }),
        };
        self.push(frame, shape);
    }
}

#[doc(hidden)]
pub fn to_wit(board: Board) -> w::Design {
    let mut flat = Flattener { frames: Vec::new(), items: Vec::new() };
    flat.items(board.items, None);
    let p = board.project;
    w::Design {
        project: w::Project {
            name: p.name,
            bom: p.bom,
            lcsc: p.lcsc,
            order_quantity: p.order_quantity,
            gerber_layers: p.gerber_layers,
            unsourced: p.unsourced,
            release_files: p.release_files,
            kicad_dir: p.kicad_dir,
            schematic: w::Schematic {
                title: p.schematic.title,
                company: p.schematic.company,
                revision: p.schematic.revision,
                comment: p.schematic.comment,
                library: p.schematic.library,
            },
            copper_weight_oz: p.copper_weight_oz,
            decisions: p.decisions,
        },
        no_connect: board.no_connect.iter().map(|n| n.name().to_owned()).collect(),
        rules: board.rules.as_ref().map(rules),
        netclasses: board
            .netclasses
            .iter()
            .map(|c| w::Netclass {
                name: c.name.clone(),
                nets: c.nets.iter().map(|n| n.name().to_owned()).collect(),
                rules: rules(&c.rules),
            })
            .collect(),
        severities: board.severities,
        copper_layers: board.copper_layers,
        hide_references: board.hide_references,
        finish: board.finish.map(|f| w::Finish { mask: f.mask, silk: f.silk, copper: f.copper }),
        unlink_after_route: board.unlink_after_route,
        router_clearance: board.router_clearance,
        frames: flat.frames,
        items: flat.items,
    }
}

#[doc(hidden)]
pub fn from_wit_track(t: w::RoutedTrack) -> RoutedTrack {
    RoutedTrack {
        net: t.net,
        layer: t.layer,
        width: t.width,
        start: Point { x: t.start.x, y: t.start.y },
        end: Point { x: t.end.x, y: t.end.y },
    }
}
