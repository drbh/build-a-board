// @generated from wit/*.wit by `cargo run -p bb-sdk-bindgen`; do not edit.
// wit-hash: 976f96f6eb715f7c

// Included by sdk/src/json.rs, which brings `ToJson` and `key` into scope.
use crate::bindings::bb::board::types as w;

impl ToJson for w::Point {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "x");
        self.x.to_json(out);
        key(out, ",", "y");
        self.y.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::PinRef {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "reference");
        self.reference.to_json(out);
        key(out, ",", "pad");
        self.pad.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::PadOffset {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "pad");
        self.pad.to_json(out);
        key(out, ",", "by");
        self.by.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Anchor {
    fn to_json(&self, out: &mut String) {
        match self {
            w::Anchor::Mm(value) => {
                key(out, "{", "Mm");
                value.to_json(out);
                out.push('}');
            }
            w::Anchor::Pin(value) => {
                key(out, "{", "Pin");
                value.to_json(out);
                out.push('}');
            }
            w::Anchor::Offset(value) => {
                key(out, "{", "Offset");
                value.to_json(out);
                out.push('}');
            }
        }
    }
}

impl ToJson for w::Frame {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "parent");
        self.parent.to_json(out);
        key(out, ",", "at");
        self.at.to_json(out);
        key(out, ",", "rotate");
        self.rotate.to_json(out);
        key(out, ",", "name");
        self.name.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Model {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "path");
        self.path.to_json(out);
        key(out, ",", "offset");
        self.offset.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Models {
    fn to_json(&self, out: &mut String) {
        match self {
            w::Models::Keep => out.push_str("\"Keep\""),
            w::Models::Swap(value) => {
                key(out, "{", "Swap");
                value.to_json(out);
                out.push('}');
            }
            w::Models::Replace(value) => {
                key(out, "{", "Replace");
                value.to_json(out);
                out.push('}');
            }
        }
    }
}

impl ToJson for w::Part {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "reference");
        self.reference.to_json(out);
        key(out, ",", "value");
        self.value.to_json(out);
        key(out, ",", "footprint");
        self.footprint.to_json(out);
        key(out, ",", "pins");
        self.pins.to_json(out);
        key(out, ",", "models");
        self.models.to_json(out);
        key(out, ",", "assembly_rotation");
        self.assembly_rotation.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::PadShape {
    fn to_json(&self, out: &mut String) {
        out.push_str(match self {
            w::PadShape::Rect => "\"Rect\"",
            w::PadShape::Circle => "\"Circle\"",
            w::PadShape::Oval => "\"Oval\"",
            w::PadShape::Roundrect => "\"Roundrect\"",
        });
    }
}

impl ToJson for w::Pad {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "number");
        self.number.to_json(out);
        key(out, ",", "at");
        self.at.to_json(out);
        key(out, ",", "size");
        self.size.to_json(out);
        key(out, ",", "drill");
        self.drill.to_json(out);
        key(out, ",", "shape");
        self.shape.to_json(out);
        key(out, ",", "net");
        self.net.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::FpLine {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "layer");
        self.layer.to_json(out);
        key(out, ",", "start");
        self.start.to_json(out);
        key(out, ",", "end");
        self.end.to_json(out);
        key(out, ",", "width");
        self.width.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Generated {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "reference");
        self.reference.to_json(out);
        key(out, ",", "value");
        self.value.to_json(out);
        key(out, ",", "footprint");
        self.footprint.to_json(out);
        key(out, ",", "pads");
        self.pads.to_json(out);
        key(out, ",", "body");
        self.body.to_json(out);
        key(out, ",", "graphics");
        self.graphics.to_json(out);
        key(out, ",", "models");
        self.models.to_json(out);
        key(out, ",", "assembly_rotation");
        self.assembly_rotation.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Footprint {
    fn to_json(&self, out: &mut String) {
        match self {
            w::Footprint::Library(value) => {
                key(out, "{", "Library");
                value.to_json(out);
                out.push('}');
            }
            w::Footprint::Generated(value) => {
                key(out, "{", "Generated");
                value.to_json(out);
                out.push('}');
            }
        }
    }
}

