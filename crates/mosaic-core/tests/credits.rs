//! Build reads the Credits File at the root of the Source Library and attaches each Credit
//! to its Source Image.

mod common;

use std::path::Path;

use common::{BLUE, Fixture, RED, solid, striped_mosaic_with, tiles};
use image::{Rgb, RgbImage};
use mosaic_core::{
    BuildSettings, BuildWarning, Built, CreditsFileError, CreditsFileOf, Mosaic, SourceImage,
    SourceImageId, Tile,
};

/// A 2 × 1 Mosaic whose left Tile is filled from `red.png` and right Tile from `blue.png`.
fn red_and_blue(fixture: &Fixture) -> Mosaic {
    build_red_and_blue(fixture).mosaic
}

/// Like `red_and_blue`, with the build's report.
fn build_red_and_blue(fixture: &Fixture) -> Built {
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    fixture.add_source_image("blue.png", &solid(8, 8, BLUE));
    let target = RgbImage::from_fn(20, 10, |x, _| if x < 10 { RED } else { BLUE });
    let target_image = fixture.set_target_image(&target);
    fixture.build_reporting(&target_image, 2)
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
fn the_credits_file_is_not_considered_as_a_source_image() {
    let fixture = Fixture::new();
    fixture.add_file("credits.csv", b"path,title\nred.png,Red Fuji\n");
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    let target_image = fixture.set_target_image(&solid(10, 10, RED));

    let built = fixture.build_reporting(&target_image, 1);

    assert_eq!(built.report.warnings, []);
    assert_eq!(built.report.stats.source_images, 1);
}

#[test]
fn a_picture_stored_at_several_paths_takes_the_credit_of_its_first_credited_path() {
    let fixture = Fixture::new();
    fixture.add_file(
        "credits.csv",
        b"path,title\nc/red.png,Third copy\nb/red.png,Second copy\n",
    );
    for path in ["a/red.png", "b/red.png", "c/red.png"] {
        fixture.add_source_image(path, &solid(8, 8, RED));
    }
    let target_image = fixture.set_target_image(&solid(10, 10, RED));

    let mosaic = fixture.build(&target_image, 1);

    let source_image = red(&mosaic);
    assert_eq!(source_image.last_known_path(), Path::new("a/red.png"));
    assert_eq!(source_image.credit().unwrap().title(), Some("Second copy"));
}

#[test]
fn credits_never_change_which_source_image_fills_which_tile() {
    let fixture = Fixture::new();
    let settings = BuildSettings {
        tiles_across: 3,
        repeat_radius: 0,
        seed: 7,
    };
    let without_credits = striped_mosaic_with(&fixture, &settings);
    fixture.add_file(
        "credits.csv",
        b"path,artist,title\nred.png,Someone,Red\ncolors/blue.png,Someone else,Blue\n",
    );
    let target_image = fixture.path("target.png");

    let with_credits = fixture.build_with_settings(&target_image, &settings);

    assert!(
        without_credits
            .source_image_at(Tile { column: 0, row: 0 })
            .is_some_and(|source_image| source_image.credit().is_none())
    );
    assert_eq!(with_credits.report.warnings, []);
    assert_eq!(mapping(&with_credits.mosaic), mapping(&without_credits));
    assert_eq!(with_credits.mosaic.rendered(), without_credits.rendered());
    assert!(tiles(&with_credits.mosaic).iter().any(|&tile| {
        with_credits
            .mosaic
            .source_image_at(tile)
            .unwrap()
            .credit()
            .is_some()
    }));
}

/// The identity of the Source Image filling each Tile, row by row.
fn mapping(mosaic: &Mosaic) -> Vec<SourceImageId> {
    tiles(mosaic)
        .into_iter()
        .map(|tile| mosaic.source_image_at(tile).unwrap().id())
        .collect()
}

#[test]
fn only_http_and_https_links_are_offered_as_web_links() {
    let links = [
        (
            "https://example.org/a?b=c",
            Some("https://example.org/a?b=c"),
        ),
        ("http://example.org", Some("http://example.org")),
        ("HTTPS://EXAMPLE.ORG/x", Some("HTTPS://EXAMPLE.ORG/x")),
        ("file:///etc/passwd", None),
        ("javascript:alert(1)", None),
        ("example.org/artwork", None),
        ("/artworks/1", None),
        ("https://", None),
        ("http//example.org", None),
        ("mailto:someone@example.org", None),
    ];
    let fixture = Fixture::new();
    let mut csv = String::from("path,link\n");
    let mut colors = Vec::new();
    for (index, (link, _)) in links.iter().enumerate() {
        let color = Rgb([index as u8 * 25, 0, 0]);
        fixture.add_source_image(&format!("{index}.png"), &solid(8, 8, color));
        csv.push_str(&format!("{index}.png,{link}\n"));
        colors.push(color);
    }
    fixture.add_file("credits.csv", csv.as_bytes());
    let target = RgbImage::from_fn(10 * links.len() as u32, 10, |x, _| {
        colors[(x / 10) as usize]
    });
    let target_image = fixture.set_target_image(&target);
    let settings = BuildSettings {
        tiles_across: links.len() as u32,
        repeat_radius: 0,
        seed: 0,
    };

    let mosaic = fixture.build_with_settings(&target_image, &settings).mosaic;

    for (column, (link, web_link)) in links.into_iter().enumerate() {
        let tile = Tile {
            column: column as u32,
            row: 0,
        };
        let credit = mosaic.source_image_at(tile).unwrap().credit().unwrap();
        assert_eq!(credit.link(), Some(link));
        assert_eq!(credit.web_link(), web_link, "web link of {link}");
    }
}

#[test]
fn a_row_with_every_field_blank_gives_no_credit() {
    let fixture = Fixture::new();
    fixture.add_file(
        "credits.csv",
        b"path,artist,title,date,collection,link\nred.png, ,,  ,,\n",
    );

    let mosaic = red_and_blue(&fixture);

    assert!(red(&mosaic).credit().is_none());
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

#[test]
fn a_credits_file_with_no_path_column_is_reported_and_the_build_goes_on_without_credits() {
    let fixture = Fixture::new();
    fixture.add_file("credits.csv", b"file,title\nred.png,Red Fuji\n");

    let built = build_red_and_blue(&fixture);

    assert_eq!(
        built.report.warnings,
        [BuildWarning::UnreadableCreditsFile {
            credits_file: CreditsFileOf::SourceLibrary,
            reason: CreditsFileError::NoPathColumn,
        }]
    );
    assert!(red(&built.mosaic).credit().is_none());
}

#[test]
fn an_unparseable_credits_file_is_reported_with_its_reason_and_none_of_its_credits_are_used() {
    let fixture = Fixture::new();
    // A good row, then one that isn't UTF-8, as a file saved in another encoding might be.
    fixture.add_file(
        "credits.csv",
        b"path,title\nred.png,Red Fuji\nblue.png,Blue \xff\xfe\n",
    );

    let built = build_red_and_blue(&fixture);

    match &built.report.warnings[..] {
        [
            BuildWarning::UnreadableCreditsFile {
                credits_file: CreditsFileOf::SourceLibrary,
                reason: CreditsFileError::Unparseable(reason),
            },
        ] => assert!(reason.contains("UTF-8"), "{reason}"),
        warnings => panic!("unexpected warnings {warnings:?}"),
    }
    assert!(red(&built.mosaic).credit().is_none());
    assert!(blue(&built.mosaic).credit().is_none());
}

#[test]
fn rows_naming_no_file_in_the_source_library_are_listed_in_one_warning() {
    let fixture = Fixture::new();
    fixture.add_file(
        "credits.csv",
        b"path,title\n\
          Red.png,Wrong case\n\
          red.png,Red Fuji\n\
          prints/blue.png,Moved since\n\
          blue.png,\n",
    );

    let built = build_red_and_blue(&fixture);

    assert_eq!(
        built.report.warnings,
        [BuildWarning::UnknownCreditsPaths {
            paths: vec!["Red.png".to_owned(), "prints/blue.png".to_owned()],
        }]
    );
    assert_eq!(
        red(&built.mosaic).credit().unwrap().title(),
        Some("Red Fuji")
    );
}

#[test]
fn a_path_listed_more_than_once_is_warned_about_once_and_its_first_row_is_used() {
    let fixture = Fixture::new();
    fixture.add_file(
        "credits.csv",
        b"path,title\n\
          blue.png,\n\
          red.png,Red Fuji\n\
          red.png,Second row\n\
          blue.png,Blue Wave\n\
          red.png,Third row\n",
    );

    let built = build_red_and_blue(&fixture);

    assert_eq!(
        built.report.warnings,
        [
            BuildWarning::DuplicateCreditsPath {
                credits_file: CreditsFileOf::SourceLibrary,
                path: "red.png".to_owned(),
            },
            BuildWarning::DuplicateCreditsPath {
                credits_file: CreditsFileOf::SourceLibrary,
                path: "blue.png".to_owned(),
            },
        ]
    );
    assert_eq!(
        red(&built.mosaic).credit().unwrap().title(),
        Some("Red Fuji")
    );
    assert!(blue(&built.mosaic).credit().is_none());
}

#[test]
fn no_credits_file_gives_no_warning() {
    let fixture = Fixture::new();

    let built = build_red_and_blue(&fixture);

    assert_eq!(built.report.warnings, []);
}

#[test]
fn the_report_tells_the_credits_file_problems_apart_from_skipped_files() {
    let fixture = Fixture::new();
    fixture.add_file("notes.txt", b"not a picture");
    fixture.add_file(
        "credits.csv",
        b"path,title\nred.png,Red Fuji\nred.png,Again\ngone.png,Gone\n",
    );

    let report = build_red_and_blue(&fixture).report;

    let problems: Vec<_> = report.credits_file_problems().collect();
    assert_eq!(
        problems,
        [
            &BuildWarning::DuplicateCreditsPath {
                credits_file: CreditsFileOf::SourceLibrary,
                path: "red.png".to_owned(),
            },
            &BuildWarning::UnknownCreditsPaths {
                paths: vec!["gone.png".to_owned()],
            },
        ]
    );
    let skipped: Vec<_> = report.skipped_files().collect();
    assert!(
        matches!(skipped[..], [BuildWarning::SkippedFile { path, .. }] if path == Path::new("notes.txt")),
        "{skipped:?}"
    );
}
