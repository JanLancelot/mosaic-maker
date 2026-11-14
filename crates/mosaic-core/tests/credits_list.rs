//! A Mosaic's Credits List: its Target Image's Credit, then its Source Images with their
//! Credits and how many Tiles each fills, as Markdown.

mod common;

use common::{Fixture, solid, striped_mosaic};
use image::{Rgb, RgbImage};
use mosaic_core::{BuildSettings, Mosaic};

/// Writes `contents` as the Credits File beside the Target Image, outside the Source Library.
fn add_target_image_credits_file(fixture: &Fixture, contents: &[u8]) {
    std::fs::write(fixture.path("credits.csv"), contents).unwrap();
}

#[test]
fn the_credits_list_credits_the_target_image_then_each_source_image_with_its_tiles() {
    let fixture = Fixture::new();
    add_target_image_credits_file(
        &fixture,
        b"path,artist,title,date,collection,link\n\
          target.png,Katsushika Hokusai,The Great Wave,c. 1831,The Met,https://example.org/wave\n",
    );
    fixture.add_file(
        "credits.csv",
        b"path,artist,title,date,collection,link\n\
          red.png,Utagawa Hiroshige,Red Fuji,1830,CMA 1998.192,https://example.org/red\n\
          colors/blue.png,Adolphe Braun,Blue Sea,c. 1869,,\n",
    );
    // Red fills 3 Tiles, blue 2 and green 1.
    let mosaic = striped_mosaic(&fixture);

    assert_eq!(
        mosaic.credits_list(),
        "## Target Image\n\
         \n\
         Katsushika Hokusai, [*The Great Wave*](https://example.org/wave), c. 1831, The Met\n\
         \n\
         ## Source Images\n\
         \n\
         | Artist | Title | Date | Collection | Tiles |\n\
         | --- | --- | --- | --- | ---: |\n\
         | Adolphe Braun | Blue Sea | c. 1869 |  | 2 |\n\
         | Utagawa Hiroshige | [Red Fuji](https://example.org/red) | 1830 | CMA 1998.192 | 3 |\n\
         \n\
         ## Without a Credit\n\
         \n\
         | File | Tiles |\n\
         | --- | ---: |\n\
         | green.png | 1 |\n"
    );
}

/// A one-row Mosaic of Source Images at `paths`, each a distinct gray filling one Tile, in
/// order, credited by `credits_file` in the Source Library.
fn one_tile_each(fixture: &Fixture, paths: &[&str], credits_file: &[u8]) -> Mosaic {
    fixture.add_file("credits.csv", credits_file);
    let gray = |i: usize| Rgb([(i * 255 / paths.len()) as u8; 3]);
    for (i, path) in paths.iter().enumerate() {
        fixture.add_source_image(path, &solid(8, 8, gray(i)));
    }
    let target = RgbImage::from_fn(10 * paths.len() as u32, 10, |x, _| gray(x as usize / 10));
    let target_image = fixture.set_target_image(&target);
    let settings = BuildSettings {
        tiles_across: paths.len() as u32,
        repeat_radius: 0,
        seed: 0,
    };
    fixture.build_with_settings(&target_image, &settings).mosaic
}

/// The Source Images section of `mosaic`'s Credits List, from its table's first row up to the
/// next section.
fn source_image_rows(mosaic: &Mosaic) -> String {
    let list = mosaic.credits_list();
    let table = list
        .split("| --- | --- | --- | --- | ---: |\n")
        .nth(1)
        .expect("a Source Images table");
    table.split("\n## ").next().unwrap().to_owned()
}

#[test]
fn source_images_are_sorted_by_artist_then_title_ignoring_case_with_no_artist_last_then_file_name()
{
    let fixture = Fixture::new();
    let mosaic = one_tile_each(
        &fixture,
        &[
            "a.png", "z/b.png", "c.png", "d.png", "a/e.png", "f.png", "g.png",
        ],
        b"path,artist,title,date\n\
          a.png,utagawa,B,\n\
          z/b.png,Abraham,z,\n\
          c.png,,Alpha,\n\
          d.png,Utagawa,a,\n\
          a/e.png,abraham,Z,\n\
          f.png,,beta,\n\
          g.png,,,1900\n",
    );

    assert_eq!(
        source_image_rows(&mosaic),
        "| Abraham | z |  |  | 1 |\n\
         | abraham | Z |  |  | 1 |\n\
         | Utagawa | a |  |  | 1 |\n\
         | utagawa | B |  |  | 1 |\n\
         |  | Alpha |  |  | 1 |\n\
         |  | beta |  |  | 1 |\n\
         |  |  | 1900 |  | 1 |\n"
    );
}

