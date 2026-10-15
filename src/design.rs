//! The resolved board design: what the Starlark board file evaluates to and
//! what the KiCad writer serialises.
//!
//! Geometry is in integer nanometres. Footprint-local geometry of generated
//! footprints is stored the way pcbnew stores it after `SetPosition`: the
//! absolute position is rounded first and the local offset is the difference,
//! so designs reproduce pcbnew's arithmetic exactly.

use std::collections::BTreeSet;

use crate::units::{Nm, Pt};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    /// KiCad's canonical layer name.
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

    /// Accepts KiCad's canonical and user-facing names (`F.SilkS`, `F.Silkscreen`).
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "F.Cu" => Self::FCu,
            "B.Cu" => Self::BCu,
            "F.SilkS" | "F.Silkscreen" => Self::FSilk,
            "B.SilkS" | "B.Silkscreen" => Self::BSilk,
            "F.Fab" => Self::FFab,
            "B.Fab" => Self::BFab,
            "F.CrtYd" | "F.Courtyard" => Self::FCourtyard,
            "B.CrtYd" | "B.Courtyard" => Self::BCourtyard,
            "F.Mask" => Self::FMask,
            "B.Mask" => Self::BMask,
            "Edge.Cuts" => Self::EdgeCuts,
            _ => return None,
        })
    }

    pub fn is_back(self) -> bool {
        matches!(self, Self::BCu | Self::BSilk | Self::BFab | Self::BCourtyard | Self::BMask)
    }

    pub fn is_copper(self) -> bool {
        matches!(self, Self::FCu | Self::BCu)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadShape {
    Rect,
    Circle,
    Oval,
    Roundrect,
}

impl PadShape {
    pub fn name(self) -> &'static str {
        match self {
            Self::Rect => "rect",
            Self::Circle => "circle",
            Self::Oval => "oval",
            Self::Roundrect => "roundrect",
        }
    }
}

/// A complete board design.
#[derive(Clone, Debug, Default)]
pub struct Board {
    pub copper_layers: u8,
    /// Declared nets, independent of use.
    pub nets: BTreeSet<String>,
    pub netclasses: Vec<NetClass>,
    pub footprints: Vec<Footprint>,
    pub graphics: Vec<Graphic>,
    pub texts: Vec<Text>,
    pub tracks: Vec<Track>,
    pub vias: Vec<Via>,
    pub zones: Vec<Zone>,
    /// `.kicad_pro` DRC rule severities, e.g. `courtyards_overlap = warning`.
    pub rule_severities: Vec<(String, String)>,
    pub finish: Option<Finish>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NetClass {
    pub name: String,
    pub track_width: Nm,
    pub clearance: Nm,
    pub via_diameter: Nm,
    pub via_drill: Nm,
    /// Net name patterns assigned to this class.
    pub nets: Vec<String>,
}

/// Soldermask/silkscreen colours and copper finish written to the stackup.
#[derive(Clone, Debug, PartialEq)]
pub struct Finish {
    pub mask: String,
    pub silk: String,
    pub copper: String,
}

#[derive(Clone, Debug)]
pub struct Footprint {
    pub reference: String,
    pub value: String,
    pub at: Pt,
    /// Orientation in degrees, counter-clockwise on screen.
    pub angle: f64,
    pub hide_reference: bool,
    pub source: Source,
}

#[derive(Clone, Debug)]
pub enum Source {
    /// A stock library footprint, loaded and transformed at write time.
    Library(LibraryFootprint),
    /// A footprint generated from pads and graphics.
    Generated(GeneratedFootprint),
}

#[derive(Clone, Debug)]
pub struct LibraryFootprint {
    pub library: String,
    pub name: String,
    /// Pad number → net.
    pub pins: Vec<(String, String)>,
    pub models: ModelEdit,
}

#[derive(Clone, Debug, Default)]
pub enum ModelEdit {
    /// Keep the library's 3D models.
    #[default]
    Keep,
    /// Keep the first model's placement, swap its file.
    RenameFirst(String),
    /// Replace every model.
    Replace(Vec<Model>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub path: String,
    pub offset: [f64; 3],
}

#[derive(Clone, Debug, Default)]
pub struct GeneratedFootprint {
    pub library: String,
    pub name: String,
    pub pads: Vec<Pad>,
    pub graphics: Vec<Graphic>,
    pub models: Vec<Model>,
    pub exclude_from_bom: bool,
    pub exclude_from_pos: bool,
}

/// A pad of a generated footprint; `at` is footprint-local.
#[derive(Clone, Debug, PartialEq)]
pub struct Pad {
    pub number: String,
    pub at: Pt,
    pub size: (Nm, Nm),
    pub drill: Option<Nm>,
    pub shape: PadShape,
    pub net: Option<String>,
}

/// A graphic item: board-frame on the board, footprint-local in a footprint.
#[derive(Clone, Debug, PartialEq)]
pub enum Graphic {
    Line {
        layer: Layer,
        start: Pt,
        end: Pt,
        width: Nm,
    },
    /// A filled polygon, already in KiCad's saved (fractured) form.
    Poly {
        layer: Layer,
        points: Vec<Pt>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub text: String,
    pub at: Pt,
    pub layer: Layer,
    pub size: Nm,
    pub thickness: Nm,
    pub angle: f64,
    pub mirrored: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub net: String,
    pub layer: Layer,
    pub width: Nm,
    pub start: Pt,
    pub end: Pt,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Via {
    pub net: String,
    pub at: Pt,
    pub diameter: Nm,
    pub drill: Nm,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Zone {
    pub net: String,
    pub layer: Layer,
    pub outline: Vec<Pt>,
    pub priority: u32,
    pub clearance: Nm,
    pub min_thickness: Nm,
}

impl Board {
    pub fn footprint(&self, reference: &str) -> Option<&Footprint> {
        self.footprints.iter().find(|fp| fp.reference == reference)
    }

    /// Every net the design names, declared or used.
    pub fn all_nets(&self) -> BTreeSet<&str> {
        let mut nets: BTreeSet<&str> = self.nets.iter().map(String::as_str).collect();
        nets.extend(self.tracks.iter().map(|t| t.net.as_str()));
        nets.extend(self.vias.iter().map(|v| v.net.as_str()));
        nets.extend(self.zones.iter().map(|z| z.net.as_str()));
        for fp in &self.footprints {
            match &fp.source {
                Source::Library(lib) => nets.extend(lib.pins.iter().map(|(_, net)| net.as_str())),
                Source::Generated(generated) => nets.extend(generated.pads.iter().filter_map(|p| p.net.as_deref())),
            }
        }
        nets
    }
}
