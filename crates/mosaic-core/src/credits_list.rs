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

/// The Credit's shown title, linked by its web link if it has one. `None` if it has none.
fn title(credit: &Credit, italic: bool) -> Option<String> {
    let text = text(credit.shown_title()?);
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
/// whitespace collapsed and its characters that mean something in Markdown escaped. A `&` is
/// escaped only where it starts an entity, a `!` only before a bracket, and the field's start
/// only where it would begin a heading or list, so ordinary text such as "Braun & Co." stays as
/// written.
fn text(field: &str) -> String {
    let field = collapsed(field);
    let marker = block_marker(&field);
    let mut text = String::new();
    for (at, character) in field.char_indices() {
        let rest = &field[at..];
        let escape = match character {
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '|' | '~' => true,
            '&' => starts_entity(rest),
            '!' => rest[1..].starts_with('['),
            _ => marker == Some(at),
        };
        if escape {
            text.push('\\');
        }
        text.push(character);
    }
    text
}

/// Where the character is that would make `field`, at the start of a line, begin a heading or
/// a list: a leading `#`, `-` or `+`, or the `.` or `)` after leading digits that ends an
/// ordered list's number.
fn block_marker(field: &str) -> Option<usize> {
    if field.starts_with(['#', '-', '+']) {
        return Some(0);
    }
    let digits = field.bytes().take_while(u8::is_ascii_digit).count();
    let after = field[digits..].strip_prefix(['.', ')'])?;
    let ends_number = after.is_empty() || after.starts_with(' ');
    ((1..=9).contains(&digits) && ends_number).then_some(digits)
}

/// Whether `rest`, which starts with `&`, starts with an entity such as `&amp;` or `&#38;`.
fn starts_entity(rest: &str) -> bool {
    let name = &rest[1..];
    let name = name.strip_prefix('#').unwrap_or(name);
    let length = name.bytes().take_while(u8::is_ascii_alphanumeric).count();
    length > 0 && name[length..].starts_with(';')
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
