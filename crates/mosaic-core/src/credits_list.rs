//! The Credits List: a Mosaic's Target Image and Source Images with their Credits, and how
//! many Tiles each Source Image fills, as Markdown.

use std::fmt::Write;

use crate::credit::Credit;
use crate::mosaic::Mosaic;

impl Mosaic {
    /// The Credits List as Markdown: a Target Image section with its Credit on one line, a
    /// Source Images table of every credited Source Image with how many Tiles it fills, and a
    /// Without a Credit table of the rest, left out when there are none.
    pub fn credits_list(&self) -> String {
        let mut credited = Vec::new();
        let mut uncredited = Vec::new();
        for source_image in self.source_images() {
            let tiles = self.tiles_filled_by(source_image.id()).len();
            match source_image.credit() {
                Some(credit) => credited.push((credit, source_image, tiles)),
                None => uncredited.push((source_image, tiles)),
            }
        }
        // By artist then title ignoring case, unknown ones after known ones, then file name.
        credited.sort_by_cached_key(|(credit, source_image, _)| {
            let ignoring_case = |field: Option<&str>| {
                (
                    field.is_none(),
                    field.map(|field| collapsed(field).to_lowercase()),
                )
            };
            (
                ignoring_case(credit.artist()),
                ignoring_case(credit.title()),
                source_image.file_name(),
                source_image.last_known_path(),
            )
        });
        uncredited.sort_by_key(|(source_image, _)| {
            (source_image.file_name(), source_image.last_known_path())
        });

        let mut list = String::new();
        list.push_str("## Target Image\n\n");
        list.push_str(&self.target_image_line());
        list.push_str("\n\n## Source Images\n\n");
        list.push_str("| Artist | Title | Date | Collection | Tiles |\n");
        list.push_str("| --- | --- | --- | --- | ---: |\n");
        for (credit, _, tiles) in credited {
            let text = |field: Option<&str>| field.map(text).unwrap_or_default();
            let _ = writeln!(
                list,
                "| {} | {} | {} | {} | {tiles} |",
                text(credit.artist()),
                title(credit, false).unwrap_or_default(),
                text(credit.date()),
                text(credit.collection()),
            );
        }
        if !uncredited.is_empty() {
            list.push_str("\n## Without a Credit\n\n");
            list.push_str("| File | Tiles |\n");
            list.push_str("| --- | ---: |\n");
            for (source_image, tiles) in uncredited {
                let _ = writeln!(list, "| {} | {tiles} |", text(&source_image.file_name()));
            }
        }
        list
    }

    /// The Target Image's Credit on one line: artist, italic title, date and collection. Its
    /// file name when it has no Credit.
    fn target_image_line(&self) -> String {
        let Some(credit) = self.target_image_credit() else {
            let target_image = self.target_image();
            return text(
                &target_image
                    .file_name()
                    .unwrap_or(target_image.as_os_str())
                    .to_string_lossy(),
            );
        };
        [
            credit.artist().map(text),
            title(credit, true),
            credit.date().map(text),
            credit.collection().map(text),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ")
    }
}

/// The Credit's title, or its link as written in place of a missing one, linked by its web
/// link if it has one. `None` if it has neither.
fn title(credit: &Credit, italic: bool) -> Option<String> {
    let text = text(credit.title().or(credit.link())?);
    let text = if italic { format!("*{text}*") } else { text };
    Some(match credit.web_link() {
        Some(link) => format!("[{text}]({})", destination(link)),
        None => text,
    })
}

/// `field` with its whitespace collapsed to single spaces.
fn collapsed(field: &str) -> String {
    field.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `field` as Markdown text that shows as written, on one line and inside a table: its
/// whitespace collapsed and its characters that mean something in Markdown escaped.
fn text(field: &str) -> String {
    let mut text = String::new();
    for character in collapsed(field).chars() {
        if matches!(
            character,
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '|' | '~'
        ) {
            text.push('\\');
        }
        text.push(character);
    }
    text
}

/// `web_link` as a Markdown link destination, with the characters that would end it, or a
/// table's column, percent-encoded.
fn destination(web_link: &str) -> String {
    let mut destination = String::new();
    for character in web_link.chars() {
        match character {
            '(' | ')' | '<' | '>' | '|' | '\\' | '`' => {
                let _ = write!(destination, "%{:02X}", character as u32);
            }
            character if character.is_whitespace() => destination.push_str("%20"),
            character => destination.push(character),
        }
    }
    destination
}
