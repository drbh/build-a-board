//! Raster artwork to silkscreen polygons.
//!
//! A transparent PNG is reduced to a binary mask of its visible light (or
//! dark) linework, resampled to a 0.1 mm grid, cleaned with a morphological
//! opening, traced into outer borders with holes, and simplified. Every step
//! is bit-exact with the OpenCV 4.8 pipeline the board was first styled
//! with, so regenerated silkscreen polygons are byte-identical.

mod approx;
mod contours;
mod raster;

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use bb_core::artwork::{ArtworkImage, ArtworkSource};

use contours::Point;
use raster::Gray;

pub use bb_core::artwork::Tone;

/// Silkscreen raster pitch.
pub const RASTER_MM: f64 = 0.10;
/// Mask level for `luminance * alpha`.
const VISIBLE: f32 = 96.0;
/// Re-binarisation level after resampling.
const RESAMPLED: u8 = 112;
/// Douglas–Peucker tolerance, in raster pixels.
const SIMPLIFY_PX: f64 = 0.65;
/// Smallest kept outline or hole, in square raster pixels.
const MIN_AREA_PX: f64 = 2.0;

/// Artwork images found in the board directory, as the design names them.
pub struct ArtworkDir<'a>(pub &'a Path);

impl ArtworkSource for ArtworkDir<'_> {
    fn locate(&self, image: &str) -> Option<PathBuf> {
        let path = self.0.join(image);
        path.is_file().then_some(path)
    }

    fn image(&self, path: &Path) -> Option<ArtworkImage> {
        let png = std::fs::read(path).ok()?;
        let reader = png::Decoder::new(std::io::Cursor::new(&png)).read_info().ok()?;
        let (width, height) = (reader.info().width, reader.info().height);
        Some(ArtworkImage { png, width, height })
    }
}

/// One filled silkscreen region in raster pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shape {
    pub outline: Vec<Point>,
    pub holes: Vec<Vec<Point>>,
}

/// Traced artwork on its raster grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Traced {
    pub width_px: u32,
    pub height_px: u32,
    pub shapes: Vec<Shape>,
}

impl Traced {
    /// Board millimetres for a raster point when the artwork is centred on
    /// `center` (computed in the same floating-point order as the original
    /// tooling, so conversion to nanometres is reproducible).
    pub fn to_mm(&self, center: (f64, f64), (px, py): Point) -> (f64, f64) {
        let x0 = center.0 - f64::from(self.width_px) * RASTER_MM / 2.0;
        let y0 = center.1 - f64::from(self.height_px) * RASTER_MM / 2.0;
        (x0 + f64::from(px) * RASTER_MM, y0 + f64::from(py) * RASTER_MM)
    }
}

/// Trace `png` scaled to `width_mm` wide.
pub fn trace(png: &Path, width_mm: f64, tone: Tone) -> Result<Traced> {
    let cleaned = binary_raster(png, width_mm, tone)?;
    let (width_px, height_px) = (cleaned.width as u32, cleaned.height as u32);
    let found = contours::find_contours(&cleaned);
    let simplify = |points: &[Point]| approx::approx_closed(points, SIMPLIFY_PX);
    let large = |points: &[Point]| contours::area(points) >= MIN_AREA_PX;
    let mut shapes = Vec::new();
    for contour in found.iter().filter(|c| c.parent.is_none() && large(&c.points)) {
        let outline = simplify(&contour.points);
        if outline.len() < 3 {
            continue;
        }
        let holes = std::iter::successors(contour.first_child, |&child| found[child].next)
            .map(|child| &found[child].points)
            .filter_map(|points| Some(simplify(points)).filter(|hole| hole.len() >= 3 && large(points)))
            .collect();
        shapes.push(Shape { outline, holes });
    }
    ensure!(!shapes.is_empty(), "tracing {} produced no usable polygons", png.display());
    Ok(Traced { width_px, height_px, shapes })
}

/// The cleaned binary mask on the silkscreen raster grid, cropped to the
/// artwork's visible extent.
fn binary_raster(png: &Path, width_mm: f64, tone: Tone) -> Result<Gray> {
    let mask = visible_mask(png, tone)?;
    let (x, y, w, h) =
        mask.nonzero_bounds().with_context(|| format!("no visible artwork found in {}", png.display()))?;
    let mask = mask.crop(x, y, w, h);

    let width_px = round_half_even(width_mm / RASTER_MM);
    let height_px = round_half_even(width_px as f64 * h as f64 / w as f64).max(1);
    ensure!(width_px >= 1, "artwork width {width_mm} mm is below one {RASTER_MM} mm pixel");
    ensure!(
        (width_px as usize) < w && (height_px as usize) < h,
        "{}: {width_mm} mm needs {width_px}x{height_px} px but the artwork is only {w}x{h} px",
        png.display()
    );
    let resampled = raster::resize_area(&mask, width_px as usize, height_px as usize);
    Ok(raster::open_2x2(&resampled.threshold(RESAMPLED)))
}

