//! Builds Mosaics: recreates a Target Image out of Source Images from a Source Library.
//!
//! This crate holds all domain logic and has no GUI dependencies.

mod build;
mod cancel;
mod color_signature;
mod mosaic;

pub use build::{BuildError, BuildSettings, Progress, Stage, build};
pub use cancel::CancelToken;
pub use mosaic::{Mosaic, RENDERED_TILE_SIZE, SourceImage, Tile};
