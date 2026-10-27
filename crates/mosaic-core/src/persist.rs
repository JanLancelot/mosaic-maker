//! Saving a Mosaic to a self-contained `.mosaic` file and opening it again, without needing
//! its Source Library.
//!
//! A `.mosaic` file is a zip holding `manifest.json` (see [`Manifest`]), the rendered Mosaic as
//! `mosaic.png`, and the Thumbnail of every Source Image used as `thumbnails/<identity>.png`.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder, ImageFormat, ImageReader, RgbImage};
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::build::BuildSettings;
use crate::mosaic::{Mosaic, RENDERED_TILE_SIZE, SourceImage, SourceImageId, THUMBNAIL_SIZE};

/// Bump whenever the manifest's layout or meaning changes. Files of any other version are
/// refused rather than misread.
const FORMAT_VERSION: u32 = 1;

const MANIFEST: &str = "manifest.json";
const RENDERED: &str = "mosaic.png";

fn thumbnail_entry(id: SourceImageId) -> String {
    format!("thumbnails/{id}.png")
}

/// Everything about a Mosaic except its pictures.
#[derive(Serialize, Deserialize)]
struct Manifest {
    format_version: u32,
    settings: Settings,
    grid: Grid,
    /// The Source Library's absolute path when the Mosaic was saved.
    source_library: PathBuf,
    source_images: Vec<SourceImageEntry>,
    /// Row-major: the index into `source_images` filling each Tile.
    tiles: Vec<usize>,
}

/// Read first, so a file of another version is refused before its manifest is misread.
#[derive(Deserialize)]
struct Version {
    format_version: u32,
}

#[derive(Serialize, Deserialize)]
struct Settings {
    /// The Target Image's absolute path.
    target_image: PathBuf,
    tiles_across: u32,
    repeat_radius: u32,
    seed: u64,
}

#[derive(Serialize, Deserialize)]
struct Grid {
    columns: u32,
    rows: u32,
}

#[derive(Serialize, Deserialize)]
struct SourceImageEntry {
    /// The Source Image's identity, as 64 hex digits.
    id: String,
    /// For people reading the manifest; opening takes the name from `last_known_path`.
    file_name: String,
    /// Relative to `source_library`.
    last_known_path: PathBuf,
}

/// Why a Mosaic couldn't be saved.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct SaveError(#[from] io::Error);

/// Why a `.mosaic` file couldn't be opened.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("the file couldn't be read ({0})")]
    Unreadable(io::Error),
    #[error("the file isn't a Mosaic, or it is damaged ({0})")]
    Corrupt(String),
    #[error("the file uses format version {0}, which this version of Mosaic Maker can't open")]
    UnsupportedVersion(u32),
}

fn corrupt(error: impl ToString) -> OpenError {
    OpenError::Corrupt(error.to_string())
}

impl Mosaic {
    /// Writes the Mosaic to the `.mosaic` file at `path`, replacing any file there. The file
    /// holds everything needed to open and inspect the Mosaic again, even once its Source
    /// Library is gone. An existing file is left as it was if saving fails.
    pub fn save(&self, path: &Path) -> Result<(), SaveError> {
        write_replacing(path, |writer| self.write_to(writer))?;
        Ok(())
    }

    fn write_to(&self, writer: impl Write + Seek) -> io::Result<()> {
        let mut zip = ZipWriter::new(writer);
        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        // PNGs are compressed already.
        let stored = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .large_file(true);

        zip.start_file(MANIFEST, deflated)?;
        serde_json::to_writer_pretty(&mut zip, &self.manifest())?;
        zip.start_file(RENDERED, stored)?;
        write_png(&mut zip, self.rendered())?;
        for source_image in self.source_images() {
            zip.start_file(thumbnail_entry(source_image.id()), stored)?;
            write_png(&mut zip, source_image.thumbnail())?;
        }
        zip.finish()?.flush()
    }

    fn manifest(&self) -> Manifest {
        let BuildSettings {
            tiles_across,
            repeat_radius,
            seed,
        } = *self.settings();
        Manifest {
            format_version: FORMAT_VERSION,
            settings: Settings {
                target_image: self.target_image().to_owned(),
                tiles_across,
                repeat_radius,
                seed,
            },
            grid: Grid {
                columns: tiles_across,
                rows: self.rows(),
            },
            source_library: self.source_library().to_owned(),
            source_images: self
                .source_images()
                .iter()
                .map(|source_image| SourceImageEntry {
                    id: source_image.id().to_string(),
                    file_name: source_image.file_name().into_owned(),
                    last_known_path: source_image.last_known_path().to_owned(),
                })
                .collect(),
            tiles: self.tile_source_images().to_vec(),
        }
    }

