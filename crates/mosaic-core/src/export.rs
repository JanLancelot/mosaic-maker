use std::io;
use std::path::Path;

use image::RgbImage;
use image::imageops;
use rayon::prelude::*;

use crate::analysis;
use crate::mosaic::{Mosaic, Tile};
use crate::persist;

/// The most pixels an exported PNG may have: a gigapixel, about 3 GB while it is drawn.
pub const MAX_EXPORT_PIXELS: u64 = 1 << 30;

/// What an export says beyond the PNG itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportReport {
    /// The Tiles drawn from their Source Image's Thumbnail because its Original is
    /// unreachable, row by row.
    pub tiles_from_thumbnails: Vec<Tile>,
}

/// A Source Image as its Tiles are drawn in an export.
enum Picture {
    Original(RgbImage),
    /// Its Thumbnail, since its Original is unreachable.
    Thumbnail(RgbImage),
}

/// Why a Mosaic couldn't be exported.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("the size per Tile must be at least 1 px")]
    NoTileSize,
    #[error("at {tile_size} px per Tile the PNG would be over a gigapixel")]
    TooLarge { tile_size: u32 },
    #[error("the PNG couldn't be written ({0})")]
    Write(#[from] io::Error),
}

impl Mosaic {
    /// Writes the Mosaic to a PNG at `path` with `tile_size` pixels per Tile, replacing any
    /// file there. Each Tile is drawn from its Source Image's Original where that is
    /// reachable, and from its Thumbnail where it isn't.
    pub fn export_png(&self, path: &Path, tile_size: u32) -> Result<ExportReport, ExportError> {
        if tile_size == 0 {
            return Err(ExportError::NoTileSize);
        }
        let side = |tiles: u32| tiles.checked_mul(tile_size);
        let (Some(width), Some(height)) = (side(self.tiles_across()), side(self.rows())) else {
            return Err(ExportError::TooLarge { tile_size });
        };
        if u64::from(width) * u64::from(height) > MAX_EXPORT_PIXELS {
            return Err(ExportError::TooLarge { tile_size });
        }

        let pictures: Vec<Picture> = self
            .source_images()
            .par_iter()
            .map(|source_image| {
                match analysis::original_as_tile(
                    &self.path_of(source_image),
                    source_image.id(),
                    tile_size,
                ) {
                    Some(original) => Picture::Original(original),
                    None => {
                        Picture::Thumbnail(analysis::scaled_to(source_image.thumbnail(), tile_size))
                    }
                }
            })
            .collect();

        let mut exported = RgbImage::new(width, height);
        let mut tiles_from_thumbnails = Vec::new();
        for (tile, index) in self.tiles_with_source_images() {
            let picture = match &pictures[index] {
                Picture::Original(picture) => picture,
                Picture::Thumbnail(picture) => {
                    tiles_from_thumbnails.push(tile);
                    picture
                }
            };
            imageops::replace(
                &mut exported,
                picture,
                i64::from(tile.column * tile_size),
                i64::from(tile.row * tile_size),
            );
        }

        persist::write_replacing(path, |writer| persist::write_png(writer, &exported))?;
        Ok(ExportReport {
            tiles_from_thumbnails,
        })
    }
}
