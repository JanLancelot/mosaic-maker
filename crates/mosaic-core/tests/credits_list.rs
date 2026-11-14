//! A Mosaic's Credits List: its Target Image's Credit, then its Source Images with their
//! Credits and how many Tiles each fills, as Markdown.

mod common;

use common::{Fixture, striped_mosaic};

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
