//! Builds Mosaics: recreates a Target Image out of Source Images from a Source Library.
//!
//! This crate holds all domain logic and has no GUI dependencies.

mod analysis;
mod assign;
mod build;
mod cache;
mod cancel;
mod color_signature;
mod export;
mod mosaic;
mod persist;
mod scan;
mod viewport;
mod warning;

pub use build::{BuildError, BuildSettings, Built, DEFAULT_REPEAT_RADIUS, Progress, Stage, build};
pub use cache::AnalysisCache;
pub use cancel::CancelToken;
pub use export::{ExportError, ExportReport, MAX_EXPORT_PIXELS};
pub use mosaic::{Mosaic, RENDERED_TILE_SIZE, SourceImage, SourceImageId, THUMBNAIL_SIZE, Tile};
pub use persist::{OpenError, SaveError};
pub use viewport::{ScreenPoint, ScreenRect, Viewport};
pub use warning::{BuildReport, BuildStats, BuildWarning, SkipReason};
