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

        // Each Source Image as its Tiles show it, and whether that came from its Thumbnail.
        let pictures: Vec<(RgbImage, bool)> = self
            .source_images()
            .par_iter()
            .map(|source_image| {
                match analysis::original_as_tile(
                    &self.path_of(source_image),
                    source_image.id(),
                    tile_size,
                ) {
                    Some(original) => (original, false),
                    None => (
                        analysis::scaled_to(source_image.thumbnail(), tile_size),
                        true,
                    ),
                }
            })
            .collect();

        let mut exported = RgbImage::new(width, height);
        let mut tiles_from_thumbnails = Vec::new();
        let tiles_across = self.tiles_across() as usize;
        for (i, &index) in self.tile_source_images().iter().enumerate() {
            let tile = Tile {
                column: (i % tiles_across) as u32,
                row: (i / tiles_across) as u32,
            };
            let (picture, from_thumbnail) = &pictures[index];
            imageops::replace(
                &mut exported,
                picture,
                i64::from(tile.column * tile_size),
                i64::from(tile.row * tile_size),
            );
            if *from_thumbnail {
                tiles_from_thumbnails.push(tile);
            }
        }

        persist::write_replacing(path, |writer| persist::write_png(writer, &exported))?;
        Ok(ExportReport {
            tiles_from_thumbnails,
        })
    }
}
