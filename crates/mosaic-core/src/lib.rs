//! Builds Mosaics: recreates a Target Image out of Source Images from a Source Library.
//!
//! This crate holds all domain logic and has no GUI dependencies.

mod analysis;
mod assign;
mod build;
mod cache;
mod cancel;
mod color_signature;
mod mosaic;
mod scan;
mod warning;

pub use build::{BuildError, BuildSettings, Built, DEFAULT_REPEAT_RADIUS, Progress, Stage, build};
pub use cache::AnalysisCache;
pub use cancel::CancelToken;
pub use mosaic::{Mosaic, RENDERED_TILE_SIZE, SourceImage, SourceImageId, Tile};
pub use warning::{BuildReport, BuildStats, BuildWarning, SkipReason};
