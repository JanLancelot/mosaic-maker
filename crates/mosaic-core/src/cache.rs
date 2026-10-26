use std::io::Write;
use std::path::{Path, PathBuf};

use image::RgbImage;

use crate::analysis::Analysis;
use crate::color_signature::ColorSignature;
use crate::mosaic::{SourceImageId, THUMBNAIL_SIZE};

/// The kinds of entry the cache keeps for each Source Image, each in its own folder.
#[derive(Clone, Copy)]
enum Entry {
    Analysis,
    Thumbnail,
}

impl Entry {
    fn folder(self) -> &'static str {
        match self {
            Entry::Analysis => "analysis",
            Entry::Thumbnail => "thumbnails",
        }
    }

    /// Opens every entry of this kind; bump the version whenever its layout or contents
    /// change, so entries from older builds are recomputed rather than misread.
    fn magic(self) -> &'static [u8; 8] {
        match self {
            Entry::Analysis => b"MMCACHE2",
            Entry::Thumbnail => b"MMTHUMB1",
        }
    }
}

/// Where per-Source Image analysis and Thumbnails are kept between builds, keyed by Source
/// Image identity.
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
        let color_signature = self.load_entry(Entry::Analysis, id)?;
        Some(Analysis {
            color_signature: ColorSignature::from_bytes(&color_signature)?,
        })
    }

    /// Keeps `analysis` for the Source Image `id`. The cache only saves work, so a failure
    /// to write is ignored; the entry is recomputed next time.
    pub(crate) fn store(&self, id: SourceImageId, analysis: &Analysis) {
        let color_signature = analysis.color_signature.to_bytes();
        let _ = self.try_store(Entry::Analysis, id, &color_signature);
    }

    /// The cached Thumbnail of the Source Image `id`, or `None` if there is none or it can't
    /// be read back.
    pub(crate) fn load_thumbnail(&self, id: SourceImageId) -> Option<RgbImage> {
        let pixels = self.load_entry(Entry::Thumbnail, id)?;
        let length = pixels.len();
        RgbImage::from_raw(THUMBNAIL_SIZE, THUMBNAIL_SIZE, pixels)
            .filter(|thumbnail| thumbnail.as_raw().len() == length)
    }

    /// Keeps `thumbnail` for the Source Image `id`, ignoring any failure like [`Self::store`].
    pub(crate) fn store_thumbnail(&self, id: SourceImageId, thumbnail: &RgbImage) {
        let _ = self.try_store(Entry::Thumbnail, id, thumbnail.as_raw());
    }

    /// What the `entry` for `id` holds after its header, if it exists and is for `id`.
    fn load_entry(&self, entry: Entry, id: SourceImageId) -> Option<Vec<u8>> {
        let bytes = std::fs::read(self.entry_path(entry, id)).ok()?;
        let header = [entry.magic().as_slice(), id.as_bytes()].concat();
        bytes.strip_prefix(header.as_slice()).map(<[u8]>::to_vec)
    }

    fn try_store(&self, entry: Entry, id: SourceImageId, contents: &[u8]) -> std::io::Result<()> {
        if !self.writable {
            return Ok(());
        }
        let path = self.entry_path(entry, id);
        let folder = path.parent().expect("entries sit in a folder");
        std::fs::create_dir_all(folder)?;
        // Written whole and then moved into place, so a concurrent build never reads half.
        let mut file = tempfile::NamedTempFile::new_in(folder)?;
        file.write_all(entry.magic())?;
        file.write_all(id.as_bytes())?;
        file.write_all(contents)?;
        file.persist(path)?;
        Ok(())
    }

    fn entry_path(&self, entry: Entry, id: SourceImageId) -> PathBuf {
        self.folder.join(entry.folder()).join(id.to_string())
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
