use std::path::{Path, PathBuf};

use crate::build::{BuildWarning, SkipReason};

/// Every file under `source_library`, at any depth, relative to it and in path order so
/// builds are repeatable. Hidden files and folders (named with a leading `.`) are left
/// out. A subfolder that can't be read is skipped with a warning; only an unreadable
/// `source_library` itself is an error.
pub(crate) fn files_in(
    source_library: &Path,
    warnings: &mut Vec<BuildWarning>,
) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut folders = vec![PathBuf::new()];
    while let Some(folder) = folders.pop() {
        match read_folder(source_library, &folder, &mut files, &mut folders) {
            Ok(()) => {}
            Err(error) if folder.as_os_str().is_empty() => return Err(error),
            Err(error) => warnings.push(BuildWarning::SkippedFile {
                path: folder,
                reason: SkipReason::Unreadable(error.to_string()),
            }),
        }
    }
    files.sort();
    Ok(files)
}

fn read_folder(
    source_library: &Path,
    folder: &Path,
    files: &mut Vec<PathBuf>,
    folders: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(source_library.join(folder))? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let relative_path = folder.join(entry.file_name());
        // Symbolic links to folders aren't followed, so a link cycle can't trap the scan.
        if entry.file_type()?.is_dir() {
            folders.push(relative_path);
        } else if source_library.join(&relative_path).is_file() {
            files.push(relative_path);
        }
    }
    Ok(())
}