#[test]
fn a_target_image_without_a_credit_is_listed_by_its_file_name() {
    let fixture = Fixture::new();
    let mosaic = striped_mosaic(&fixture);

    let list = mosaic.credits_list();

    assert!(
        list.starts_with("## Target Image\n\ntarget.png\n\n## Source Images\n"),
        "{list}"
    );
}

#[test]
fn only_a_web_link_links_a_title_and_a_link_without_a_title_stands_in_for_it() {
    let fixture = Fixture::new();
    let mosaic = one_tile_each(
        &fixture,
        &["a.png", "b.png", "c.png", "d.png", "e.png"],
        b"path,artist,title,link\n\
          a.png,A,Web,https://example.org/a\n\
          b.png,B,File,file:///etc/passwd\n\
          c.png,C,Script,javascript:alert(1)\n\
          d.png,D,,http://example.org/d\n\
          e.png,E,,pages/e.html\n",
    );

    assert_eq!(
        source_image_rows(&mosaic),
        "| A | [Web](https://example.org/a) |  |  | 1 |\n\
         | B | File |  |  | 1 |\n\
         | C | Script |  |  | 1 |\n\
         | D | [http://example.org/d](http://example.org/d) |  |  | 1 |\n\
         | E | pages/e.html |  |  | 1 |\n"
    );
}

/// The Target Image line of `mosaic`'s Credits List.
fn target_image_line(mosaic: &Mosaic) -> String {
    let list = mosaic.credits_list();
    let line = list
        .strip_prefix("## Target Image\n\n")
        .expect("a Target Image section first");
    line.lines().next().unwrap().to_owned()
}

#[test]
fn the_target_image_line_links_its_italic_title_only_by_a_web_link_and_leaves_out_unknown_fields() {
    let fixture = Fixture::new();
    add_target_image_credits_file(
        &fixture,
        b"path,artist,title,date,collection,link\n\
          target.png,,The Great Wave,,The Met,file:///wave.html\n",
    );
    let mosaic = striped_mosaic(&fixture);

    assert_eq!(target_image_line(&mosaic), "*The Great Wave*, The Met");
}

#[test]
fn a_target_image_link_without_a_title_stands_in_for_it() {
    let fixture = Fixture::new();
    add_target_image_credits_file(
        &fixture,
        b"path,artist,link\ntarget.png,Katsushika Hokusai,https://example.org/wave\n",
    );
    let mosaic = striped_mosaic(&fixture);

    assert_eq!(
        target_image_line(&mosaic),
        "Katsushika Hokusai, [*https://example.org/wave*](https://example.org/wave)"
    );
}

#[test]
fn every_field_is_escaped_for_markdown_with_its_whitespace_collapsed() {
    let fixture = Fixture::new();
    add_target_image_credits_file(
        &fixture,
        b"path,artist,title,date,link\n\
          target.png,Hokusai | Studio,The *Great*  Wave,\"c.\n\t1831\",https://example.org/wave_(1)|2\n",
    );
    let mosaic = one_tile_each(
        &fixture,
        &["a.png", "no_credit [copy].png"],
        b"path,artist,title,collection\n\
          a.png,<b>Smith</b>   & Jones,`Code` _under_ [x](y) ~s~ back\\slash,A | B\n",
    );

    let list = mosaic.credits_list();

    assert_eq!(
        target_image_line(&mosaic),
        "Hokusai \\| Studio, [*The \\*Great\\* Wave*](https://example.org/wave_%281%29%7C2), c. 1831"
    );
    assert_eq!(
        source_image_rows(&mosaic),
        "| \\<b\\>Smith\\</b\\> & Jones \
         | \\`Code\\` \\_under\\_ \\[x\\](y) \\~s\\~ back\\\\slash |  | A \\| B | 1 |\n"
    );
    assert!(
        list.ends_with("| File | Tiles |\n| --- | ---: |\n| no\\_credit \\[copy\\].png | 1 |\n"),
        "{list}"
    );
}

#[test]
fn the_without_a_credit_section_is_left_out_when_every_source_image_has_a_credit() {
    let fixture = Fixture::new();
    fixture.add_file(
        "credits.csv",
        b"path,title\nred.png,Red\ncolors/blue.png,Blue\ncolors/green.png,Green\n",
    );
    let mosaic = striped_mosaic(&fixture);

    let list = mosaic.credits_list();

    assert!(
        list.ends_with(
            "| --- | --- | --- | --- | ---: |\n\
             |  | Blue |  |  | 2 |\n\
             |  | Green |  |  | 1 |\n\
             |  | Red |  |  | 3 |\n"
        ),
        "{list}"
    );
}
