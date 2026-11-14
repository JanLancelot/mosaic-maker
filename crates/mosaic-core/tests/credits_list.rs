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
