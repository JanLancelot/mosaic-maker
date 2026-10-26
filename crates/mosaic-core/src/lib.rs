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

pub use build::{
    BuildError, BuildSettings, BuildStats, BuildWarning, Built, DEFAULT_REPEAT_RADIUS, Progress,
    SkipReason, Stage, build,
};
pub use cache::AnalysisCache;
pub use cancel::CancelToken;
pub use mosaic::{Mosaic, RENDERED_TILE_SIZE, SourceImage, SourceImageId, Tile};
