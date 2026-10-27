mod common;

use common::{BLUE, Fixture, GREEN, RED, STRIPES, solid, striped_mosaic, tile};
use image::{Rgb, RgbImage};
use mosaic_core::{Mosaic, Tile};

/// Asserts that every pixel of `tile` in `exported`, at `tile_size` pixels per Tile, is `color`.
fn assert_tile_is(exported: &RgbImage, tile: Tile, tile_size: u32, color: Rgb<u8>) {
    for y in tile.row * tile_size..(tile.row + 1) * tile_size {
        for x in tile.column * tile_size..(tile.column + 1) * tile_size {
            assert_eq!(
                *exported.get_pixel(x, y),
                color,
                "{tile:?}, pixel ({x}, {y})"
            );
        }
    }
}

#[test]
fn an_exported_png_has_the_chosen_size_per_tile() {
    let fixture = Fixture::new();
    let mosaic = striped_mosaic(&fixture);
    let path = fixture.path("striped.png");

    let report = mosaic.export_png(&path, 64).unwrap();

    let exported = image::open(&path).unwrap().into_rgb8();
    assert_eq!(exported.dimensions(), (3 * 64, 2 * 64));
    for (row, colors) in (0..).zip(STRIPES) {
        for (column, color) in (0..).zip(colors) {
            assert_tile_is(&exported, tile(column, row), 64, color);
        }
    }
    assert_eq!(report.tiles_from_thumbnails, []);
}

/// 512 px square, in alternating 1 px columns of black and white. Scaled down to a Thumbnail,
/// it blurs to gray.
fn fine_stripes() -> RgbImage {
    RgbImage::from_fn(512, 512, |x, _| Rgb([if x % 2 == 0 { 0 } else { 255 }; 3]))
}

#[test]
fn an_export_draws_tiles_from_their_originals_where_reachable() {
    let fixture = Fixture::new();
    fixture.add_source_image("stripes.png", &fine_stripes());
    let target_image = fixture.set_target_image(&solid(10, 10, Rgb([128; 3])));
    let mosaic = fixture.build(&target_image, 1);
    let path = fixture.path("stripes.png");

    let report = mosaic.export_png(&path, 512).unwrap();

    assert_eq!(image::open(&path).unwrap().into_rgb8(), fine_stripes());
    assert_eq!(report.tiles_from_thumbnails, []);
}

#[test]
fn an_export_draws_tiles_from_thumbnails_where_originals_are_unreachable_and_reports_them() {
    let fixture = Fixture::new();
    let mosaic = striped_mosaic(&fixture);
    std::fs::remove_file(fixture.source_library().join("red.png")).unwrap();
    // Edited since the build, so no longer the same Source Image.
    fixture.add_source_image("colors/blue.png", &solid(8, 8, Rgb([0, 0, 254])));
    let path = fixture.path("striped.png");

    let report = mosaic.export_png(&path, 64).unwrap();

    assert_eq!(
        report.tiles_from_thumbnails,
        [tile(0, 0), tile(1, 0), tile(2, 0), tile(1, 1), tile(2, 1)]
    );
    let exported = image::open(&path).unwrap().into_rgb8();
    assert_tile_is(&exported, tile(0, 0), 64, RED);
    assert_tile_is(&exported, tile(1, 0), 64, BLUE);
    assert_tile_is(&exported, tile(0, 1), 64, GREEN);
}

#[test]
fn a_reopened_mosaic_exports_from_its_thumbnails_once_its_source_library_is_deleted() {
    let fixture = Fixture::new();
    let saved = fixture.path("striped.mosaic");
    striped_mosaic(&fixture).save(&saved).unwrap();
    std::fs::remove_dir_all(fixture.source_library()).unwrap();
    let mosaic = Mosaic::open(&saved).unwrap();
    let path = fixture.path("striped.png");

    let report = mosaic.export_png(&path, 16).unwrap();

    assert_eq!(report.tiles_from_thumbnails.len(), 6);
    let exported = image::open(&path).unwrap().into_rgb8();
    assert_eq!(exported.dimensions(), (3 * 16, 2 * 16));
    assert_tile_is(&exported, tile(1, 1), 16, RED);
}