/// Pixels whose tone-adjusted luminance, weighted by opacity, reaches the
/// visibility level.
fn visible_mask(png: &Path, tone: Tone) -> Result<Gray> {
    let file = File::open(png).with_context(|| format!("cannot open {}", png.display()))?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info().with_context(|| format!("cannot decode {}", png.display()))?;
    let info = reader.info();
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        bail!("{}: expected 8-bit RGBA artwork, found {:?} {:?}", png.display(), info.bit_depth, info.color_type);
    }
    let mut pixels = vec![0; reader.output_buffer_size().context("artwork too large")?];
    let frame = reader.next_frame(&mut pixels)?;
    let (width, height) = (frame.width as usize, frame.height as usize);

    let mut mask = Gray::new(width, height);
    for (out, rgba) in mask.data.iter_mut().zip(pixels[..frame.buffer_size()].chunks_exact(4)) {
        let luminance = f32::from(raster::luminance(rgba[0], rgba[1], rgba[2]));
        let luminance = match tone {
            Tone::Light => luminance,
            Tone::Dark => 255.0 - luminance,
        };
        let alpha = f32::from(rgba[3]) / 255.0;
        *out = if luminance * alpha >= VISIBLE { 255 } else { 0 };
    }
    Ok(mask)
}

/// Python's `round()`: nearest integer, ties to even.
fn round_half_even(value: f64) -> i64 {
    value.round_ties_even() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_size_rounds_like_python() {
        assert_eq!(round_half_even(12.5 / RASTER_MM), 125);
        assert_eq!(round_half_even(2.5), 2);
        assert_eq!(round_half_even(3.5), 4);
    }

    #[test]
    fn to_mm_centres_the_raster() {
        let traced = Traced { width_px: 10, height_px: 4, shapes: Vec::new() };
        assert_eq!(traced.to_mm((5.0, 5.0), (0, 0)), (4.5, 4.8));
    }

    /// Compare against OpenCV output dumped by the reference script:
    /// `BB_ARTWORK_REFERENCE=<dir with reference.json and source PNGs> cargo test -- --ignored artwork`.
    #[test]
    #[ignore = "needs an OpenCV reference dump"]
    fn matches_opencv_reference() {
        let dir = std::env::var("BB_ARTWORK_REFERENCE").expect("BB_ARTWORK_REFERENCE");
        let reference: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(Path::new(&dir).join("reference.json")).unwrap()).unwrap();
        let points = |v: &serde_json::Value| -> Vec<Point> {
            v.as_array()
                .unwrap()
                .iter()
                .map(|p| (p[0].as_i64().unwrap() as i32, p[1].as_i64().unwrap() as i32))
                .collect()
        };
        for (key, expected) in reference.as_object().unwrap() {
            let (name, width) = key.split_once('-').unwrap();
            let png = Path::new(&dir).join(format!("{name}.png"));
            let traced = trace(&png, width.parse().unwrap(), Tone::Light).unwrap();
            assert_eq!(traced.width_px as i64, expected["width_px"].as_i64().unwrap(), "{key} width");
            assert_eq!(traced.height_px as i64, expected["height_px"].as_i64().unwrap(), "{key} height");
            let shapes: Vec<Shape> = expected["shapes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| Shape {
                    outline: points(&s["outline"]),
                    holes: s["holes"].as_array().unwrap().iter().map(points).collect(),
                })
                .collect();
            let found = contours::find_contours(&binary_raster(&png, width.parse().unwrap(), Tone::Light).unwrap());
            let all: Vec<Vec<Point>> = expected["contours"].as_array().unwrap().iter().map(points).collect();
            assert_eq!(found.iter().map(|c| c.points.clone()).collect::<Vec<_>>(), all, "{key} contours");
            let link = |v: &serde_json::Value| usize::try_from(v.as_i64().unwrap()).ok();
            for (contour, h) in found.iter().zip(expected["hierarchy"].as_array().unwrap()) {
                let h = h.as_array().unwrap();
                assert_eq!(
                    (contour.next, contour.prev, contour.first_child, contour.parent),
                    (link(&h[0]), link(&h[1]), link(&h[2]), link(&h[3])),
                    "{key} hierarchy"
                );
            }
            assert_eq!(traced.shapes.len(), shapes.len(), "{key} shape count");
            assert_eq!(traced.shapes, shapes, "{key} shapes");
        }
    }
}
