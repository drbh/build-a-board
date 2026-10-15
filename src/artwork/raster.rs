//! Single-channel 8-bit raster operations, bit-exact with OpenCV 4.8.

/// A row-major single-channel 8-bit image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gray {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

impl Gray {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height, data: vec![0; width * height] }
    }

    pub fn get(&self, x: usize, y: usize) -> u8 {
        self.data[y * self.width + x]
    }

    /// `cv::boundingRect(cv::findNonZero(img))` as `(x, y, width, height)`.
    pub fn nonzero_bounds(&self) -> Option<(usize, usize, usize, usize)> {
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        for (y, row) in self.data.chunks_exact(self.width).enumerate() {
            for (x, _) in row.iter().enumerate().filter(|(_, v)| **v != 0) {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
        (x0 != usize::MAX).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
    }

    pub fn crop(&self, x: usize, y: usize, width: usize, height: usize) -> Self {
        let data = (y..y + height).flat_map(|row| &self.data[row * self.width + x..][..width]).copied().collect();
        Self { width, height, data }
    }

    /// `cv::threshold(..., THRESH_BINARY)` with a max value of 255.
    pub fn threshold(&self, level: u8) -> Self {
        Self { data: self.data.iter().map(|&v| if v > level { 255 } else { 0 }).collect(), ..*self }
    }
}

/// `cv::cvtColor(BGR2GRAY)` for 8-bit input: fixed-point BT.601 weights.
pub fn luminance(r: u8, g: u8, b: u8) -> u8 {
    const SHIFT: u32 = 14;
    let sum = u32::from(b) * 1868 + u32::from(g) * 9617 + u32::from(r) * 4899;
    ((sum + (1 << (SHIFT - 1))) >> SHIFT) as u8
}

/// One source sample's contribution to a destination sample.
struct Weight {
    dst: usize,
    src: usize,
    alpha: f32,
}

/// OpenCV's `computeResizeAreaTab`: fractional coverage of each source
/// cell by each destination cell, relative to the destination cell size.
fn area_weights(src_size: usize, dst_size: usize, scale: f64) -> Vec<Weight> {
    let mut tab = Vec::with_capacity(src_size * 2);
    for dx in 0..dst_size {
        let fsx1 = dx as f64 * scale;
        let fsx2 = fsx1 + scale;
        let cell = scale.min(src_size as f64 - fsx1);
        let sx2 = (fsx2.floor() as isize).min(src_size as isize - 1);
        let sx1 = (fsx1.ceil() as isize).min(sx2);
        if sx1 as f64 - fsx1 > 1e-3 {
            tab.push(Weight { dst: dx, src: (sx1 - 1) as usize, alpha: ((sx1 as f64 - fsx1) / cell) as f32 });
        }
        for sx in sx1..sx2 {
            tab.push(Weight { dst: dx, src: sx as usize, alpha: (1.0 / cell) as f32 });
        }
        if fsx2 - sx2 as f64 > 1e-3 {
            let alpha = (fsx2 - sx2 as f64).min(1.0).min(cell) / cell;
            tab.push(Weight { dst: dx, src: sx2 as usize, alpha: alpha as f32 });
        }
    }
    tab
}

/// `cv::resize(..., INTER_AREA)` for a non-integer downscale
/// (`ResizeArea_Invoker<uchar, float>`), including its single-precision
/// accumulation order and round-half-to-even output.
pub fn resize_area(src: &Gray, width: usize, height: usize) -> Gray {
    assert!(width < src.width && height < src.height, "INTER_AREA path here only covers downscaling");
    let scale_x = 1.0 / (width as f64 / src.width as f64);
    let scale_y = 1.0 / (height as f64 / src.height as f64);
    let xtab = area_weights(src.width, width, scale_x);
    let ytab = area_weights(src.height, height, scale_y);

    let mut dst = Gray::new(width, height);
    let mut buf = vec![0f32; width];
    let mut sum = vec![0f32; width];
    let mut prev_dy = ytab[0].dst;
    let store = |dst: &mut Gray, dy: usize, sum: &[f32]| {
        for (out, value) in dst.data[dy * width..][..width].iter_mut().zip(sum) {
            *out = value.round_ties_even().clamp(0.0, 255.0) as u8;
        }
    };
    for row in &ytab {
        buf.fill(0.0);
        let source = &src.data[row.src * src.width..][..src.width];
        for w in &xtab {
            buf[w.dst] += f32::from(source[w.src]) * w.alpha;
        }
        if row.dst != prev_dy {
            store(&mut dst, prev_dy, &sum);
            for (s, b) in sum.iter_mut().zip(&buf) {
                *s = row.alpha * b;
            }
            prev_dy = row.dst;
        } else {
            for (s, b) in sum.iter_mut().zip(&buf) {
                *s += row.alpha * b;
            }
        }
    }
    store(&mut dst, prev_dy, &sum);
    dst
}

/// `cv::morphologyEx(MORPH_OPEN)` with a 2x2 all-ones kernel: an erosion
/// then a dilation, each taking the extreme over the pixel and its
/// up/left neighbours (anchor at the kernel's centre, `(1, 1)`). Outside
/// the image counts as foreground for erosion and background for dilation.
pub fn open_2x2(src: &Gray) -> Gray {
    fn apply(src: &Gray, outside: u8, pick: fn(u8, u8) -> u8) -> Gray {
        let mut out = Gray::new(src.width, src.height);
        let at = |x: isize, y: isize| {
            if x < 0 || y < 0 { outside } else { src.get(x as usize, y as usize) }
        };
        for y in 0..src.height as isize {
            for x in 0..src.width as isize {
                let value = pick(pick(at(x, y), at(x - 1, y)), pick(at(x, y - 1), at(x - 1, y - 1)));
                out.data[y as usize * src.width + x as usize] = value;
            }
        }
        out
    }
    let eroded = apply(src, 255, u8::min);
    apply(&eroded, 0, u8::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luminance_weights() {
        assert_eq!(luminance(255, 255, 255), 255);
        assert_eq!(luminance(0, 0, 0), 0);
        assert_eq!(luminance(255, 0, 0), 76);
        assert_eq!(luminance(0, 255, 0), 150);
        assert_eq!(luminance(0, 0, 255), 29);
    }

    #[test]
    fn area_weights_cover_each_destination_cell() {
        let tab = area_weights(5, 2, 2.5);
        for dx in 0..2 {
            let total: f32 = tab.iter().filter(|w| w.dst == dx).map(|w| w.alpha).sum();
            assert!((total - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn opening_removes_isolated_pixels_and_shifts_blocks_like_opencv() {
        let mut img = Gray::new(6, 6);
        img.data[6 + 1] = 255;
        for y in 3..5 {
            for x in 3..5 {
                img.data[y * 6 + x] = 255;
            }
        }
        // cv2.morphologyEx(MORPH_OPEN, ones((2, 2))): the even kernel's
        // anchor moves the surviving 2x2 block down and right by one pixel.
        let lit: Vec<(usize, usize)> = (0..6)
            .flat_map(|y| (0..6).map(move |x| (x, y)))
            .filter(|&(x, y)| open_2x2(&img).get(x, y) == 255)
            .collect();
        assert_eq!(lit, vec![(4, 4), (5, 4), (4, 5), (5, 5)]);
    }
}
