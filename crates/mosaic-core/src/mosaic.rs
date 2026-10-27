use std::borrow::Cow;
use std::fmt;
use std::path::{Path, PathBuf};

use image::RgbImage;

use crate::analysis;
use crate::build::BuildSettings;

/// Side length, in pixels, of one Tile in a Mosaic's rendered image.
pub const RENDERED_TILE_SIZE: u32 = 32;

/// Side length, in pixels, of a Source Image's Thumbnail.
pub const THUMBNAIL_SIZE: u32 = 128;

/// One square grid position in a Mosaic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tile {
    pub column: u32,
    pub row: u32,
}

/// What identifies a Source Image: a hash of its file's contents (ADR 0003). Byte-identical
/// files share one; any change to the bytes gives a new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceImageId([u8; 32]);

impl SourceImageId {
    pub(crate) fn of_contents(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The identity written as by [`Display`](fmt::Display): 64 lowercase hex digits.
    pub(crate) fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let mut bytes = [0; 32];
        for (byte, pair) in bytes.iter_mut().zip(hex.as_bytes().chunks(2)) {
            *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
        }
        Some(Self(bytes))
    }
}

impl fmt::Display for SourceImageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

/// One picture from the Source Library that fills Tiles in a Mosaic.
#[derive(Clone, PartialEq, Eq)]
pub struct SourceImage {
    id: SourceImageId,
    last_known_path: PathBuf,
    thumbnail: RgbImage,
}

impl fmt::Debug for SourceImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The Thumbnail's pixels would bury everything else.
        f.debug_struct("SourceImage")
            .field("id", &self.id)
            .field("last_known_path", &self.last_known_path)
            .finish_non_exhaustive()
    }
}

impl SourceImage {
    pub(crate) fn new(id: SourceImageId, last_known_path: PathBuf, thumbnail: RgbImage) -> Self {
        Self {
            id,
            last_known_path,
            thumbnail,
        }
    }

    pub fn id(&self) -> SourceImageId {
        self.id
    }

    /// Where the Source Image's file was last found, relative to the Source Library.
    pub fn last_known_path(&self) -> &Path {
        &self.last_known_path
    }

    /// The Source Image center-cropped and scaled to `THUMBNAIL_SIZE` pixels square.
    pub fn thumbnail(&self) -> &RgbImage {
        &self.thumbnail
    }

    /// The name of the Source Image's file, for showing to the user.
    pub fn file_name(&self) -> Cow<'_, str> {
        self.last_known_path
            .file_name()
            .unwrap_or(self.last_known_path.as_os_str())
            .to_string_lossy()
    }
}

/// A grid of Tiles that recreates a Target Image, each Tile filled by one Source Image.
#[derive(Debug, Clone)]
pub struct Mosaic {
    /// The Target Image the Mosaic recreates, as an absolute path.
    target_image: PathBuf,
    /// The Source Library the Mosaic was built from, as an absolute path.
    source_library: PathBuf,
    settings: BuildSettings,
    rows: u32,
    source_images: Vec<SourceImage>,
    /// Row-major: the index into `source_images` filling each Tile.
    tile_source_images: Vec<usize>,
    rendered: RgbImage,
}

impl Mosaic {
    pub(crate) fn new(
        target_image: PathBuf,
        source_library: PathBuf,
        settings: BuildSettings,
        rows: u32,
        source_images: Vec<SourceImage>,
        tile_source_images: Vec<usize>,
        rendered: RgbImage,
    ) -> Self {
        debug_assert_eq!(
            tile_source_images.len(),
            (settings.tiles_across * rows) as usize
        );
        Self {
            target_image,
            source_library,
            settings,
            rows,
            source_images,
            tile_source_images,
            rendered,
        }
    }

    /// The Target Image the Mosaic recreates, as an absolute path.
    pub fn target_image(&self) -> &Path {
        &self.target_image
    }

    /// The Source Library the Mosaic was built from, as an absolute path.
    pub fn source_library(&self) -> &Path {
        &self.source_library
    }

    /// The settings the Mosaic was built with.
    pub fn settings(&self) -> &BuildSettings {
        &self.settings
    }

    pub fn tiles_across(&self) -> u32 {
        self.settings.tiles_across
    }

    pub fn rows(&self) -> u32 {
        self.rows
    }

    /// The Source Image filling `tile`, or `None` if the Tile is outside the grid.
    pub fn source_image_at(&self, tile: Tile) -> Option<&SourceImage> {
        if tile.column >= self.tiles_across() || tile.row >= self.rows {
            return None;
        }
        let index = (tile.row * self.tiles_across() + tile.column) as usize;
        Some(&self.source_images[self.tile_source_images[index]])
    }

    /// Where the file of `source_image` was last found: its last-known path within the
    /// Source Library the Mosaic was built from.
    pub fn path_of(&self, source_image: &SourceImage) -> PathBuf {
        self.source_library.join(&source_image.last_known_path)
    }

    /// The Original of `source_image`, shown on white and scaled down to fit within
    /// `max_side` pixels square, or `None` if the Original is unreachable.
    pub fn original(&self, source_image: &SourceImage, max_side: u32) -> Option<RgbImage> {
        analysis::original(&self.path_of(source_image), source_image.id, max_side)
    }

    /// Every Tile that the Source Image `id` fills, row by row. None if it isn't used.
    pub fn tiles_filled_by(&self, id: SourceImageId) -> Vec<Tile> {
        let Some(wanted) = self
            .source_images
            .iter()
            .position(|source_image| source_image.id == id)
        else {
            return Vec::new();
        };
        self.tiles_with_source_images()
            .filter(|&(_, index)| index == wanted)
            .map(|(tile, _)| tile)
            .collect()
    }

    /// The Mosaic drawn at `RENDERED_TILE_SIZE` pixels per Tile.
    pub fn rendered(&self) -> &RgbImage {
        &self.rendered
    }

    /// Every Source Image filling at least one Tile, each once.
    pub(crate) fn source_images(&self) -> &[SourceImage] {
        &self.source_images
    }

    /// Row-major: the index into [`Self::source_images`] filling each Tile.
    pub(crate) fn tile_source_images(&self) -> &[usize] {
        &self.tile_source_images
    }

    /// Every Tile, row by row, with the index into [`Self::source_images`] filling it.
    pub(crate) fn tiles_with_source_images(&self) -> impl Iterator<Item = (Tile, usize)> {
        let tiles_across = self.tiles_across() as usize;
        self.tile_source_images
            .iter()
            .enumerate()
            .map(move |(i, &index)| {
                let tile = Tile {
                    column: (i % tiles_across) as u32,
                    row: (i / tiles_across) as u32,
                };
                (tile, index)
            })
    }
}
