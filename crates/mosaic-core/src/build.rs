use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use image::imageops::{self, FilterType};
use image::{ImageReader, RgbImage};
use rayon::prelude::*;

use crate::analysis::{self, AnalysedSourceImage};
use crate::assign::{self, Assignment};
use crate::cache::AnalysisCache;
use crate::cancel::CancelToken;
use crate::color_signature::ColorSignature;
use crate::mosaic::{Mosaic, RENDERED_TILE_SIZE, SourceImage, Tile};
use crate::scan;
use crate::warning::{BuildReport, BuildStats, BuildWarning};

/// The Repeat Radius a new build starts with.
pub const DEFAULT_REPEAT_RADIUS: u32 = 3;

/// The choices that shape a Mosaic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildSettings {
    /// The number of Tile columns. The row count follows from the Target Image's shape.
    pub tiles_across: u32,
    /// Within this many Tiles (Chebyshev distance) the same Source Image should not fill
    /// two Tiles. 0 allows neighbours to repeat.
    pub repeat_radius: u32,
    /// Decides the order Tiles are filled in; the same inputs and seed give the same Mosaic.
    pub seed: u64,
}

/// A finished build: the Mosaic and what happened along the way.
#[derive(Debug)]
pub struct Built {
    pub mosaic: Mosaic,
    pub report: BuildReport,
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
    #[error("{} changed or disappeared during the build", .path.display())]
    SourceImageChanged { path: PathBuf },
    #[error("the build was cancelled")]
    Cancelled,
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

/// Builds a Mosaic recreating `target_image` out of the Source Images found anywhere in
/// `source_library`, reusing and adding to the analysis kept in `cache`.
///
/// Rows follow from the Target Image's aspect ratio so Tiles are square; any edge that
/// doesn't fill a whole row of Tiles is cropped. `on_progress` is called as each stage
/// starts and as work completes. Cancelling `cancel` stops the build with
/// [`BuildError::Cancelled`].
pub fn build(
    target_image: &Path,
    source_library: &Path,
    settings: &BuildSettings,
    cache: &AnalysisCache,
    on_progress: impl FnMut(Progress),
    cancel: &CancelToken,
) -> Result<Built, BuildError> {
    let mut reporter = Reporter {
        on_progress,
        cancel,
    };
    let target = open_image(target_image).map_err(BuildError::TargetImage)?;
    let grid = Grid::over(&target, settings.tiles_across)?;

    let cache = &cache.kept_out_of(source_library);
    let mut warnings = Vec::new();
    let analysed = analyse_source_library(source_library, cache, &mut warnings, &mut reporter)?;
    if analysed.is_empty() {
        return Err(BuildError::EmptySourceLibrary);
    }

    let tile_count = grid.tile_count();
    reporter.report(Stage::MatchingTiles, 0, tile_count)?;
    let tile_color_signatures: Vec<ColorSignature> = grid
        .tiles()
        .map(|tile| {
            ColorSignature::of_square(
                &target,
                f64::from(tile.column) * grid.tile_side,
                f64::from(tile.row) * grid.tile_side,
                grid.tile_side,
            )
        })
        .collect();
    let source_image_color_signatures: Vec<ColorSignature> = analysed
        .iter()
        .map(|analysed| analysed.analysis.color_signature.clone())
        .collect();
    let Assignment {
        tile_source_images: assignment,
        warnings: assignment_warnings,
    } = assign::assign(
        grid.tiles_across,
        &tile_color_signatures,
        &source_image_color_signatures,
        settings.repeat_radius,
        settings.seed,
        |done| reporter.report(Stage::MatchingTiles, done, tile_count),
    )?;
    warnings.extend(assignment_warnings);

    // Keep only the Source Images that fill at least one Tile, in the order they first do.
    let mut used_indices: HashMap<usize, usize> = HashMap::new();
    let mut used: Vec<&AnalysedSourceImage> = Vec::new();
    let tile_source_images: Vec<usize> = assignment
        .iter()
        .map(|&analysed_index| {
            *used_indices.entry(analysed_index).or_insert_with(|| {
                used.push(&analysed[analysed_index]);
                used.len() - 1
            })
        })
        .collect();

    let thumbnails_and_rendered =
        in_parallel(&used, Stage::Rendering, &mut reporter, |analysed| {
            let thumbnail = analysis::thumbnail(
                source_library,
                &analysed.last_known_path,
                analysed.id,
                cache,
            )
            .ok_or_else(|| BuildError::SourceImageChanged {
                path: analysed.last_known_path.clone(),
            })?;
            let rendered = imageops::resize(
                &thumbnail,
                RENDERED_TILE_SIZE,
                RENDERED_TILE_SIZE,
                FilterType::Lanczos3,
            );
            Ok((thumbnail, rendered))
        })?
        .into_iter()
        .collect::<Result<Vec<_>, BuildError>>()?;
    let rendered = render(
        &grid,
        &tile_source_images,
        thumbnails_and_rendered.iter().map(|(_, rendered)| rendered),
    );
    let source_images = used
        .iter()
        .zip(thumbnails_and_rendered)
        .map(|(analysed, (thumbnail, _))| {
            SourceImage::new(analysed.id, analysed.last_known_path.clone(), thumbnail)
        })
        .collect();

    let mosaic = Mosaic::new(
        std::path::absolute(source_library).unwrap_or_else(|_| source_library.to_owned()),
        grid.tiles_across,
        grid.rows,
        source_images,
        tile_source_images,
        rendered,
    );
    Ok(Built {
        mosaic,
        report: BuildReport {
            warnings,
            stats: BuildStats {
                source_images: analysed.len(),
                cache_hits: analysed
                    .iter()
                    .filter(|analysed| analysed.from_cache)
                    .count(),
            },
        },
    })
}

/// Opens the Target Image, with any transparency shown on white like a Source Image's.
fn open_image(path: &Path) -> Result<RgbImage, image::ImageError> {
    Ok(analysis::on_white(
        ImageReader::open(path)?.with_guessed_format()?.decode()?,
    ))
}

/// Analyses every decodable image in `source_library` and its subfolders, reusing what
/// `cache` holds. Source Images come in path order, so builds are repeatable however the
/// parallel work interleaves. Byte-identical files are one Source Image, last seen at the
/// first path. Files that can't be Source Images are skipped with a warning.
fn analyse_source_library(
    source_library: &Path,
    cache: &AnalysisCache,
    warnings: &mut Vec<BuildWarning>,
    reporter: &mut Reporter<'_, impl FnMut(Progress)>,
) -> Result<Vec<AnalysedSourceImage>, BuildError> {
    let files = scan::files_in(source_library, warnings).map_err(BuildError::SourceLibrary)?;

    let total = files.len();
    let outcomes = in_parallel(
        &files,
        Stage::AnalysingSourceImages,
        reporter,
        |relative_path| analysis::analyse_file(source_library, relative_path, cache),
    )?;

    let mut analysed: Vec<AnalysedSourceImage> = Vec::with_capacity(total);
    let mut seen = HashSet::new();
    for (relative_path, outcome) in files.into_iter().zip(outcomes) {
        match outcome {
            Ok(source_image) => {
                if seen.insert(source_image.id) {
                    analysed.push(source_image);
                }
            }
            Err(reason) => warnings.push(BuildWarning::SkippedFile {
                path: relative_path,
                reason,
            }),
        }
    }
    Ok(analysed)
}

/// Runs `work` on every one of `items` across all cores, returning the results in the same
/// order. Progress through `stage` is reported from the calling thread as each item completes.
fn in_parallel<T: Sync, R: Send>(
    items: &[T],
    stage: Stage,
    reporter: &mut Reporter<'_, impl FnMut(Progress)>,
    work: impl Fn(&T) -> R + Sync,
) -> Result<Vec<R>, BuildError> {
    reporter.report(stage, 0, items.len())?;
    let (cancel, work) = (reporter.cancel, &work);
    let (sender, completions) = mpsc::channel();
    std::thread::scope(|scope| {
        let workers = scope.spawn(move || {
            items
                .par_iter()
                .map_with(sender, |sender, item| {
                    if cancel.is_cancelled() {
                        return None;
                    }
                    let result = work(item);
                    // The receiver only goes away once the build is cancelled.
                    let _ = sender.send(());
                    Some(result)
                })
                .collect::<Option<Vec<_>>>()
        });
        // Ends once every worker is done and has dropped its sender.
        let mut reported = Ok(());
        for (done, ()) in completions.iter().enumerate() {
            reported = reporter.report(stage, done + 1, items.len());
            if reported.is_err() {
                break;
            }
        }
        drop(completions);
        let results = workers
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
        reported?;
        results.ok_or(BuildError::Cancelled)
    })
}

/// Draws each Tile as `rendered_source_images[tile_source_images[tile]]`.
fn render<'a>(
    grid: &Grid,
    tile_source_images: &[usize],
    rendered_source_images: impl Iterator<Item = &'a RgbImage>,
) -> RgbImage {
    let rendered_source_images: Vec<&RgbImage> = rendered_source_images.collect();
    let mut rendered = RgbImage::new(
        grid.tiles_across * RENDERED_TILE_SIZE,
        grid.rows * RENDERED_TILE_SIZE,
    );
    for (tile, &index) in grid.tiles().zip(tile_source_images) {
        imageops::replace(
            &mut rendered,
            rendered_source_images[index],
            i64::from(tile.column * RENDERED_TILE_SIZE),
            i64::from(tile.row * RENDERED_TILE_SIZE),
        );
    }
    rendered
}
