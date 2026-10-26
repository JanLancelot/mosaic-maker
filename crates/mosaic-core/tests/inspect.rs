mod common;

use std::collections::HashSet;

use common::{BLUE, Fixture, GREEN, RED, solid, tiles};
use image::{Rgb, RgbImage};
use mosaic_core::{AnalysisCache, BuildSettings, CancelToken, Mosaic, THUMBNAIL_SIZE, Tile, build};

fn tile(column: u32, row: u32) -> Tile {
    Tile { column, row }
}

/// A 3 × 2 Mosaic, Repeat Radius 0, whose Tiles are red, blue, red / green, red, blue.
fn striped_mosaic(fixture: &Fixture) -> Mosaic {
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    fixture.add_source_image("colors/blue.png", &solid(8, 8, BLUE));
    fixture.add_source_image("colors/green.png", &solid(8, 8, GREEN));
    let colors = [[RED, BLUE, RED], [GREEN, RED, BLUE]];
    let target = RgbImage::from_fn(30, 20, |x, y| colors[(y / 10) as usize][(x / 10) as usize]);
    let target_image = fixture.set_target_image(&target);
    let settings = BuildSettings {
        tiles_across: 3,
        repeat_radius: 0,
        seed: 0,
    };
    fixture.build_with_settings(&target_image, &settings).mosaic
}

#[test]
fn every_tile_a_source_image_fills_is_found() {
    let fixture = Fixture::new();
    let mosaic = striped_mosaic(&fixture);
    let id_at = |tile| mosaic.source_image_at(tile).unwrap().id();

    assert_eq!(
        mosaic.tiles_filled_by(id_at(tile(0, 0))),
        [tile(0, 0), tile(2, 0), tile(1, 1)]
    );
    assert_eq!(
        mosaic.tiles_filled_by(id_at(tile(1, 0))),
        [tile(1, 0), tile(2, 1)]
    );
    assert_eq!(mosaic.tiles_filled_by(id_at(tile(0, 1))), [tile(0, 1)]);
}

#[test]
fn the_source_images_used_fill_every_tile_exactly_once_between_them() {
    let fixture = Fixture::new();
    let mosaic = striped_mosaic(&fixture);

    let used: HashSet<_> = tiles(&mosaic)
        .into_iter()
        .map(|tile| mosaic.source_image_at(tile).unwrap().id())
        .collect();
    let mut filled: Vec<Tile> = used
        .into_iter()
        .flat_map(|id| mosaic.tiles_filled_by(id))
        .collect();
    filled.sort_by_key(|tile| (tile.row, tile.column));

    assert_eq!(filled, tiles(&mosaic));
}

#[test]
fn the_source_image_filling_a_tile_gives_its_filename_and_full_path() {
    let fixture = Fixture::new();
    let mosaic = striped_mosaic(&fixture);

    let blue = mosaic.source_image_at(tile(1, 0)).unwrap();

    assert_eq!(blue.file_name(), "blue.png");
    assert_eq!(
        mosaic.path_of(blue),
        fixture.source_library().join("colors/blue.png")
    );
    assert!(mosaic.path_of(blue).is_absolute());
}

/// 400 × 200, with green bands either side of a central square that is red on its left half
/// and blue on its right.
fn banded_red_blue() -> RgbImage {
    RgbImage::from_fn(400, 200, |x, _| match x {
        100..200 => RED,
        200..300 => BLUE,
        _ => GREEN,
    })
}

/// Asserts that `thumbnail` shows `banded_red_blue` center-cropped and unaltered: red on the
/// left, blue on the right, no green. Pixels near the red-blue edge may blend from scaling.
fn assert_banded_red_blue_thumbnail(thumbnail: &RgbImage) {
    assert_eq!(thumbnail.dimensions(), (THUMBNAIL_SIZE, THUMBNAIL_SIZE));
    let middle = THUMBNAIL_SIZE / 2;
    for (x, y, pixel) in thumbnail.enumerate_pixels() {
        let expected = match x {
            x if x + 4 < middle => RED,
            x if x >= middle + 4 => BLUE,
            _ => continue,
        };
        assert_eq!(*pixel, expected, "thumbnail pixel ({x}, {y})");
    }
}

fn red_left_blue_right_target(fixture: &Fixture) -> std::path::PathBuf {
    fixture.set_target_image(&RgbImage::from_fn(
        20,
        20,
        |x, _| if x < 10 { RED } else { BLUE },
    ))
}

#[test]
fn every_source_image_used_has_a_thumbnail_center_cropped_to_128_px_and_unaltered() {
    let fixture = Fixture::new();
    fixture.add_source_image("banded.png", &banded_red_blue());
    fixture.add_source_image("tiny_gray.png", &solid(4, 4, Rgb([90; 3])));
    let target = RgbImage::from_fn(40, 20, |x, _| if x < 20 { RED } else { Rgb([90; 3]) });
    let target_image = fixture.set_target_image(&target);

    let mosaic = fixture.build(&target_image, 2);

    let banded = mosaic.source_image_at(tile(0, 0)).unwrap();
    assert_eq!(banded.file_name(), "banded.png");
    assert_banded_red_blue_thumbnail(banded.thumbnail());
    let gray = mosaic.source_image_at(tile(1, 0)).unwrap();
    assert_eq!(gray.file_name(), "tiny_gray.png");
    assert_eq!(
        *gray.thumbnail(),
        solid(THUMBNAIL_SIZE, THUMBNAIL_SIZE, Rgb([90; 3]))
    );
}

