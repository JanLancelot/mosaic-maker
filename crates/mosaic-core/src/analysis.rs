use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::imageops::{self, FilterType};
use image::{DynamicImage, ImageError, ImageReader, Rgb, RgbImage};

use crate::cache::AnalysisCache;
use crate::color_signature::ColorSignature;
use crate::mosaic::{SourceImageId, THUMBNAIL_SIZE};
use crate::warning::SkipReason;

/// What matching needs to know about a Source Image's pixels.
pub(crate) struct Analysis {
    pub(crate) color_signature: ColorSignature,
}

/// A Source Image together with its analysis.
pub(crate) struct AnalysedSourceImage {
    pub(crate) id: SourceImageId,
    /// Where the Source Image's file was found, relative to the Source Library.
    pub(crate) last_known_path: PathBuf,
    pub(crate) analysis: Analysis,
    /// Whether `analysis` was read from the [`AnalysisCache`] rather than computed.
    pub(crate) from_cache: bool,
}

/// Reads the file at `relative_path` in `source_library` and analyses it, or reuses its
/// analysis from `cache`, or says why it can't be a Source Image. A fresh analysis also
/// leaves the Source Image's Thumbnail in `cache`, while the file is decoded anyway.
pub(crate) fn analyse_file(
    source_library: &Path,
    relative_path: &Path,
    cache: &AnalysisCache,
) -> Result<AnalysedSourceImage, SkipReason> {
    let bytes = std::fs::read(source_library.join(relative_path))
        .map_err(|error| SkipReason::Unreadable(error.to_string()))?;
    let id = SourceImageId::of_contents(&bytes);
    let last_known_path = relative_path.to_owned();
    if let Some(analysis) = cache.load(id) {
        return Ok(AnalysedSourceImage {
            id,
            last_known_path,
            analysis,
            from_cache: true,
        });
    }
    let cropped = center_square(&decode(&bytes)?);
    let side = f64::from(cropped.width());
    let analysis = Analysis {
        color_signature: ColorSignature::of_square(&cropped, 0.0, 0.0, side),
    };
    cache.store(id, &analysis);
    cache.store_thumbnail(id, &scaled_to_thumbnail(&cropped));
    Ok(AnalysedSourceImage {
        id,
        last_known_path,
        analysis,
        from_cache: false,
    })
}

/// The Thumbnail of the Source Image `id`, last found at `relative_path` in
/// `source_library`: from `cache`, or made afresh from the file and kept in `cache`. `None` if
/// the file no longer holds that Source Image.
pub(crate) fn thumbnail(
    source_library: &Path,
    relative_path: &Path,
    id: SourceImageId,
    cache: &AnalysisCache,
) -> Option<RgbImage> {
    if let Some(thumbnail) = cache.load_thumbnail(id) {
        return Some(thumbnail);
    }
    let bytes = read_if_still(&source_library.join(relative_path), id)?;
    let thumbnail = scaled_to_thumbnail(&center_square(&decode(&bytes).ok()?));
    cache.store_thumbnail(id, &thumbnail);
    Some(thumbnail)
}

/// The Original of the Source Image `id`, last found at `path`, scaled down to fit within
/// `max_side` pixels square, or `None` if it is unreachable. Unlike a Thumbnail it is not
/// cropped.
pub(crate) fn original(path: &Path, id: SourceImageId, max_side: u32) -> Option<RgbImage> {
    let image = decode(&read_if_still(path, id)?).ok()?;
    let (width, height) = image.dimensions();
    let longest = width.max(height);
    if longest <= max_side {
        return Some(image);
    }
    let fit =
        |side: u32| ((u64::from(side) * u64::from(max_side) / u64::from(longest)) as u32).max(1);
    Some(imageops::resize(
        &image,
        fit(width),
        fit(height),
        FilterType::Lanczos3,
    ))
}

/// The Original of the Source Image `id`, last found at `path`, as a Tile shows it:
/// center-cropped and scaled to `side` pixels square. `None` if it is unreachable.
pub(crate) fn original_as_tile(path: &Path, id: SourceImageId, side: u32) -> Option<RgbImage> {
    let square = center_square(&decode(&read_if_still(path, id)?).ok()?);
    Some(scaled_to(&square, side))
}

/// `square` scaled to `side` pixels square, or left as it is if it is that size already.
pub(crate) fn scaled_to(square: &RgbImage, side: u32) -> RgbImage {
    if square.dimensions() == (side, side) {
        return square.clone();
    }
    imageops::resize(square, side, side, FilterType::Lanczos3)
}

/// The contents of the file at `path`, if it can be read and still holds the Source Image
/// `id`.
fn read_if_still(path: &Path, id: SourceImageId) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    (SourceImageId::of_contents(&bytes) == id).then_some(bytes)
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

/// The largest square at the center of `image`.
fn center_square(image: &RgbImage) -> RgbImage {
    let (width, height) = image.dimensions();
    let side = width.min(height);
    let (left, top) = ((width - side) / 2, (height - side) / 2);
    imageops::crop_imm(image, left, top, side, side).to_image()
}

fn scaled_to_thumbnail(square: &RgbImage) -> RgbImage {
    scaled_to(square, THUMBNAIL_SIZE)
}
