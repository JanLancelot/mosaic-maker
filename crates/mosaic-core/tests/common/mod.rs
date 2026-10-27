//! Fixtures shared by the test files: tiny synthetic images in a temporary folder.
// Each test file uses only some of these.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use image::{Rgb, RgbImage};
use mosaic_core::{
    AnalysisCache, BuildError, BuildSettings, Built, CancelToken, DEFAULT_REPEAT_RADIUS, Mosaic,
    Progress, Tile, build,
};
use tempfile::TempDir;

pub const RED: Rgb<u8> = Rgb([255, 0, 0]);
pub const GREEN: Rgb<u8> = Rgb([0, 255, 0]);
pub const BLUE: Rgb<u8> = Rgb([0, 0, 255]);

/// The colors of a striped Mosaic's Tiles, row by row.
pub const STRIPES: [[Rgb<u8>; 3]; 2] = [[RED, BLUE, RED], [GREEN, RED, BLUE]];

pub struct Fixture {
    dir: TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("library")).unwrap();
        Fixture { dir }
    }

    pub fn cache(&self) -> AnalysisCache {
        AnalysisCache::in_folder(self.cache_folder())
    }

    /// The folder `cache` keeps its entries in.
    pub fn cache_folder(&self) -> PathBuf {
        self.dir.path().join("cache")
    }

    pub fn source_library(&self) -> PathBuf {
        self.dir.path().join("library")
    }

    /// Saves `image` at `relative_path` in the Source Library, creating folders as needed.
    pub fn add_source_image(&self, relative_path: &str, image: &RgbImage) {
        let path = self.source_library().join(relative_path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        image.save(path).unwrap();
    }

    /// A path in the fixture's folder, outside the Source Library, for a file a test writes.
    pub fn path(&self, file_name: &str) -> PathBuf {
        self.dir.path().join(file_name)
    }

    pub fn add_file(&self, relative_path: &str, contents: &[u8]) {
        let path = self.source_library().join(relative_path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    pub fn set_target_image(&self, image: &RgbImage) -> PathBuf {
        let path = self.dir.path().join("target.png");
        image.save(&path).unwrap();
        path
    }

    pub fn build(&self, target_image: &Path, tiles_across: u32) -> Mosaic {
        self.build_reporting(target_image, tiles_across).mosaic
    }

    pub fn build_reporting(&self, target_image: &Path, tiles_across: u32) -> Built {
        self.build_with(target_image, tiles_across, |_| {}, &CancelToken::new())
            .unwrap()
    }

    pub fn build_with_settings(&self, target_image: &Path, settings: &BuildSettings) -> Built {
        build(
            target_image,
            &self.source_library(),
            settings,
            &self.cache(),
            |_| {},
            &CancelToken::new(),
        )
        .unwrap()
    }

    pub fn build_with(
        &self,
        target_image: &Path,
        tiles_across: u32,
        on_progress: impl FnMut(Progress),
        cancel: &CancelToken,
    ) -> Result<Built, BuildError> {
        let settings = BuildSettings {
            tiles_across,
            repeat_radius: DEFAULT_REPEAT_RADIUS,
            seed: 0,
        };
        build(
            target_image,
            &self.source_library(),
            &settings,
            &self.cache(),
            on_progress,
            cancel,
        )
    }
}

pub fn solid(width: u32, height: u32, color: Rgb<u8>) -> RgbImage {
    RgbImage::from_pixel(width, height, color)
}

pub fn tile(column: u32, row: u32) -> Tile {
    Tile { column, row }
}

/// A 3 × 2 Mosaic, Repeat Radius 0, whose Tiles are colored as in `STRIPES`, filled from
/// `red.png`, `colors/blue.png` and `colors/green.png`.
pub fn striped_mosaic(fixture: &Fixture) -> Mosaic {
    let settings = BuildSettings {
        tiles_across: 3,
        repeat_radius: 0,
        seed: 0,
    };
    striped_mosaic_with(fixture, &settings)
}

/// Like `striped_mosaic`, built with `settings`, which must keep 3 Tiles Across and Repeat
/// Radius 0 for the Tiles to match `STRIPES`.
pub fn striped_mosaic_with(fixture: &Fixture, settings: &BuildSettings) -> Mosaic {
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    fixture.add_source_image("colors/blue.png", &solid(8, 8, BLUE));
    fixture.add_source_image("colors/green.png", &solid(8, 8, GREEN));
    let target = RgbImage::from_fn(30, 20, |x, y| STRIPES[(y / 10) as usize][(x / 10) as usize]);
    let target_image = fixture.set_target_image(&target);
    fixture.build_with_settings(&target_image, settings).mosaic
}

/// Every Tile of `mosaic`, row by row.
pub fn tiles(mosaic: &Mosaic) -> Vec<Tile> {
    (0..mosaic.rows())
        .flat_map(|row| (0..mosaic.tiles_across()).map(move |column| Tile { column, row }))
        .collect()
}