#[test]
fn thumbnails_are_the_same_when_the_analysis_comes_from_the_cache() {
    let fixture = Fixture::new();
    fixture.add_source_image("banded.png", &banded_red_blue());
    let target_image = red_left_blue_right_target(&fixture);
    let first = fixture.build_reporting(&target_image, 1);

    let second = fixture.build_reporting(&target_image, 1);

    assert_eq!(second.report.stats.cache_hits, 1);
    assert_banded_red_blue_thumbnail(
        second
            .mosaic
            .source_image_at(tile(0, 0))
            .unwrap()
            .thumbnail(),
    );
    assert_eq!(second.mosaic.rendered(), first.mosaic.rendered());
}

#[test]
fn thumbnails_are_made_afresh_when_the_cache_is_corrupt() {
    let fixture = Fixture::new();
    fixture.add_source_image("banded.png", &banded_red_blue());
    let target_image = red_left_blue_right_target(&fixture);
    fixture.build(&target_image, 1);
    let cache = fixture.cache_folder();
    let mut folders = vec![cache];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                std::fs::write(path, b"garbage").unwrap();
            }
        }
    }

    let mosaic = fixture.build(&target_image, 1);

    assert_banded_red_blue_thumbnail(mosaic.source_image_at(tile(0, 0)).unwrap().thumbnail());
}

#[test]
fn thumbnails_are_made_even_when_the_cache_cannot_be_written() {
    let fixture = Fixture::new();
    fixture.add_source_image("banded.png", &banded_red_blue());
    let target_image = red_left_blue_right_target(&fixture);
    let settings = BuildSettings {
        tiles_across: 1,
        repeat_radius: 0,
        seed: 0,
    };
    // Never written, since it lies inside the Source Library.
    let cache = AnalysisCache::in_folder(fixture.source_library().join("cache"));

    let built = build(
        &target_image,
        &fixture.source_library(),
        &settings,
        &cache,
        |_| {},
        &CancelToken::new(),
    )
    .unwrap();

    assert_banded_red_blue_thumbnail(
        built
            .mosaic
            .source_image_at(tile(0, 0))
            .unwrap()
            .thumbnail(),
    );
}

#[test]
fn the_preview_shows_the_whole_original_scaled_to_fit() {
    let fixture = Fixture::new();
    fixture.add_source_image("banded.png", &banded_red_blue());
    let mosaic = fixture.build(&red_left_blue_right_target(&fixture), 1);
    let source_image = mosaic.source_image_at(tile(0, 0)).unwrap();

    let preview = mosaic.preview(source_image, 100).unwrap();

    // 400 × 200 scaled to fit 100 px, uncropped: green bands either side of red and blue.
    assert_eq!(preview.dimensions(), (100, 50));
    for (x, color) in [(5, GREEN), (37, RED), (62, BLUE), (94, GREEN)] {
        assert_eq!(*preview.get_pixel(x, 25), color, "preview pixel ({x}, 25)");
    }
    let small = mosaic.preview(source_image, 1000).unwrap();
    assert_eq!(small, banded_red_blue(), "a preview is never scaled up");
}

#[test]
fn there_is_no_preview_once_the_original_is_gone_or_edited() {
    let fixture = Fixture::new();
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    fixture.add_source_image("blue.png", &solid(8, 8, BLUE));
    let mosaic = fixture.build(&red_left_blue_right_target(&fixture), 2);
    let red = mosaic.source_image_at(tile(0, 0)).unwrap();
    let blue = mosaic.source_image_at(tile(1, 0)).unwrap();

    std::fs::remove_file(fixture.source_library().join("red.png")).unwrap();
    fixture.add_source_image("blue.png", &solid(8, 8, Rgb([0, 0, 254])));

    assert_eq!(mosaic.preview(red, 100), None);
    assert_eq!(mosaic.preview(blue, 100), None);
}

#[test]
fn a_transparent_original_is_previewed_on_white() {
    let fixture = Fixture::new();
    // Transparent black around an opaque red center.
    let transparent = image::RgbaImage::from_fn(30, 30, |x, y| {
        if (10..20).contains(&x) && (10..20).contains(&y) {
            image::Rgba([255, 0, 0, 255])
        } else {
            image::Rgba([0, 0, 0, 0])
        }
    });
    transparent
        .save(fixture.source_library().join("transparent.png"))
        .unwrap();
    let mosaic = fixture.build(&red_left_blue_right_target(&fixture), 1);

    let preview = mosaic
        .preview(mosaic.source_image_at(tile(0, 0)).unwrap(), 100)
        .unwrap();

    assert_eq!(*preview.get_pixel(0, 0), Rgb([255, 255, 255]));
    assert_eq!(*preview.get_pixel(15, 15), RED);
}
