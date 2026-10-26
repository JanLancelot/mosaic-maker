use std::sync::LazyLock;

use image::RgbImage;

/// The signature divides a square region into `GRID` × `GRID` parts.
const GRID: usize = 3;

/// A color in CIELAB (D65).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Lab {
    l: f32,
    a: f32,
    b: f32,
}

impl Lab {
    fn from_linear_rgb([r, g, b]: [f32; 3]) -> Self {
        const WHITE: [f32; 3] = [0.950_47, 1.0, 1.088_83];
        let x = 0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b;
        let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b;
        let z = 0.019_333_9 * r + 0.119_192 * g + 0.950_304_1 * b;
        let f = |t: f32| {
            const DELTA: f32 = 6.0 / 29.0;
            if t > DELTA * DELTA * DELTA {
                t.cbrt()
            } else {
                t / (3.0 * DELTA * DELTA) + 4.0 / 29.0
            }
        };
        let (fx, fy, fz) = (f(x / WHITE[0]), f(y / WHITE[1]), f(z / WHITE[2]));
        Lab {
            l: 116.0 * fy - 16.0,
            a: 500.0 * (fx - fy),
            b: 200.0 * (fy - fz),
        }
    }

    /// Perceptual distance (CIE76 ΔE).
    fn distance(self, other: Lab) -> f32 {
        let (dl, da, db) = (self.l - other.l, self.a - other.a, self.b - other.b);
        (dl * dl + da * da + db * db).sqrt()
    }
}

static SRGB_TO_LINEAR: LazyLock<[f32; 256]> = LazyLock::new(|| {
    std::array::from_fn(|i| {
        let c = i as f32 / 255.0;
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    })
});

/// The color layout of a square region: the average color of each part of a grid laid over
/// it, in CIELAB.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Signature([Lab; GRID * GRID]);

impl Signature {
    /// The signature of the square of `image` with top-left corner (`left`, `top`) and side
    /// `side`, all in pixels. A pixel belongs to whichever part contains its center.
    pub(crate) fn of_square(image: &RgbImage, left: f64, top: f64, side: f64) -> Self {
        let part = side / GRID as f64;
        Signature(std::array::from_fn(|i| {
            let (column, row) = (i % GRID, i / GRID);
            let columns = pixel_span(left + column as f64 * part, part, image.width());
            let rows = pixel_span(top + row as f64 * part, part, image.height());
            average_color(image, columns, rows)
        }))
    }

    /// How different two signatures look: the sum of the perceptual distances between
    /// their corresponding parts.
    pub(crate) fn distance(&self, other: &Signature) -> f32 {
        self.0
            .iter()
            .zip(&other.0)
            .map(|(a, b)| a.distance(*b))
            .sum()
    }
}

/// The pixel indices whose centers fall in `[start, start + length)`. When the span is
/// narrower than a pixel and contains no center, the pixel under its midpoint stands in.
fn pixel_span(start: f64, length: f64, limit: u32) -> std::ops::Range<u32> {
    let first = (start - 0.5).ceil().max(0.0) as u32;
    let end = ((start + length - 0.5).ceil().max(0.0) as u32).min(limit);
    if first < end {
        first..end
    } else {
        let middle = ((start + length / 2.0) as u32).min(limit - 1);
        middle..middle + 1
    }
}

fn average_color(
    image: &RgbImage,
    columns: std::ops::Range<u32>,
    rows: std::ops::Range<u32>,
) -> Lab {
    let mut sum = [0.0f64; 3];
    for y in rows.clone() {
        for x in columns.clone() {
            let pixel = image.get_pixel(x, y);
            for (total, channel) in sum.iter_mut().zip(pixel.0) {
                *total += f64::from(SRGB_TO_LINEAR[channel as usize]);
            }
        }
    }
    let count = (columns.len() * rows.len()) as f64;
    Lab::from_linear_rgb(sum.map(|total| (total / count) as f32))
}
