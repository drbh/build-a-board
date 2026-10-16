//! Raster artwork named by a design: what it is and where it comes from.
//! Tracing it onto silkscreen is `bb`'s job; the resolver only checks it
//! exists and the preview only shows it.

use std::path::{Path, PathBuf};

/// Which part of the artwork becomes silkscreen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Light linework on transparency prints as silk.
    Light,
    /// Dark linework prints; its light details become negative space.
    Dark,
}

/// A PNG as the preview embeds it, with its pixel size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtworkImage {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Where a design's artwork images come from: `bb` looks beside the board
/// program, a browser in the files it was handed.
pub trait ArtworkSource {
    /// The file an artwork item's `image` (as written in the design) names,
    /// or `None` when there is no such image.
    fn locate(&self, image: &str) -> Option<PathBuf>;

    /// A located image's PNG, for the preview.
    fn image(&self, path: &Path) -> Option<ArtworkImage>;
}
