//! What a build reads from its Source Library before analysing any picture: the files that may
//! hold Source Images, and the Credits Files' Credits.
//!
//! Two Credits Files are read: the one at the root of the Source Library, crediting its Source
//! Images, and the one beside the Target Image, crediting it. When the Target Image sits at the
//! root of the Source Library, they are the same file. Any other is ignored.

mod scan;

use std::path::{Path, PathBuf};

use crate::credit::{self, CREDITS_FILE_NAME, Credit, Credits};
use crate::warning::{BuildWarning, CreditsFileOf};

/// What a build reads from its Source Library.
pub(crate) struct SourceLibraryContents {
    /// Every file that may hold a Source Image, relative to the Source Library, in path order.
    pub(crate) files: Vec<PathBuf>,
    pub(crate) credits: LibraryCredits,
    /// What was wrong with the Source Library's folders and Credits Files.
    pub(crate) warnings: Vec<BuildWarning>,
}

/// The Credits a build's Credits Files give.
pub(crate) struct LibraryCredits {
    /// The Credits in the Source Library's Credits File, by path.
    source_images: Credits,
    target_image: Option<Credit>,
}

impl LibraryCredits {
    /// The Credit of a Source Image found at `paths`, relative to the Source Library and in
    /// path order: that of the first of them its Credits File credits.
    pub(crate) fn of_source_image(&self, paths: &[PathBuf]) -> Option<&Credit> {
        paths.iter().find_map(|path| self.source_images.of(path))
    }

    /// The Target Image's Credit, if its Credits File gives one.
    pub(crate) fn of_target_image(&self) -> Option<&Credit> {
        self.target_image.as_ref()
    }
}

/// Reads `source_library` for a build of `target_image`. Only an unreadable `source_library`
/// itself is an error; everything else wrong with it is a warning: a subfolder that can't be
/// read, a linked folder, a Credits File that is ignored or can't be read or parsed, rows of the
/// Source Library's naming no file in it (except the Target Image's own when the two share the
/// file), and paths listed more than once (only the Target Image's own in a Credits File that
/// credits only it, since its other rows are ignored).
pub(crate) fn read(
    source_library: &Path,
    target_image: &Path,
) -> std::io::Result<SourceLibraryContents> {
    let mut warnings = Vec::new();
    let scanned = scan::files_in(source_library, &mut warnings)?;

    let target_image_name = target_image.file_name().map(Path::new);
    let target_image_folder = folder_within(folder_of(target_image), source_library);
    let shares_credits_file = target_image_folder.as_deref() == Some(Path::new(""));

    let mut files = Vec::with_capacity(scanned.len());
    let mut has_source_library_file = false;
    for file in scanned {
        if file.file_name() != Some(CREDITS_FILE_NAME.as_ref()) {
            files.push(file);
            continue;
        }
        // The Target Image's Credits File is read from beside it, wherever that is.
        let folder = file.parent().unwrap_or(Path::new(""));
        if folder == Path::new("") {
            has_source_library_file = true;
        } else if Some(folder) != target_image_folder.as_deref() {
            warnings.push(BuildWarning::IgnoredCreditsFile { path: file });
        }
    }

    let is_target_image =
        |path: &str| target_image_name.is_some_and(|name| credit::names(path, name));
    let source_images = if has_source_library_file {
        let credits = Credits::read(
            &source_library.join(CREDITS_FILE_NAME),
            CreditsFileOf::SourceLibrary,
            &mut warnings,
        );
        warn_repeated(
            &credits,
            CreditsFileOf::SourceLibrary,
            |_| true,
            &mut warnings,
        );
        let mut paths = credits.paths_naming_none_of(&files);
        paths.retain(|path| !(shares_credits_file && is_target_image(path)));
        if !paths.is_empty() {
            warnings.push(BuildWarning::UnknownCreditsPaths { paths });
        }
        credits
    } else {
        Credits::default()
    };

    let target_image_credits = if shares_credits_file {
        None
    } else {
        let credits = Credits::read(
            &folder_of(target_image).join(CREDITS_FILE_NAME),
            CreditsFileOf::TargetImage,
            &mut warnings,
        );
        warn_repeated(
            &credits,
            CreditsFileOf::TargetImage,
            is_target_image,
            &mut warnings,
        );
        Some(credits)
    };
    let target_image = target_image_name
        .and_then(|name| {
            target_image_credits
                .as_ref()
                .unwrap_or(&source_images)
                .of(name)
        })
        .cloned();

    Ok(SourceLibraryContents {
        files,
        credits: LibraryCredits {
            source_images,
            target_image,
        },
        warnings,
    })
}

/// Warns that `credits_file` lists more than once each path it does that `matters`.
fn warn_repeated(
    credits: &Credits,
    credits_file: CreditsFileOf,
    matters: impl Fn(&str) -> bool,
    warnings: &mut Vec<BuildWarning>,
) {
    warnings.extend(
        credits
            .repeated_paths()
            .iter()
            .filter(|path| matters(path))
            .map(|path| BuildWarning::DuplicateCreditsPath {
                credits_file,
                path: path.clone(),
            }),
    );
}

/// The folder holding the file at `path`.
fn folder_of(path: &Path) -> &Path {
    match path.parent() {
        Some(folder) if !folder.as_os_str().is_empty() => folder,
        _ => Path::new("."),
    }
}

/// Where `folder` is within `source_library`, however each is written: empty for its root, and
/// `None` if it is outside it.
fn folder_within(folder: &Path, source_library: &Path) -> Option<PathBuf> {
    let folder = std::fs::canonicalize(folder).ok()?;
    let source_library = std::fs::canonicalize(source_library).ok()?;
    folder
        .strip_prefix(source_library)
        .ok()
        .map(Path::to_path_buf)
}
