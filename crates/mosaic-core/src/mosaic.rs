use std::path::{Path, PathBuf};

use image::RgbImage;

/// Side length, in pixels, of one Tile in a Mosaic's rendered image.
pub const RENDERED_TILE_SIZE: u32 = 32;

/// One square grid position in a Mosaic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tile {
    pub column: u32,
    pub row: u32,
}

/// One picture from the Source Library that fills Tiles in a Mosaic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceImage {
    path: PathBuf,
}

impl SourceImage {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Where the Source Image's file was found.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// A grid of Tiles that recreates a Target Image, each Tile filled by one Source Image.
#[derive(Debug, Clone)]
pub struct Mosaic {
    tiles_across: u32,
    rows: u32,
    source_images: Vec<SourceImage>,
    /// Row-major: the index into `source_images` filling each Tile.
    tile_source_images: Vec<usize>,
    rendered: RgbImage,
}

impl Mosaic {
    pub(crate) fn new(
        tiles_across: u32,
        rows: u32,
        source_images: Vec<SourceImage>,
        tile_source_images: Vec<usize>,
        rendered: RgbImage,
    ) -> Self {
        debug_assert_eq!(tile_source_images.len(), (tiles_across * rows) as usize);
        Self {
            tiles_across,
            rows,
            source_images,
            tile_source_images,
            rendered,
        }
    }

    pub fn tiles_across(&self) -> u32 {
        self.tiles_across
    }

    pub fn rows(&self) -> u32 {
        self.rows
    }

    /// The Source Image filling `tile`, or `None` if the Tile is outside the grid.
    pub fn source_image_at(&self, tile: Tile) -> Option<&SourceImage> {
        if tile.column >= self.tiles_across || tile.row >= self.rows {
            return None;
        }
        let index = (tile.row * self.tiles_across + tile.column) as usize;
        Some(&self.source_images[self.tile_source_images[index]])
    }

    /// The Mosaic drawn at `RENDERED_TILE_SIZE` pixels per Tile.
    pub fn rendered(&self) -> &RgbImage {
        &self.rendered
    }
}
