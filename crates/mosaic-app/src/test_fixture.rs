//! The fixture the smoke tests and the Session tests share: a striped Target Image and a
//! Source Library with one solid Source Image per stripe, in a temporary folder.

use std::path::PathBuf;

use image::{Rgb, RgbImage};
use mosaic_core::{AnalysisCache, BuildSettings};
use tempfile::TempDir;

use crate::session::Setup;

/// The Target Image is four stripes, each two Tiles wide, of these colors.
pub const STRIPES: [(&str, Rgb<u8>); 4] = [
    ("red.png", Rgb([220, 40, 40])),
    ("green.png", Rgb([40, 180, 70])),
    ("blue.png", Rgb([40, 90, 220])),
    ("yellow.png", Rgb([240, 200, 40])),
];
pub const TILES_ACROSS: u32 = 8;
pub const ROWS: u32 = 4;
const TILE_PIXELS: u32 = 10;

pub struct Fixture {
    dir: TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        let fixture = Fixture {
            dir: TempDir::new().unwrap(),
        };
        std::fs::create_dir(fixture.source_library()).unwrap();
        for (file_name, color) in STRIPES {
            RgbImage::from_pixel(64, 64, color)
                .save(fixture.source_library().join(file_name))
                .unwrap();
        }
        let stripe_pixels = TILES_ACROSS / STRIPES.len() as u32 * TILE_PIXELS;
        let target_image =
            RgbImage::from_fn(TILES_ACROSS * TILE_PIXELS, ROWS * TILE_PIXELS, |x, _| {
                STRIPES[(x / stripe_pixels) as usize].1
            });
        target_image.save(fixture.target_image()).unwrap();
        fixture
    }

    pub fn target_image(&self) -> PathBuf {
        self.dir.path().join("target.png")
    }

    pub fn source_library(&self) -> PathBuf {
        self.dir.path().join("library")
    }

    /// A path in the fixture's folder, outside the Source Library, for a file a test writes.
    pub fn path(&self, file_name: &str) -> PathBuf {
        self.dir.path().join(file_name)
    }

    pub fn cache(&self) -> AnalysisCache {
        AnalysisCache::in_folder(self.dir.path().join("cache"))
    }

    /// The Target Image and Source Library chosen, with settings that fill each stripe with
    /// its own Source Image throughout.
    pub fn setup(&self) -> Setup {
        Setup {
            target_image: Some(self.target_image()),
            source_library: Some(self.source_library()),
            settings: BuildSettings {
                tiles_across: TILES_ACROSS,
                repeat_radius: 0,
                seed: 0,
            },
        }
    }
}
