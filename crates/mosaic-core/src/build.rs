use std::collections::HashMap;
use std::path::{Path, PathBuf};

use image::imageops::{self, FilterType};
use image::{ImageReader, RgbImage};

use crate::cancel::CancelToken;
use crate::color_signature::ColorSignature;
use crate::mosaic::{Mosaic, RENDERED_TILE_SIZE, SourceImage, Tile};

/// The choices that shape a Mosaic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildSettings {
    /// The number of Tile columns. The row count follows from the Target Image's shape.
    pub tiles_across: u32,
}

/// The part of a build that is currently running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    AnalysingSourceImages,
    MatchingTiles,
    Rendering,
}

/// How far a build has got: `done` of `total` items in `stage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub stage: Stage,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("could not read the Target Image: {0}")]
    TargetImage(image::ImageError),
    #[error("could not read the Source Library: {0}")]
    SourceLibrary(std::io::Error),
    #[error("the Source Library contains no readable Source Images")]
    EmptySourceLibrary,
    #[error("Tiles Across must be between 1 and the Target Image's width ({width} px)")]
    TilesAcrossOutOfRange { width: u32 },
    #[error("the Target Image is too short for a single row of {tiles_across} Tiles Across")]
    TargetImageTooShort { tiles_across: u32 },
    #[error("the build was cancelled")]
    Cancelled,
}

/// A Source Image together with what the build needs to know about it.
struct AnalysedSourceImage {
    source_image: SourceImage,
    color_signature: ColorSignature,
    /// The center-cropped Source Image scaled to `RENDERED_TILE_SIZE`.
    rendered_source_image: RgbImage,
}

/// The Tiles laid over the Target Image.
struct Grid {
    tiles_across: u32,
    rows: u32,
    /// The side of one Tile, in Target Image pixels.
    tile_side: f64,
}

impl Grid {
    /// Rows follow from the Target Image's aspect ratio so Tiles are square; any edge
    /// that doesn't fill a whole row of Tiles is left out.
    fn over(target: &RgbImage, tiles_across: u32) -> Result<Self, BuildError> {
        let (width, height) = target.dimensions();
        if tiles_across == 0 || tiles_across > width {
            return Err(BuildError::TilesAcrossOutOfRange { width });
        }
        let rows = (u64::from(height) * u64::from(tiles_across) / u64::from(width)) as u32;
        if rows == 0 {
            return Err(BuildError::TargetImageTooShort { tiles_across });
        }
        Ok(Grid {
            tiles_across,
            rows,
            tile_side: f64::from(width) / f64::from(tiles_across),
        })
    }

    fn tile_count(&self) -> usize {
        (self.tiles_across * self.rows) as usize
    }

    /// Every Tile, in row-major order.
    fn tiles(&self) -> impl Iterator<Item = Tile> {
        let tiles_across = self.tiles_across;
        (0..self.rows)
            .flat_map(move |row| (0..tiles_across).map(move |column| Tile { column, row }))
    }
}

/// Passes progress on to the caller and stops the build once it is cancelled.
struct Reporter<'a, F> {
    on_progress: F,
    cancel: &'a CancelToken,
}

