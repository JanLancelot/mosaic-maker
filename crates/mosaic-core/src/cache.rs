use std::io::Write;
use std::path::{Path, PathBuf};

use image::RgbImage;

use crate::analysis::Analysis;
use crate::color_signature::ColorSignature;
use crate::mosaic::{RENDERED_TILE_SIZE, SourceImageId};

/// Opens every entry; bump the version whenever the layout or the analysis changes, so
/// entries from older builds are recomputed rather than misread.
const MAGIC: &[u8; 8] = b"MMCACHE1";

/// Where per-Source Image analysis is kept between builds, keyed by Source Image identity.
/// One cache serves every Source Library, and it is never written inside one.
#[derive(Debug, Clone)]
pub struct AnalysisCache {
    folder: PathBuf,
    /// False once the cache has turned out to lie inside the Source Library being built.
    writable: bool,
}

impl AnalysisCache {
    /// A cache kept in `folder`.
    pub fn in_folder(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: folder.into(),
            writable: true,
        }
    }

    /// A cache in the platform's user cache folder (`~/Library/Caches/mosaic-maker/` on
    /// macOS), or in the temporary folder where the platform has none.
    pub fn in_user_cache_folder() -> Self {
        let parent = dirs::cache_dir().unwrap_or_else(std::env::temp_dir);
        Self::in_folder(parent.join("mosaic-maker"))
    }

    /// This cache as a build of `source_library` may use it: read-only if its folder lies
    /// inside `source_library`, since nothing is ever written inside a Source Library.
    pub(crate) fn kept_out_of(&self, source_library: &Path) -> Self {
        let inside = match (resolved(&self.folder), source_library.canonicalize()) {
            (Ok(folder), Ok(source_library)) => folder.starts_with(source_library),
            // If either can't be resolved, play safe.
            _ => true,
        };
        Self {
            folder: self.folder.clone(),
            writable: self.writable && !inside,
        }
    }

    /// The cached analysis of the Source Image `id`, or `None` if there is none or it can't
    /// be read back.
    pub(crate) fn load(&self, id: SourceImageId) -> Option<Analysis> {
        let bytes = std::fs::read(self.entry_path(id)).ok()?;
        let rest = bytes.strip_prefix(MAGIC)?;
        let rest = rest.strip_prefix(id.as_bytes())?;
        let (color_signature, pixels) = rest.split_at_checked(ColorSignature::BYTE_LENGTH)?;
        Some(Analysis {
            color_signature: ColorSignature::from_bytes(color_signature)?,
            rendered_source_image: RgbImage::from_raw(
                RENDERED_TILE_SIZE,
                RENDERED_TILE_SIZE,
                pixels.to_vec(),
            )
            .filter(|image| image.as_raw().len() == pixels.len())?,
        })
    }

    /// Keeps `analysis` for the Source Image `id`. The cache only saves work, so a failure
    /// to write is ignored; the entry is recomputed next time.
    pub(crate) fn store(&self, id: SourceImageId, analysis: &Analysis) {
        let _ = self.try_store(id, analysis);
    }

    fn try_store(&self, id: SourceImageId, analysis: &Analysis) -> std::io::Result<()> {
        if !self.writable {
            return Ok(());
        }
        let path = self.entry_path(id);
        let folder = path.parent().expect("entries sit in a folder");
        std::fs::create_dir_all(folder)?;
        // Written whole and then moved into place, so a concurrent build never reads half.
        let mut file = tempfile::NamedTempFile::new_in(folder)?;
        file.write_all(MAGIC)?;
        file.write_all(id.as_bytes())?;
        file.write_all(&analysis.color_signature.to_bytes())?;
        file.write_all(analysis.rendered_source_image.as_raw())?;
        file.persist(path)?;
        Ok(())
    }

    fn entry_path(&self, id: SourceImageId) -> PathBuf {
        self.folder.join("analysis").join(id.to_string())
    }
}

/// `path` made absolute with symbolic links resolved, even if its last parts don't exist yet.
fn resolved(path: &Path) -> std::io::Result<PathBuf> {
    let path = std::path::absolute(path)?;
    let mut missing = Vec::new();
    for ancestor in path.ancestors() {
        if let Ok(existing) = ancestor.canonicalize() {
            return Ok(missing
                .into_iter()
                .rev()
                .fold(existing, |path, part| path.join(part)));
        }
        missing.extend(ancestor.file_name());
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "no part of the path exists",
    ))
}
