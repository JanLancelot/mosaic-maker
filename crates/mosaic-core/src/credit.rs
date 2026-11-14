use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::warning::{BuildWarning, CreditsFileError, CreditsFileOf};

/// The name of a Credits File.
pub(crate) const CREDITS_FILE_NAME: &str = "credits.csv";

/// Who made a picture and where it comes from. Every field is free text, kept as written,
/// and any of them may be unknown; a Credit has at least one known field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credit {
    artist: Option<String>,
    title: Option<String>,
    date: Option<String>,
    collection: Option<String>,
    link: Option<String>,
}

impl Credit {
    /// A Credit of these fields, or `None` if every one is unknown.
    fn of_fields(
        artist: Option<String>,
        title: Option<String>,
        date: Option<String>,
        collection: Option<String>,
        link: Option<String>,
    ) -> Option<Self> {
        let credit = Self {
            artist,
            title,
            date,
            collection,
            link,
        };
        let fields = [
            &credit.artist,
            &credit.title,
            &credit.date,
            &credit.collection,
            &credit.link,
        ];
        fields.iter().any(|field| field.is_some()).then_some(credit)
    }

    pub fn artist(&self) -> Option<&str> {
        self.artist.as_deref()
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn date(&self) -> Option<&str> {
        self.date.as_deref()
    }

    pub fn collection(&self) -> Option<&str> {
        self.collection.as_deref()
    }

    /// The link as written, whatever it points to. See [`Self::web_link`] for one that is
    /// safe to open.
    pub fn link(&self) -> Option<&str> {
        self.link.as_deref()
    }

    /// The link, as written, if it is a well-formed `http` or `https` URL: the only links
    /// the app opens, so a Credits File can't make it open a file or run a script.
    pub fn web_link(&self) -> Option<&str> {
        let link = self.link.as_deref()?;
        let url = url::Url::parse(link).ok()?;
        let is_web = matches!(url.scheme(), "http" | "https") && url.has_host();
        is_web.then_some(link)
    }
}

/// The Credits read from a Credits File, by path relative to its folder with `/` separators.
#[derive(Debug, Default)]
pub(crate) struct Credits {
    /// `None` for a path whose row leaves every field blank.
    by_path: HashMap<String, Option<Credit>>,
    /// Every path the file names, in the order of its rows.
    paths: Vec<String>,
}

impl Credits {
    /// Reads `credits_file`, found at `path`. A missing file gives no Credits. One that can't
    /// be read or parsed gives none either, with a warning. A path listed in more than one
    /// row takes its first row's Credit, with a warning.
    pub(crate) fn read(
        path: &Path,
        credits_file: CreditsFileOf,
        warnings: &mut Vec<BuildWarning>,
    ) -> Self {
        let parsed = match std::fs::read(path) {
            Ok(contents) => Self::parse(&contents),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Self::default(),
            Err(error) => Err(CreditsFileError::Unreadable(error.to_string())),
        };
        match parsed {
            Ok((credits, duplicated_paths)) => {
                warnings.extend(
                    duplicated_paths
                        .into_iter()
                        .map(|path| BuildWarning::DuplicateCreditsPath { credits_file, path }),
                );
                credits
            }
            Err(reason) => {
                warnings.push(BuildWarning::UnreadableCreditsFile {
                    credits_file,
                    reason,
                });
                Self::default()
            }
        }
    }

    /// The Credits in `contents`, and each path listed in more than one row, in the order
    /// they are first repeated.
    fn parse(contents: &[u8]) -> Result<(Self, Vec<String>), CreditsFileError> {
        let unparseable = |error: csv::Error| CreditsFileError::Unparseable(error.to_string());
        let mut reader = csv::ReaderBuilder::new()
            .trim(csv::Trim::All)
            .flexible(true)
            .from_reader(contents);
        let headers = reader.headers().map_err(unparseable)?.clone();
        let column = |name: &str| headers.iter().position(|header| header == name);
        let path_column = column("path").ok_or(CreditsFileError::NoPathColumn)?;
        let [artist, title, date, collection, link] =
            ["artist", "title", "date", "collection", "link"].map(column);

        let mut by_path = HashMap::new();
        let mut paths = Vec::new();
        let mut duplicated_paths: Vec<String> = Vec::new();
        for record in reader.records() {
            let record = record.map_err(unparseable)?;
            let field = |column: Option<usize>| {
                column
                    .and_then(|column| record.get(column))
                    .filter(|field| !field.is_empty())
                    .map(str::to_owned)
            };
            let Some(path) = field(Some(path_column)) else {
                continue;
            };
            let credit = Credit::of_fields(
                field(artist),
                field(title),
                field(date),
                field(collection),
                field(link),
            );
            match by_path.entry(path) {
                Entry::Vacant(entry) => {
                    paths.push(entry.key().clone());
                    entry.insert(credit);
                }
                Entry::Occupied(entry) => {
                    if !duplicated_paths.contains(entry.key()) {
                        duplicated_paths.push(entry.key().clone());
                    }
                }
            }
        }
        Ok((Self { by_path, paths }, duplicated_paths))
    }

    /// The Credit given for `relative_path`, if any.
    pub(crate) fn of(&self, relative_path: &Path) -> Option<&Credit> {
        self.by_path.get(&slash_path(relative_path)?)?.as_ref()
    }

    /// The paths the file names that are none of `relative_paths`, in the order of its rows.
    pub(crate) fn paths_naming_none_of(&self, relative_paths: &[PathBuf]) -> Vec<String> {
        let named: HashSet<String> = relative_paths
            .iter()
            .filter_map(|path| slash_path(path))
            .collect();
        self.paths
            .iter()
            .filter(|path| !named.contains(*path))
            .cloned()
            .collect()
    }
}

/// `relative_path` written with `/` separators, as a Credits File names it on every platform.
fn slash_path(relative_path: &Path) -> Option<String> {
    let names: Option<Vec<&str>> = relative_path
        .components()
        .map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    Some(names?.join("/"))
}
