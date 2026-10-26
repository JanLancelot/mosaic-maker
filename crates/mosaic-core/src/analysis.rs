use std::io::Cursor;
use std::path::Path;

use image::imageops::{self, FilterType};
use image::{DynamicImage, ImageError, ImageReader, Rgb, RgbImage};

use crate::cache::AnalysisCache;
use crate::color_signature::ColorSignature;
use crate::mosaic::{RENDERED_TILE_SIZE, SourceImage, SourceImageId};
use crate::warning::SkipReason;

/// What the build needs to know about a Source Image's pixels.
pub(crate) struct Analysis {
    pub(crate) color_signature: ColorSignature,
    /// The center-cropped Source Image scaled to `RENDERED_TILE_SIZE`.
    pub(crate) rendered_source_image: RgbImage,
}

/// A Source Image together with its analysis.
pub(crate) struct AnalysedSourceImage {
    pub(crate) source_image: SourceImage,
    pub(crate) analysis: Analysis,
    /// Whether `analysis` was read from the [`AnalysisCache`] rather than computed.
    pub(crate) from_cache: bool,
}

/// Reads the file at `relative_path` in `source_library` and analyses it, or reuses its
/// analysis from `cache`, or says why it can't be a Source Image.
pub(crate) fn analyse_file(
    source_library: &Path,
    relative_path: &Path,
    cache: &AnalysisCache,
) -> Result<AnalysedSourceImage, SkipReason> {
    let bytes = std::fs::read(source_library.join(relative_path))
        .map_err(|error| SkipReason::Unreadable(error.to_string()))?;
    let id = SourceImageId::of_contents(&bytes);
    let source_image = SourceImage::new(id, relative_path.to_owned());
    if let Some(analysis) = cache.load(id) {
        return Ok(AnalysedSourceImage {
            source_image,
            analysis,
            from_cache: true,
        });
    }
    let analysis = analyse(&decode(&bytes)?);
    cache.store(id, &analysis);
    Ok(AnalysedSourceImage {
        source_image,
        analysis,
        from_cache: false,
    })
}

/// Decodes a JPEG, PNG or WebP file, with any transparency shown on white (ADR 0002).
fn decode(bytes: &[u8]) -> Result<RgbImage, SkipReason> {
    if is_heic(bytes) {
        return Err(SkipReason::Heic);
    }
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| SkipReason::Unreadable(error.to_string()))?;
    let Some(format) = reader.format() else {
        return Err(SkipReason::NotAnImage);
    };
    match reader.decode() {
        Ok(image) => Ok(on_white(image)),
        Err(ImageError::Unsupported(_)) => {
            Err(SkipReason::UnsupportedFormat(format!("{format:?}")))
        }
        Err(error) => Err(SkipReason::Corrupt(error.to_string())),
    }
}

/// `image` with its transparent areas filled with white. Opaque pixels, and every pixel of
/// an image without transparency, stay exactly as they are. Pixels are taken as sRGB.
pub(crate) fn on_white(image: DynamicImage) -> RgbImage {
    if !image.color().has_alpha() {
        return image.into_rgb8();
    }
    let image = image.into_rgba8();
    RgbImage::from_fn(image.width(), image.height(), |x, y| {
        let [r, g, b, alpha] = image.get_pixel(x, y).0;
        let alpha = u32::from(alpha);
        Rgb([r, g, b]
            .map(|channel| ((u32::from(channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8))
    })
}

/// Whether `bytes` open like a HEIC (or HEIF) file: an ISO media `ftyp` box naming a HEIF
/// brand.
fn is_heic(bytes: &[u8]) -> bool {
    const BRANDS: [&[u8; 4]; 8] = [
        b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx", b"mif1", b"msf1",
    ];
    bytes.len() >= 12 && &bytes[4..8] == b"ftyp" && BRANDS.iter().any(|b| &bytes[8..12] == *b)
}

fn analyse(image: &RgbImage) -> Analysis {
    let (width, height) = image.dimensions();
    let side = width.min(height);
    let (left, top) = ((width - side) / 2, (height - side) / 2);
    let cropped = imageops::crop_imm(image, left, top, side, side).to_image();
    Analysis {
        color_signature: ColorSignature::of_square(&cropped, 0.0, 0.0, f64::from(side)),
        rendered_source_image: imageops::resize(
            &cropped,
            RENDERED_TILE_SIZE,
            RENDERED_TILE_SIZE,
            FilterType::Lanczos3,
        ),
    }
}