impl<F: FnMut(Progress)> Reporter<'_, F> {
    fn report(&mut self, stage: Stage, done: usize, total: usize) -> Result<(), BuildError> {
        (self.on_progress)(Progress { stage, done, total });
        if self.cancel.is_cancelled() {
            Err(BuildError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Builds a Mosaic recreating `target_image` out of the Source Images found at the top
/// level of `source_library`.
///
/// Rows follow from the Target Image's aspect ratio so Tiles are square; any edge that
/// doesn't fill a whole row of Tiles is cropped. `on_progress` is called as each stage
/// starts and as work completes. Cancelling `cancel` stops the build with
/// [`BuildError::Cancelled`].
pub fn build(
    target_image: &Path,
    source_library: &Path,
    settings: &BuildSettings,
    on_progress: impl FnMut(Progress),
    cancel: &CancelToken,
) -> Result<Mosaic, BuildError> {
    let mut reporter = Reporter {
        on_progress,
        cancel,
    };
    let target = open_image(target_image).map_err(BuildError::TargetImage)?;
    let grid = Grid::over(&target, settings.tiles_across)?;

    let analysed = analyse_source_library(source_library, &mut reporter)?;
    if analysed.is_empty() {
        return Err(BuildError::EmptySourceLibrary);
    }

    let tile_count = grid.tile_count();
    reporter.report(Stage::MatchingTiles, 0, tile_count)?;
    let mut assignment = Vec::with_capacity(tile_count);
    for (index, tile) in grid.tiles().enumerate() {
        let tile_color_signature = ColorSignature::of_square(
            &target,
            f64::from(tile.column) * grid.tile_side,
            f64::from(tile.row) * grid.tile_side,
            grid.tile_side,
        );
        assignment.push(best_match(&analysed, &tile_color_signature));
        reporter.report(Stage::MatchingTiles, index + 1, tile_count)?;
    }

    reporter.report(Stage::Rendering, 0, 1)?;
    let rendered = render(&grid, &assignment, &analysed);
    reporter.report(Stage::Rendering, 1, 1)?;

    // Keep only the Source Images that fill at least one Tile.
    let mut used: HashMap<usize, usize> = HashMap::new();
    let mut source_images = Vec::new();
    let tile_source_images = assignment
        .iter()
        .map(|&analysed_index| {
            *used.entry(analysed_index).or_insert_with(|| {
                source_images.push(analysed[analysed_index].source_image.clone());
                source_images.len() - 1
            })
        })
        .collect();

    Ok(Mosaic::new(
        grid.tiles_across,
        grid.rows,
        source_images,
        tile_source_images,
        rendered,
    ))
}

fn open_image(path: &Path) -> Result<RgbImage, image::ImageError> {
    Ok(ImageReader::open(path)?
        .with_guessed_format()?
        .decode()?
        .into_rgb8())
}

/// Analyses every decodable image at the top level of `source_library`, in file name
/// order so builds are repeatable. Files that can't be decoded are skipped.
fn analyse_source_library(
    source_library: &Path,
    reporter: &mut Reporter<'_, impl FnMut(Progress)>,
) -> Result<Vec<AnalysedSourceImage>, BuildError> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(source_library)
        .map_err(BuildError::SourceLibrary)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()
        .map_err(BuildError::SourceLibrary)?;
    paths.retain(|path| path.is_file());
    paths.sort();

    let total = paths.len();
    reporter.report(Stage::AnalysingSourceImages, 0, total)?;
    let mut analysed = Vec::with_capacity(total);
    for (index, path) in paths.into_iter().enumerate() {
        if let Ok(image) = open_image(&path) {
            analysed.push(analyse(SourceImage::new(path), &image));
        }
        reporter.report(Stage::AnalysingSourceImages, index + 1, total)?;
    }
    Ok(analysed)
}

fn analyse(source_image: SourceImage, image: &RgbImage) -> AnalysedSourceImage {
    let (width, height) = image.dimensions();
    let side = width.min(height);
    let (left, top) = ((width - side) / 2, (height - side) / 2);
    let cropped = imageops::crop_imm(image, left, top, side, side).to_image();
    AnalysedSourceImage {
        source_image,
        color_signature: ColorSignature::of_square(&cropped, 0.0, 0.0, f64::from(side)),
        rendered_source_image: imageops::resize(
            &cropped,
            RENDERED_TILE_SIZE,
            RENDERED_TILE_SIZE,
            FilterType::Lanczos3,
        ),
    }
}

/// The index of the Source Image that looks most like `tile_color_signature`. Ties go to the
/// first in file name order.
fn best_match(analysed: &[AnalysedSourceImage], tile_color_signature: &ColorSignature) -> usize {
    analysed
        .iter()
        .map(|candidate| candidate.color_signature.distance(tile_color_signature))
        .enumerate()
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(index, _)| index)
        .expect("the Source Library is not empty")
}

fn render(grid: &Grid, assignment: &[usize], analysed: &[AnalysedSourceImage]) -> RgbImage {
    let mut rendered = RgbImage::new(
        grid.tiles_across * RENDERED_TILE_SIZE,
        grid.rows * RENDERED_TILE_SIZE,
    );
    for (tile, &analysed_index) in grid.tiles().zip(assignment) {
        imageops::replace(
            &mut rendered,
            &analysed[analysed_index].rendered_source_image,
            i64::from(tile.column * RENDERED_TILE_SIZE),
            i64::from(tile.row * RENDERED_TILE_SIZE),
        );
    }
    rendered
}
