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
            let ignoring_case =
                |field: Option<&str>| (field.is_none(), field.map(str::to_lowercase));
            (
                ignoring_case(credit.artist()),
                ignoring_case(credit.title()),
                source_image.file_name(),
                source_image.last_known_path(),
            )
        });
        uncredited.sort_by_key(|(source_image, _)| source_image.file_name());

        let mut list = String::new();
        list.push_str("## Target Image\n\n");
        list.push_str(&self.target_image_line());
        list.push_str("\n\n## Source Images\n\n");
        list.push_str("| Artist | Title | Date | Collection | Tiles |\n");
        list.push_str("| --- | --- | --- | --- | ---: |\n");
        for (credit, _, tiles) in credited {
            let text = |field: Option<&str>| field.unwrap_or_default().to_owned();
            let _ = writeln!(
                list,
                "| {} | {} | {} | {} | {tiles} |",
                text(credit.artist()),
                title(credit, false),
                text(credit.date()),
                text(credit.collection()),
            );
        }
        if !uncredited.is_empty() {
            list.push_str("\n## Without a Credit\n\n");
            list.push_str("| File | Tiles |\n");
            list.push_str("| --- | ---: |\n");
            for (source_image, tiles) in uncredited {
                let _ = writeln!(list, "| {} | {tiles} |", source_image.file_name());
            }
        }
        list
    }

    /// The Target Image's Credit on one line: artist, italic title, date and collection. Its
    /// file name when it has no Credit.
    fn target_image_line(&self) -> String {
        let Some(credit) = self.target_image_credit() else {
            let target_image = self.target_image();
            return target_image
                .file_name()
                .unwrap_or(target_image.as_os_str())
                .to_string_lossy()
                .into_owned();
        };
        let title = credit.title().map(|_| title(credit, true));
        [
            credit.artist().map(str::to_owned),
            title,
            credit.date().map(str::to_owned),
            credit.collection().map(str::to_owned),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ")
    }
}

/// The Credit's title, linked by its web link if it has one.
fn title(credit: &Credit, italic: bool) -> String {
    let text = credit.title().unwrap_or_default();
    let text = if italic {
        format!("*{text}*")
    } else {
        text.to_owned()
    };
    match credit.web_link() {
        Some(link) => format!("[{text}]({link})"),
        None => text,
    }
}
