//! The Credits Files a build reads: the one at the root of the Source Library, crediting its
//! Source Images, and the one beside the Target Image, crediting it. When the Target Image sits
//! at the root of the Source Library, they are the same file. Any other is ignored.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::credit::{CREDITS_FILE_NAME, Credit, Credits};
use crate::mosaic::SourceImageId;
use crate::warning::{BuildWarning, CreditsFileOf};

/// What a build's Credits Files say.
pub(crate) struct CreditsFiles {
    /// The Credits in the Source Library's Credits File, by path.
    source_library: Credits,
    target_image_credit: Option<Credit>,
}

impl CreditsFiles {
    /// Reads the Credits Files of a build of `target_image` from `source_library`, and takes
    /// them out of `files`, the files the Source Library holds, so they aren't considered as
    /// Source Images. Any other Credits File in the Source Library is taken out too, and
    /// warned about as ignored. Their problems are warnings: one that can't be read or parsed, rows of
    /// the Source Library's that name none of `files` (except the Target Image's own when the
    /// two share the file), and paths listed more than once (only the Target Image's own in a
    /// Credits File that credits only it, since its other rows are ignored).
    pub(crate) fn read(
        target_image: &Path,
        source_library: &Path,
        files: &mut Vec<PathBuf>,
        warnings: &mut Vec<BuildWarning>,
    ) -> Self {
        let target_image_name = target_image.file_name().map(Path::new);
        let target_image_folder = folder_within(folder_of(target_image), source_library);
        let shares_credits_file = target_image_folder.as_deref() == Some(Path::new(""));
        let has_source_library_file = take(files, Path::new(CREDITS_FILE_NAME));
        // In a subfolder of the Source Library, the Target Image's Credits File is in it.
        if let Some(folder) = target_image_folder {
            take(files, &folder.join(CREDITS_FILE_NAME));
        }
        // Any other Credits File is ignored, rather than skipped as a picture.
        files.retain(|file| {
            let is_credits_file = file.file_name() == Some(CREDITS_FILE_NAME.as_ref());
            if is_credits_file {
                warnings.push(BuildWarning::IgnoredCreditsFile { path: file.clone() });
            }
            !is_credits_file
        });

        let source_library_credits = if has_source_library_file {
            let credits = Credits::read(
                &source_library.join(CREDITS_FILE_NAME),
                CreditsFileOf::SourceLibrary,
                warnings,
            );
            warn_repeated(&credits, CreditsFileOf::SourceLibrary, |_| true, warnings);
            let mut paths = credits.paths_naming_none_of(files);
            paths.retain(|path| !shares_credits_file || Some(Path::new(path)) != target_image_name);
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
                warnings,
            );
            let is_target_image = |path: &str| Some(Path::new(path)) == target_image_name;
            warn_repeated(
                &credits,
                CreditsFileOf::TargetImage,
                is_target_image,
                warnings,
            );
            Some(credits)
        };
        let target_image_credit = target_image_name
            .and_then(|name| {
                target_image_credits
                    .as_ref()
                    .unwrap_or(&source_library_credits)
                    .of(name)
            })
            .cloned();

        Self {
            source_library: source_library_credits,
            target_image_credit,
        }
    }

    /// The Target Image's Credit, if its Credits File gives one.
    pub(crate) fn target_image_credit(&self) -> Option<&Credit> {
        self.target_image_credit.as_ref()
    }

    /// The Credit of each Source Image that has one, given every path in the Source Library
    /// holding a Source Image, in path order, with its identity. A Source Image stored at
    /// several paths takes the Credit of the first of them that has one.
    pub(crate) fn source_image_credits(
        &self,
        identified_paths: &[(PathBuf, SourceImageId)],
    ) -> HashMap<SourceImageId, Credit> {
        let mut credits = HashMap::new();
        for (path, id) in identified_paths {
            if let Some(credit) = self.source_library.of(path) {
                credits.entry(*id).or_insert_with(|| credit.clone());
            }
        }
        credits
    }
}

/// Removes `path` from `files`, telling whether it was there.
fn take(files: &mut Vec<PathBuf>, path: &Path) -> bool {
    let before = files.len();
    files.retain(|file| file != path);
    files.len() != before
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
