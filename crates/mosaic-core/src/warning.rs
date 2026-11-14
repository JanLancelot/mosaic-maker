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

    /// What was wrong with the Credits Files the build read.
    pub fn credits_file_problems(&self) -> impl Iterator<Item = &BuildWarning> {
        self.warnings.iter().filter(|warning| {
            matches!(
                warning,
                BuildWarning::UnreadableCreditsFile { .. }
                    | BuildWarning::UnknownCreditsPaths { .. }
                    | BuildWarning::DuplicateCreditsPath { .. }
            )
        })
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
    /// `credits_file` couldn't be read or parsed, so the build went on without any of its
    /// Credits.
    UnreadableCreditsFile {
        credits_file: CreditsFileOf,
        reason: CreditsFileError,
    },
    /// Rows of the Source Library's Credits File whose paths, as written there, name no
    /// file in the Source Library. They are in the order of the file's rows.
    UnknownCreditsPaths { paths: Vec<String> },
    /// `credits_file` lists `path`, as written there, in more than one row. The first of
    /// them gives its Credit.
    DuplicateCreditsPath {
        credits_file: CreditsFileOf,
        path: String,
    },
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
            BuildWarning::UnreadableCreditsFile {
                credits_file,
                reason,
            } => write!(f, "ignored {credits_file}: {reason}"),
            BuildWarning::UnknownCreditsPaths { paths } => write!(
                f,
                "{} names files that aren't in the Source Library: {}",
                CreditsFileOf::SourceLibrary,
                paths.join(", ")
            ),
            BuildWarning::DuplicateCreditsPath { credits_file, path } => write!(
                f,
                "{credits_file} lists {path} more than once; its first row is used"
            ),
        }
    }
}

/// Which Credits File a warning is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreditsFileOf {
    /// The one at the root of the Source Library, crediting its Source Images.
    SourceLibrary,
    /// The one beside the Target Image, crediting it.
    TargetImage,
}

impl fmt::Display for CreditsFileOf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CreditsFileOf::SourceLibrary => f.write_str("the Source Library's Credits File"),
            CreditsFileOf::TargetImage => f.write_str("the Target Image's Credits File"),
        }
    }
}

/// Why a Credits File couldn't be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CreditsFileError {
    #[error("it couldn't be read ({0})")]
    Unreadable(String),
    #[error("it isn't valid CSV ({0})")]
    Unparseable(String),
    #[error("its header row has no `path` column")]
    NoPathColumn,
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
