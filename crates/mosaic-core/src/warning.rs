use std::fmt;
use std::path::PathBuf;

use crate::mosaic::Tile;

/// What a build says beyond the Mosaic itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildReport {
    pub warnings: Vec<BuildWarning>,
    pub stats: BuildStats,
}

impl BuildReport {
    /// The files and folders in the Source Library that hold no usable Source Image.
    pub fn skipped_files(&self) -> impl Iterator<Item = &BuildWarning> {
        self.warnings
            .iter()
            .filter(|warning| matches!(warning, BuildWarning::SkippedFile { .. }))
    }

    /// How many Tiles were filled with a smaller Repeat Radius than asked for.
    pub fn relaxed_tile_count(&self) -> usize {
        self.warnings
            .iter()
            .filter(|warning| matches!(warning, BuildWarning::RepeatRadiusRelaxed { .. }))
            .count()
    }
}

/// Something the build worked around rather than failing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildWarning {
    /// A file or folder in the Source Library that holds no usable Source Image. `path` is
    /// relative to the Source Library.
    SkippedFile { path: PathBuf, reason: SkipReason },
    /// Every Source Image already filled a Tile within the Repeat Radius of `tile`, so for
    /// this Tile alone the radius shrank to `repeat_radius`.
    RepeatRadiusRelaxed { tile: Tile, repeat_radius: u32 },
}

impl fmt::Display for BuildWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildWarning::SkippedFile { path, reason } => {
                write!(f, "skipped {}: {reason}", path.display())
            }
            BuildWarning::RepeatRadiusRelaxed {
                tile,
                repeat_radius,
            } => write!(
                f,
                "Repeat Radius relaxed to {repeat_radius} for the Tile at column {}, row {}",
                tile.column, tile.row
            ),
        }
    }
}

/// Why a file in the Source Library was not used as a Source Image.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SkipReason {
    #[error("HEIC isn't supported")]
    Heic,
    #[error("not an image")]
    NotAnImage,
    #[error("{0} isn't supported")]
    UnsupportedFormat(String),
    #[error("the image is corrupt ({0})")]
    Corrupt(String),
    #[error("it couldn't be read ({0})")]
    Unreadable(String),
    /// Following links to folders could loop forever, so they are left alone.
    #[error("it links to a folder, and linked folders aren't followed")]
    LinkedFolder,
}

/// Counts describing how a build went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BuildStats {
    /// Distinct Source Images found in the Source Library.
    pub source_images: usize,
    /// Source Images whose analysis was read from the [`AnalysisCache`](crate::AnalysisCache).
    pub cache_hits: usize,
}
