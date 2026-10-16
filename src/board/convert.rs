//! The design as Wasmtime lifts it, converted field for field into
//! bb-core's mirror of the same WIT types.

use bb_core::wit as core;

use super::runtime::wit;

impl From<wit::Design> for core::Design {
    fn from(d: wit::Design) -> Self {
        Self {
            project: d.project.into(),
            no_connect: d.no_connect,
            rules: d.rules.map(Into::into),
            netclasses: d.netclasses.into_iter().map(Into::into).collect(),
            severities: d.severities,
            copper_layers: d.copper_layers,
            hide_references: d.hide_references,
            finish: d.finish.map(|f| core::Finish { mask: f.mask, silk: f.silk, copper: f.copper }),
            unlink_after_route: d.unlink_after_route,
            router_clearance: d.router_clearance,
            frames: d.frames.into_iter().map(Into::into).collect(),
            items: d.items.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<wit::Project> for core::Project {
    fn from(p: wit::Project) -> Self {
        let wit::Schematic { title, company, revision, comment, library } = p.schematic;
        Self {
            name: p.name,
            bom: p.bom,
            lcsc: p.lcsc,
            order_quantity: p.order_quantity,
            gerber_layers: p.gerber_layers,
            unsourced: p.unsourced,
            release_files: p.release_files,
            kicad_dir: p.kicad_dir,
            schematic: core::Schematic { title, company, revision, comment, library },
            copper_weight_oz: p.copper_weight_oz,
            decisions: p.decisions,
        }
    }
}

impl From<wit::Rules> for core::Rules {
    fn from(r: wit::Rules) -> Self {
        Self {
            track_width: r.track_width,
            clearance: r.clearance,
            via_diameter: r.via_diameter,
            via_drill: r.via_drill,
        }
    }
}

impl From<wit::Netclass> for core::Netclass {
    fn from(c: wit::Netclass) -> Self {
        Self { name: c.name, nets: c.nets, rules: c.rules.into() }
    }
}

impl From<wit::Frame> for core::Frame {
    fn from(f: wit::Frame) -> Self {
        Self { parent: f.parent, at: f.at.into(), rotate: f.rotate, name: f.name }
    }
}

impl From<wit::Point> for core::Point {
    fn from(p: wit::Point) -> Self {
        Self { x: p.x, y: p.y }
    }
}

impl From<wit::PinRef> for core::PinRef {
    fn from(p: wit::PinRef) -> Self {
        Self { reference: p.reference, pad: p.pad }
    }
}

impl From<wit::Anchor> for core::Anchor {
    fn from(a: wit::Anchor) -> Self {
        match a {
            wit::Anchor::Mm(p) => Self::Mm(p.into()),
            wit::Anchor::Pin(p) => Self::Pin(p.into()),
            wit::Anchor::Offset(o) => Self::Offset(core::PadOffset { pad: o.pad.into(), by: o.by.into() }),
        }
    }
}

fn anchors(anchors: Vec<wit::Anchor>) -> Vec<core::Anchor> {
    anchors.into_iter().map(Into::into).collect()
}

impl From<wit::Model> for core::Model {
    fn from(m: wit::Model) -> Self {
        Self { path: m.path, offset: m.offset }
    }
}

fn models(models: Vec<wit::Model>) -> Vec<core::Model> {
    models.into_iter().map(Into::into).collect()
}

impl From<wit::Footprint> for core::Footprint {
    fn from(f: wit::Footprint) -> Self {
        match f {
            wit::Footprint::Library(part) => Self::Library(core::Part {
                reference: part.reference,
                value: part.value,
                footprint: part.footprint,
                pins: part.pins,
                models: match part.models {
                    wit::Models::Keep => core::Models::Keep,
                    wit::Models::Swap(path) => core::Models::Swap(path),
                    wit::Models::Replace(list) => core::Models::Replace(models(list)),
                },
                assembly_rotation: part.assembly_rotation,
            }),
            wit::Footprint::Generated(g) => Self::Generated(core::Generated {
                reference: g.reference,
                value: g.value,
                footprint: g.footprint,
                pads: g.pads.into_iter().map(Into::into).collect(),
                body: g.body.map(Into::into),
                graphics: g
                    .graphics
                    .into_iter()
                    .map(|l| core::FpLine { layer: l.layer, start: l.start.into(), end: l.end.into(), width: l.width })
                    .collect(),
                models: models(g.models),
                assembly_rotation: g.assembly_rotation,
            }),
        }
    }
}

impl From<wit::Pad> for core::Pad {
    fn from(p: wit::Pad) -> Self {
        Self {
            number: p.number,
            at: p.at.into(),
            size: p.size.into(),
            drill: p.drill,
            shape: match p.shape {
                wit::PadShape::Rect => core::PadShape::Rect,
                wit::PadShape::Circle => core::PadShape::Circle,
                wit::PadShape::Oval => core::PadShape::Oval,
                wit::PadShape::Roundrect => core::PadShape::Roundrect,
            },
            net: p.net,
        }
    }
}

impl From<wit::Item> for core::Item {
    fn from(i: wit::Item) -> Self {
        Self { frame: i.frame, shape: i.shape.into() }
    }
}

impl From<wit::Shape> for core::Shape {
    fn from(s: wit::Shape) -> Self {
        match s {
            wit::Shape::Placement(p) => Self::Placement(core::Placement {
                footprint: p.footprint.into(),
                at: p.at.into(),
                rotate: p.rotate,
                hide_reference: p.hide_reference,
            }),
            wit::Shape::Track(t) => Self::Track(core::Track {
                net: t.net,
                points: anchors(t.points),
                width: t.width,
                layer: t.layer,
                keep: match t.keep {
                    wit::Keep::No => core::Keep::No,
                    wit::Keep::Laid => core::Keep::Laid,
                    wit::Keep::Routed => core::Keep::Routed,
                },
            }),
            wit::Shape::Via(v) => {
                Self::Via(core::Via { net: v.net, at: v.at.into(), diameter: v.diameter, drill: v.drill })
            }
            wit::Shape::Zone(z) => Self::Zone(core::Zone {
                net: z.net,
                layer: z.layer,
                outline: anchors(z.outline),
                priority: z.priority,
                clearance: z.clearance,
                min_thickness: z.min_thickness,
            }),
            wit::Shape::Text(t) => Self::Text(core::Text {
                text: t.text,
                at: t.at.into(),
                size: t.size,
                rotate: t.rotate,
                layer: t.layer,
                thickness: t.thickness,
            }),
            wit::Shape::Outline(o) => Self::Outline(core::Outline {
                layer: o.layer,
                points: anchors(o.points),
                width: o.width,
                closed: o.closed,
            }),
            wit::Shape::PinLegends(l) => Self::PinLegends(core::PinLegends {
                reference: l.reference,
                legends: l.legends,
                axis: match l.axis {
                    wit::Axis::Row => core::Axis::Row,
                    wit::Axis::Column => core::Axis::Column,
                },
                at: l.at,
                size: l.size,
                rotate: l.rotate,
                layer: l.layer,
            }),
            wit::Shape::Artwork(a) => Self::Artwork(core::Artwork {
                reference: a.reference,
                image: a.image,
                center: a.center.into(),
                width: a.width,
                layer: a.layer,
                tone: match a.tone {
                    wit::Tone::Light => core::Tone::Light,
                    wit::Tone::Dark => core::Tone::Dark,
                },
                value: a.value,
                footprint: a.footprint,
            }),
        }
    }
}