impl ToJson for w::Placement {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "footprint");
        self.footprint.to_json(out);
        key(out, ",", "at");
        self.at.to_json(out);
        key(out, ",", "rotate");
        self.rotate.to_json(out);
        key(out, ",", "hide_reference");
        self.hide_reference.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Keep {
    fn to_json(&self, out: &mut String) {
        out.push_str(match self {
            w::Keep::No => "\"No\"",
            w::Keep::Laid => "\"Laid\"",
            w::Keep::Routed => "\"Routed\"",
        });
    }
}

impl ToJson for w::Track {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "net");
        self.net.to_json(out);
        key(out, ",", "points");
        self.points.to_json(out);
        key(out, ",", "width");
        self.width.to_json(out);
        key(out, ",", "layer");
        self.layer.to_json(out);
        key(out, ",", "keep");
        self.keep.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Via {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "net");
        self.net.to_json(out);
        key(out, ",", "at");
        self.at.to_json(out);
        key(out, ",", "diameter");
        self.diameter.to_json(out);
        key(out, ",", "drill");
        self.drill.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Zone {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "net");
        self.net.to_json(out);
        key(out, ",", "layer");
        self.layer.to_json(out);
        key(out, ",", "outline");
        self.outline.to_json(out);
        key(out, ",", "priority");
        self.priority.to_json(out);
        key(out, ",", "clearance");
        self.clearance.to_json(out);
        key(out, ",", "min_thickness");
        self.min_thickness.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Text {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "text");
        self.text.to_json(out);
        key(out, ",", "at");
        self.at.to_json(out);
        key(out, ",", "size");
        self.size.to_json(out);
        key(out, ",", "rotate");
        self.rotate.to_json(out);
        key(out, ",", "layer");
        self.layer.to_json(out);
        key(out, ",", "thickness");
        self.thickness.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Outline {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "layer");
        self.layer.to_json(out);
        key(out, ",", "points");
        self.points.to_json(out);
        key(out, ",", "width");
        self.width.to_json(out);
        key(out, ",", "closed");
        self.closed.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Axis {
    fn to_json(&self, out: &mut String) {
        out.push_str(match self {
            w::Axis::Row => "\"Row\"",
            w::Axis::Column => "\"Column\"",
        });
    }
}

impl ToJson for w::PinLegends {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "reference");
        self.reference.to_json(out);
        key(out, ",", "legends");
        self.legends.to_json(out);
        key(out, ",", "axis");
        self.axis.to_json(out);
        key(out, ",", "at");
        self.at.to_json(out);
        key(out, ",", "size");
        self.size.to_json(out);
        key(out, ",", "rotate");
        self.rotate.to_json(out);
        key(out, ",", "layer");
        self.layer.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Tone {
    fn to_json(&self, out: &mut String) {
        out.push_str(match self {
            w::Tone::Light => "\"Light\"",
            w::Tone::Dark => "\"Dark\"",
        });
    }
}

impl ToJson for w::Artwork {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "reference");
        self.reference.to_json(out);
        key(out, ",", "image");
        self.image.to_json(out);
        key(out, ",", "center");
        self.center.to_json(out);
        key(out, ",", "width");
        self.width.to_json(out);
        key(out, ",", "layer");
        self.layer.to_json(out);
        key(out, ",", "tone");
        self.tone.to_json(out);
        key(out, ",", "value");
        self.value.to_json(out);
        key(out, ",", "footprint");
        self.footprint.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Shape {
    fn to_json(&self, out: &mut String) {
        match self {
            w::Shape::Placement(value) => {
                key(out, "{", "Placement");
                value.to_json(out);
                out.push('}');
            }
            w::Shape::Track(value) => {
                key(out, "{", "Track");
                value.to_json(out);
                out.push('}');
            }
            w::Shape::Via(value) => {
                key(out, "{", "Via");
                value.to_json(out);
                out.push('}');
            }
            w::Shape::Zone(value) => {
                key(out, "{", "Zone");
                value.to_json(out);
                out.push('}');
            }
            w::Shape::Text(value) => {
                key(out, "{", "Text");
                value.to_json(out);
                out.push('}');
            }
            w::Shape::Outline(value) => {
                key(out, "{", "Outline");
                value.to_json(out);
                out.push('}');
            }
            w::Shape::PinLegends(value) => {
                key(out, "{", "PinLegends");
                value.to_json(out);
                out.push('}');
            }
            w::Shape::Artwork(value) => {
                key(out, "{", "Artwork");
                value.to_json(out);
                out.push('}');
            }
        }
    }
}

