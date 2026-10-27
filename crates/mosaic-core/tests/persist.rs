mod common;

use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use common::{Fixture, striped_mosaic, striped_mosaic_with, tile, tiles};
use mosaic_core::{BuildSettings, Mosaic, OpenError};

const SETTINGS: BuildSettings = BuildSettings {
    tiles_across: 3,
    repeat_radius: 0,
    seed: 7,
};

#[test]
fn an_opened_mosaic_has_the_saved_settings_grid_tiles_and_inspect_results() {
    let fixture = Fixture::new();
    let mosaic = striped_mosaic_with(&fixture, &SETTINGS);
    let path = fixture.path("striped.mosaic");

    mosaic.save(&path).unwrap();
    let opened = Mosaic::open(&path).unwrap();

    assert_eq!(*opened.settings(), SETTINGS);
    assert_eq!(opened.target_image(), fixture.path("target.png"));
    assert_eq!(opened.source_library(), fixture.source_library());
    assert_eq!((opened.tiles_across(), opened.rows()), (3, 2));
    let blue = opened.source_image_at(tile(1, 0)).unwrap();
    assert_eq!(blue.file_name(), "blue.png");
    assert_eq!(
        opened.path_of(blue),
        fixture.source_library().join("colors/blue.png")
    );
    for tile in tiles(&mosaic) {
        let saved = mosaic.source_image_at(tile).unwrap();
        let reopened = opened.source_image_at(tile).unwrap();
        assert_eq!(reopened, saved, "the Source Image filling {tile:?}");
        assert_eq!(
            opened.tiles_filled_by(reopened.id()),
            mosaic.tiles_filled_by(saved.id())
        );
        assert!(opened.original(reopened, 100).is_some());
    }
    assert_eq!(opened.rendered(), mosaic.rendered());
}

#[test]
fn a_saved_mosaic_opens_and_is_inspectable_after_its_source_library_is_deleted() {
    let fixture = Fixture::new();
    let mosaic = striped_mosaic_with(&fixture, &SETTINGS);
    let path = fixture.path("striped.mosaic");
    mosaic.save(&path).unwrap();
    std::fs::remove_dir_all(fixture.source_library()).unwrap();

    let opened = Mosaic::open(&path).unwrap();

    for tile in tiles(&opened) {
        let source_image = opened.source_image_at(tile).unwrap();
        assert_eq!(
            source_image.thumbnail(),
            mosaic.source_image_at(tile).unwrap().thumbnail()
        );
        assert!(opened.tiles_filled_by(source_image.id()).contains(&tile));
        assert_eq!(opened.original(source_image, 100), None, "{tile:?}");
    }
    let blue = opened.source_image_at(tile(1, 0)).unwrap();
    assert_eq!(blue.file_name(), "blue.png");
    assert_eq!(
        opened.path_of(blue),
        fixture.source_library().join("colors/blue.png")
    );
}

#[test]
fn opening_a_file_that_isnt_a_mosaic_is_an_error() {
    let fixture = Fixture::new();
    let path = fixture.path("garbage.mosaic");
    std::fs::write(&path, b"not a zip at all").unwrap();

    assert!(matches!(Mosaic::open(&path), Err(OpenError::Corrupt(_))));
}

#[test]
fn opening_a_truncated_mosaic_file_is_an_error() {
    let fixture = Fixture::new();
    let path = fixture.path("striped.mosaic");
    striped_mosaic_with(&fixture, &SETTINGS)
        .save(&path)
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    std::fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();

    assert!(matches!(Mosaic::open(&path), Err(OpenError::Corrupt(_))));
}

#[test]
fn opening_a_missing_file_is_an_error() {
    let fixture = Fixture::new();

    let opened = Mosaic::open(&fixture.path("missing.mosaic"));

    assert!(matches!(opened, Err(OpenError::Unreadable(_))));
}

#[test]
fn saving_over_an_existing_file_replaces_it() {
    let fixture = Fixture::new();
    let path = fixture.path("striped.mosaic");
    std::fs::write(&path, b"an older file").unwrap();

    striped_mosaic_with(&fixture, &SETTINGS)
        .save(&path)
        .unwrap();

    assert_eq!(*Mosaic::open(&path).unwrap().settings(), SETTINGS);
}

/// Rewrites `format_version` in the manifest of the Mosaic File at `path`, copying every other
/// entry as it is.
fn set_format_version(path: &Path, format_version: u32) {
    let mut zip = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
    let mut rewritten = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).unwrap();
        let name = entry.name().unwrap().into_owned();
        let mut contents = Vec::new();
        entry.read_to_end(&mut contents).unwrap();
        if name == "manifest.json" {
            let mut manifest: serde_json::Value = serde_json::from_slice(&contents).unwrap();
            manifest["format_version"] = format_version.into();
            contents = serde_json::to_vec(&manifest).unwrap();
        }
        rewritten
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        rewritten.write_all(&contents).unwrap();
    }
    std::fs::write(path, rewritten.finish().unwrap().into_inner()).unwrap();
}

#[test]
fn opening_a_mosaic_file_of_another_format_version_is_an_error() {
    let fixture = Fixture::new();
    let path = fixture.path("striped.mosaic");
    striped_mosaic(&fixture).save(&path).unwrap();
    set_format_version(&path, 2);

    let opened = Mosaic::open(&path);

    assert!(matches!(opened, Err(OpenError::UnsupportedVersion(2))));
}