    /// Opens the `.mosaic` file at `path`. Its Source Library isn't needed: whether each
    /// Original is still reachable is up to [`Mosaic::original`].
    pub fn open(path: &Path) -> Result<Mosaic, OpenError> {
        let file = File::open(path).map_err(OpenError::Unreadable)?;
        let mut zip = ZipArchive::new(BufReader::new(file)).map_err(corrupt)?;

        let manifest = read_entry(&mut zip, MANIFEST)?;
        let Version { format_version } = serde_json::from_slice(&manifest).map_err(corrupt)?;
        if format_version != FORMAT_VERSION {
            return Err(OpenError::UnsupportedVersion(format_version));
        }
        let Manifest {
            format_version: _,
            settings,
            grid,
            source_library,
            source_images,
            tiles,
        } = serde_json::from_slice(&manifest).map_err(corrupt)?;

        if grid.columns != settings.tiles_across || grid.columns == 0 || grid.rows == 0 {
            return Err(corrupt("the grid doesn't match Tiles Across"));
        }
        let tile_count = u64::from(grid.columns) * u64::from(grid.rows);
        if tiles.len() as u64 != tile_count {
            return Err(corrupt(format!(
                "{} Tiles listed for a {} × {} grid",
                tiles.len(),
                grid.columns,
                grid.rows
            )));
        }
        if tiles.iter().any(|&index| index >= source_images.len()) {
            return Err(corrupt("a Tile is filled by an unlisted Source Image"));
        }

        let rendered = read_png(&mut zip, RENDERED)?;
        let rendered_size = |tiles: u32| tiles.checked_mul(RENDERED_TILE_SIZE);
        if Some(rendered.dimensions()) != rendered_size(grid.columns).zip(rendered_size(grid.rows))
        {
            return Err(corrupt("the rendered Mosaic doesn't fit the grid"));
        }
        let source_images = source_images
            .into_iter()
            .map(|entry| {
                let id = SourceImageId::from_hex(&entry.id).ok_or_else(|| {
                    corrupt(format!("{} isn't a Source Image identity", entry.id))
                })?;
                let thumbnail = read_png(&mut zip, &thumbnail_entry(id))?;
                if thumbnail.dimensions() != (THUMBNAIL_SIZE, THUMBNAIL_SIZE) {
                    return Err(corrupt(format!("the Thumbnail of {id} is the wrong size")));
                }
                Ok(SourceImage::new(id, entry.last_known_path, thumbnail))
            })
            .collect::<Result<Vec<_>, OpenError>>()?;

        Ok(Mosaic::new(
            settings.target_image,
            source_library,
            BuildSettings {
                tiles_across: settings.tiles_across,
                repeat_radius: settings.repeat_radius,
                seed: settings.seed,
            },
            grid.rows,
            source_images,
            tiles,
            rendered,
        ))
    }
}

/// Writes the file at `path` with `write`, replacing any file there. The file is written whole
/// and then moved into place, so an existing file is left as it was if writing fails.
pub(crate) fn write_replacing(
    path: &Path,
    write: impl FnOnce(&mut BufWriter<&mut File>) -> io::Result<()>,
) -> io::Result<()> {
    let folder = match path.parent() {
        Some(folder) if !folder.as_os_str().is_empty() => folder,
        _ => Path::new("."),
    };
    let mut file = tempfile::NamedTempFile::new_in(folder)?;
    let mut writer = BufWriter::new(file.as_file_mut());
    write(&mut writer)?;
    writer.flush()?;
    drop(writer);
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

pub(crate) fn write_png(writer: impl Write, image: &RgbImage) -> io::Result<()> {
    PngEncoder::new(writer)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(io::Error::other)
}

fn read_entry(zip: &mut ZipArchive<impl Read + Seek>, name: &str) -> Result<Vec<u8>, OpenError> {
    let mut entry = zip
        .by_name(name)
        .map_err(|error| corrupt(format!("{name}: {error}")))?;
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|error| corrupt(format!("{name}: {error}")))?;
    Ok(bytes)
}

fn read_png(zip: &mut ZipArchive<impl Read + Seek>, name: &str) -> Result<RgbImage, OpenError> {
    let bytes = read_entry(zip, name)?;
    let image = ImageReader::with_format(io::Cursor::new(bytes), ImageFormat::Png)
        .decode()
        .map_err(|error| corrupt(format!("{name}: {error}")))?;
    Ok(image.into_rgb8())
}