impl ToJson for w::Item {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "frame");
        self.frame.to_json(out);
        key(out, ",", "shape");
        self.shape.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Rules {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "track_width");
        self.track_width.to_json(out);
        key(out, ",", "clearance");
        self.clearance.to_json(out);
        key(out, ",", "via_diameter");
        self.via_diameter.to_json(out);
        key(out, ",", "via_drill");
        self.via_drill.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Netclass {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "name");
        self.name.to_json(out);
        key(out, ",", "nets");
        self.nets.to_json(out);
        key(out, ",", "rules");
        self.rules.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Schematic {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "title");
        self.title.to_json(out);
        key(out, ",", "company");
        self.company.to_json(out);
        key(out, ",", "revision");
        self.revision.to_json(out);
        key(out, ",", "comment");
        self.comment.to_json(out);
        key(out, ",", "library");
        self.library.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Project {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "name");
        self.name.to_json(out);
        key(out, ",", "bom");
        self.bom.to_json(out);
        key(out, ",", "lcsc");
        self.lcsc.to_json(out);
        key(out, ",", "order_quantity");
        self.order_quantity.to_json(out);
        key(out, ",", "gerber_layers");
        self.gerber_layers.to_json(out);
        key(out, ",", "unsourced");
        self.unsourced.to_json(out);
        key(out, ",", "release_files");
        self.release_files.to_json(out);
        key(out, ",", "kicad_dir");
        self.kicad_dir.to_json(out);
        key(out, ",", "schematic");
        self.schematic.to_json(out);
        key(out, ",", "copper_weight_oz");
        self.copper_weight_oz.to_json(out);
        key(out, ",", "decisions");
        self.decisions.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Finish {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "mask");
        self.mask.to_json(out);
        key(out, ",", "silk");
        self.silk.to_json(out);
        key(out, ",", "copper");
        self.copper.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::Design {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "project");
        self.project.to_json(out);
        key(out, ",", "no_connect");
        self.no_connect.to_json(out);
        key(out, ",", "rules");
        self.rules.to_json(out);
        key(out, ",", "netclasses");
        self.netclasses.to_json(out);
        key(out, ",", "severities");
        self.severities.to_json(out);
        key(out, ",", "copper_layers");
        self.copper_layers.to_json(out);
        key(out, ",", "hide_references");
        self.hide_references.to_json(out);
        key(out, ",", "finish");
        self.finish.to_json(out);
        key(out, ",", "unlink_after_route");
        self.unlink_after_route.to_json(out);
        key(out, ",", "router_clearance");
        self.router_clearance.to_json(out);
        key(out, ",", "frames");
        self.frames.to_json(out);
        key(out, ",", "items");
        self.items.to_json(out);
        out.push('}');
    }
}

impl ToJson for w::RoutedTrack {
    fn to_json(&self, out: &mut String) {
        out.push('{');
        key(out, "", "net");
        self.net.to_json(out);
        key(out, ",", "layer");
        self.layer.to_json(out);
        key(out, ",", "width");
        self.width.to_json(out);
        key(out, ",", "start");
        self.start.to_json(out);
        key(out, ",", "end");
        self.end.to_json(out);
        out.push('}');
    }
}
