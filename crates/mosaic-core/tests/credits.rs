//! Build reads the Credits File at the root of the Source Library and attaches each Credit
//! to its Source Image.

mod common;

use common::{BLUE, Fixture, RED, solid};
use mosaic_core::{Mosaic, SourceImage, Tile};

/// A 2 × 1 Mosaic whose left Tile is filled from `red.png` and right Tile from `blue.png`.
fn red_and_blue(fixture: &Fixture) -> Mosaic {
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    fixture.add_source_image("blue.png", &solid(8, 8, BLUE));
    let target = image::RgbImage::from_fn(20, 10, |x, _| if x < 10 { RED } else { BLUE });
    let target_image = fixture.set_target_image(&target);
    fixture.build(&target_image, 2)
}

fn red(mosaic: &Mosaic) -> &SourceImage {
    mosaic.source_image_at(Tile { column: 0, row: 0 }).unwrap()
}

fn blue(mosaic: &Mosaic) -> &SourceImage {
    mosaic.source_image_at(Tile { column: 1, row: 0 }).unwrap()
}

#[test]
fn a_source_image_takes_the_credit_of_its_row_in_the_credits_file() {
    let fixture = Fixture::new();
    fixture.add_file(
        "credits.csv",
        b"path,artist,title,date,collection,link\n\
          red.png,Katsushika Hokusai,Red Fuji,c. 1830,The Met,https://example.org/red\n",
    );

    let mosaic = red_and_blue(&fixture);

    let credit = red(&mosaic).credit().expect("red.png has a Credit");
    assert_eq!(credit.artist(), Some("Katsushika Hokusai"));
    assert_eq!(credit.title(), Some("Red Fuji"));
    assert_eq!(credit.date(), Some("c. 1830"));
    assert_eq!(credit.collection(), Some("The Met"));
    assert_eq!(credit.link(), Some("https://example.org/red"));
    assert!(blue(&mosaic).credit().is_none());
}

#[test]
fn columns_come_in_any_order_only_path_is_needed_and_unknown_columns_are_ignored() {
    let fixture = Fixture::new();
    fixture.add_file(
        "credits.csv",
        b"my notes,artist,path\n\
          keep this,  Hiroshige  ,  red.png  \n",
    );

    let mosaic = red_and_blue(&fixture);

    let credit = red(&mosaic).credit().expect("red.png has a Credit");
    assert_eq!(credit.artist(), Some("Hiroshige"));
    assert_eq!(
        (
            credit.title(),
            credit.date(),
            credit.collection(),
            credit.link()
        ),
        (None, None, None, None)
    );
}

#[test]
fn paths_into_subfolders_are_written_with_slashes() {
    let fixture = Fixture::new();
    fixture.add_file("credits.csv", b"path,title\nprints/red.png,Red Fuji\n");
    fixture.add_source_image("prints/red.png", &solid(8, 8, RED));
    let target_image = fixture.set_target_image(&solid(10, 10, RED));

    let mosaic = fixture.build(&target_image, 1);

    assert_eq!(red(&mosaic).credit().unwrap().title(), Some("Red Fuji"));
}

#[test]
fn a_credits_file_saved_with_a_byte_order_mark_reads_its_first_column() {
    let fixture = Fixture::new();
    fixture.add_file(
        "credits.csv",
        "\u{feff}path,title\nred.png,Red Fuji\n".as_bytes(),
    );

    let mosaic = red_and_blue(&fixture);

    assert_eq!(red(&mosaic).credit().unwrap().title(), Some("Red Fuji"));
}
