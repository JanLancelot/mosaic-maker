//! Fixtures shared by the test files: tiny synthetic images in a temporary folder.
// Each test file uses only some of these.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use image::{Rgb, RgbImage};
use mosaic_core::{
    AnalysisCache, BuildError, BuildSettings, Built, CancelToken, DEFAULT_REPEAT_RADIUS, Mosaic,
    Progress, build,
};
use tempfile::TempDir;

pub const RED: Rgb<u8> = Rgb([255, 0, 0]);
pub const GREEN: Rgb<u8> = Rgb([0, 255, 0]);
pub const BLUE: Rgb<u8> = Rgb([0, 0, 255]);

pub struct Fixture {
    pub dir: TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("library")).unwrap();
        Fixture { dir }
    }

    pub fn cache(&self) -> AnalysisCache {
        AnalysisCache::in_folder(self.dir.path().join("cache"))
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
