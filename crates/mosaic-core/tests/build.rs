mod common;

use std::path::Path;

use common::{BLUE, Fixture, GREEN, RED, solid, tiles};
use image::{Rgb, RgbImage, Rgba, RgbaImage};
use mosaic_core::{
    AnalysisCache, BuildError, BuildSettings, BuildWarning, Built, CancelToken,
    DEFAULT_REPEAT_RADIUS, Mosaic, RENDERED_TILE_SIZE, SkipReason, SourceImageId, Stage, Tile,
    build,
};

fn source_file_name(mosaic: &Mosaic, tile: Tile) -> String {
    let source_image = mosaic.source_image_at(tile).unwrap();
    source_image
        .last_known_path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[test]
fn solid_red_target_region_receives_the_solid_red_source_image() {
    let fixture = Fixture::new();
    fixture.add_source_image("blue.png", &solid(8, 8, BLUE));
    fixture.add_source_image("green.png", &solid(8, 8, GREEN));
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    let target = RgbImage::from_fn(20, 10, |x, _| if x < 10 { RED } else { BLUE });
    let target_image = fixture.set_target_image(&target);

    let mosaic = fixture.build(&target_image, 2);

    assert_eq!(
        source_file_name(&mosaic, Tile { column: 0, row: 0 }),
        "red.png"
    );
    assert_eq!(
        source_file_name(&mosaic, Tile { column: 1, row: 0 }),
        "blue.png"
    );
    let rendered = mosaic.rendered();
    for y in 0..RENDERED_TILE_SIZE {
        for x in 0..RENDERED_TILE_SIZE {
            assert_eq!(*rendered.get_pixel(x, y), RED, "rendered pixel ({x}, {y})");
        }
    }
}

/// A horizontal ramp from gray level `left` to gray level `right`.
fn horizontal_gradient(width: u32, height: u32, left: u8, right: u8) -> RgbImage {
    RgbImage::from_fn(width, height, |x, _| {
        let t = f64::from(x) / f64::from(width - 1);
        let level = (f64::from(left) + t * (f64::from(right) - f64::from(left))).round() as u8;
        Rgb([level; 3])
    })
}

#[test]
fn left_dark_right_bright_tile_prefers_the_matching_gradient_over_the_same_average_color() {
    let fixture = Fixture::new();
    // Sorted first, so a matcher that only compares average colors would pick it.
    fixture.add_source_image("average_gray.png", &solid(30, 30, Rgb([143; 3])));
    fixture.add_source_image("gradient.png", &horizontal_gradient(30, 30, 0, 255));
    let target_image = fixture.set_target_image(&horizontal_gradient(30, 30, 40, 220));

    let mosaic = fixture.build(&target_image, 1);

    assert_eq!(
        source_file_name(&mosaic, Tile { column: 0, row: 0 }),
        "gradient.png"
    );
}

#[test]
fn rows_follow_the_target_image_aspect_ratio_and_the_leftover_edge_is_cropped() {
    let fixture = Fixture::new();
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    // 4 Tiles Across makes each Tile 25 px, so 70 px holds 2.8 rows: 2 whole rows fit.
    let target_image = fixture.set_target_image(&solid(100, 70, RED));

    let mosaic = fixture.build(&target_image, 4);

    assert_eq!((mosaic.tiles_across(), mosaic.rows()), (4, 2));
    assert!(mosaic.source_image_at(Tile { column: 3, row: 1 }).is_some());
    assert!(mosaic.source_image_at(Tile { column: 0, row: 2 }).is_none());
    assert!(mosaic.source_image_at(Tile { column: 4, row: 0 }).is_none());
    assert_eq!(
        mosaic.rendered().dimensions(),
        (4 * RENDERED_TILE_SIZE, 2 * RENDERED_TILE_SIZE)
    );
}

#[test]
fn source_images_are_center_cropped_to_square_before_matching() {
    let fixture = Fixture::new();
    let dark_red = Rgb([180, 0, 0]);
    // Red in its central square, with green bands either side that the crop removes.
    let banded = RgbImage::from_fn(
        30,
        10,
        |x, _| if (10..20).contains(&x) { RED } else { GREEN },
    );
    fixture.add_source_image("banded.png", &banded);
    fixture.add_source_image("dark_red.png", &solid(10, 10, dark_red));
    let target_image = fixture.set_target_image(&solid(10, 10, RED));

    let mosaic = fixture.build(&target_image, 1);

    assert_eq!(
        source_file_name(&mosaic, Tile { column: 0, row: 0 }),
        "banded.png"
    );
    assert!(mosaic.rendered().pixels().all(|pixel| *pixel == RED));
}

#[test]
fn source_images_in_nested_subfolders_are_found_and_located_relative_to_the_source_library() {
    let fixture = Fixture::new();
    fixture.add_source_image("blue.png", &solid(8, 8, BLUE));
    fixture.add_source_image("artists/someone/red.png", &solid(8, 8, RED));
    let target_image = fixture.set_target_image(&solid(10, 10, RED));

    let mosaic = fixture.build(&target_image, 1);

    let source_image = mosaic.source_image_at(Tile { column: 0, row: 0 }).unwrap();
    assert_eq!(
        source_image.last_known_path(),
        Path::new("artists/someone/red.png")
    );
}

/// A 30 × 30 picture with a red 10 × 10 center, surrounded by `around`.
fn red_center(around: Rgb<u8>) -> RgbImage {
    RgbImage::from_fn(30, 30, |x, y| {
        if (10..20).contains(&x) && (10..20).contains(&y) {
            RED
        } else {
            around
        }
    })
}

#[test]
fn a_source_image_transparent_apart_from_a_red_center_is_shown_as_red_on_white() {
    let fixture = Fixture::new();
    // Transparent black: read without its transparency, this would look like the decoy.
    let transparent = RgbaImage::from_fn(30, 30, |x, y| {
        let Rgb([r, g, b]) = *red_center(Rgb([0, 0, 0])).get_pixel(x, y);
        let alpha = if (10..20).contains(&x) && (10..20).contains(&y) {
            255
        } else {
            0
        };
        Rgba([r, g, b, alpha])
    });
    transparent
        .save(fixture.source_library().join("transparent.png"))
        .unwrap();
    // Sorted first, so it would win a tie.
    fixture.add_source_image("black_around_red.png", &red_center(Rgb([0, 0, 0])));
    let target_image = fixture.set_target_image(&red_center(Rgb([255, 255, 255])));

    let mosaic = fixture.build(&target_image, 1);

    assert_eq!(
        source_file_name(&mosaic, Tile { column: 0, row: 0 }),
        "transparent.png"
    );
    let rendered = mosaic.rendered();
    let middle = RENDERED_TILE_SIZE / 2;
    assert_eq!(*rendered.get_pixel(0, 0), Rgb([255, 255, 255]));
    assert_eq!(*rendered.get_pixel(middle, middle), RED);
}

/// The opening of a HEIC file: an ISO media `ftyp` box with the `heic` brand.
const HEIC_HEADER: &[u8] = b"\0\0\0\x18ftypheic\0\0\0\0mif1heic";

fn png_bytes(image: &RgbImage) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

#[test]
fn heic_non_image_and_corrupt_files_are_skipped_each_with_a_reason() {
    let fixture = Fixture::new();
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    fixture.add_file("phone/holiday.heic", HEIC_HEADER);
    fixture.add_file("notes.txt", b"not a picture");
    let png = png_bytes(&solid(64, 64, BLUE));
    fixture.add_file("broken.png", &png[..png.len() / 2]);
    let target_image = fixture.set_target_image(&solid(10, 10, RED));

    let built = fixture.build_reporting(&target_image, 1);

    let mut skipped: Vec<_> = built
        .report
        .warnings
        .iter()
        .map(|warning| match warning {
            BuildWarning::SkippedFile { path, reason } => (path.to_str().unwrap(), reason),
            other => panic!("unexpected warning {other:?}"),
        })
        .collect();
    skipped.sort_by_key(|(path, _)| *path);
    assert!(
        matches!(
            skipped[..],
            [
                ("broken.png", SkipReason::Corrupt(_)),
                ("notes.txt", SkipReason::NotAnImage),
                ("phone/holiday.heic", SkipReason::Heic),
            ]
        ),
        "{skipped:?}"
    );
    assert_eq!(built.report.stats.source_images, 1);
}

#[cfg(unix)]
#[test]
fn a_linked_folder_is_skipped_with_a_reason_rather_than_followed() {
    let fixture = Fixture::new();
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    // Linking back to the Source Library itself would loop forever if followed.
    std::os::unix::fs::symlink(
        fixture.source_library(),
        fixture.source_library().join("loop"),
    )
    .unwrap();
    let target_image = fixture.set_target_image(&solid(10, 10, RED));

    let built = fixture.build_reporting(&target_image, 1);

    assert!(
        matches!(
            &built.report.warnings[..],
            [BuildWarning::SkippedFile {
                path,
                reason: SkipReason::LinkedFolder,
            }] if path == Path::new("loop")
        ),
        "{:?}",
        built.report.warnings
    );
}

#[test]
fn hidden_files_are_left_out_without_a_warning() {
    let fixture = Fixture::new();
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    fixture.add_file(".DS_Store", b"Finder's folder settings");
    fixture.add_source_image(".thumbnails/blue.png", &solid(8, 8, BLUE));
    let target_image = fixture.set_target_image(&solid(10, 10, BLUE));

    let built = fixture.build_reporting(&target_image, 1);

    assert_eq!(built.report.warnings, []);
    assert_eq!(built.report.stats.source_images, 1);
}

const TOP_LEFT: Tile = Tile { column: 0, row: 0 };

#[test]
fn byte_identical_files_at_different_paths_are_one_source_image() {
    let fixture = Fixture::new();
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    fixture.add_source_image("copies/red.png", &solid(8, 8, RED));
    let target_image = fixture.set_target_image(&solid(10, 10, RED));

    let built = fixture.build_reporting(&target_image, 1);

    assert_eq!(built.report.stats.source_images, 1);
}

#[test]
fn a_renamed_file_keeps_its_identity() {
    let fixture = Fixture::new();
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    let target_image = fixture.set_target_image(&solid(10, 10, RED));
    let before = fixture.build(&target_image, 1);

    std::fs::create_dir(fixture.source_library().join("moved")).unwrap();
    std::fs::rename(
        fixture.source_library().join("red.png"),
        fixture.source_library().join("moved/renamed.png"),
    )
    .unwrap();
    let after = fixture.build(&target_image, 1);

    let (before, after) = (
        before.source_image_at(TOP_LEFT).unwrap(),
        after.source_image_at(TOP_LEFT).unwrap(),
    );
    assert_eq!(before.id(), after.id());
    assert_eq!(after.last_known_path(), Path::new("moved/renamed.png"));
}

#[test]
fn an_edited_file_gets_a_new_identity() {
    let fixture = Fixture::new();
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    let target_image = fixture.set_target_image(&solid(10, 10, RED));
    let before = fixture.build(&target_image, 1);

    let mut edited = solid(8, 8, RED);
    edited.put_pixel(0, 0, Rgb([254, 0, 0]));
    fixture.add_source_image("red.png", &edited);
    let after = fixture.build(&target_image, 1);

    assert_ne!(
        before.source_image_at(TOP_LEFT).unwrap().id(),
        after.source_image_at(TOP_LEFT).unwrap().id()
    );
}

/// Every file under `folder` with its contents, so any write inside it shows up.
fn folder_contents(folder: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut files = Vec::new();
    let mut folders = vec![folder.to_owned()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                files.push((path.clone(), std::fs::read(path).unwrap()));
            }
        }
    }
    files.sort();
    files
}

#[test]
fn a_second_build_reuses_the_cached_analysis_and_builds_the_same_mosaic() {
    let fixture = Fixture::new();
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    fixture.add_source_image("nested/blue.png", &solid(8, 8, BLUE));
    let target = RgbImage::from_fn(20, 10, |x, _| if x < 10 { RED } else { BLUE });
    let target_image = fixture.set_target_image(&target);

    let first = fixture.build_reporting(&target_image, 2);
    let second = fixture.build_reporting(&target_image, 2);

    assert_eq!(first.report.stats.cache_hits, 0);
    assert_eq!(second.report.stats.cache_hits, 2);
    assert_eq!(second.mosaic.rendered(), first.mosaic.rendered());
}

#[test]
fn a_build_writes_nothing_inside_the_source_library() {
    let fixture = Fixture::new();
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    fixture.add_source_image("nested/blue.png", &solid(8, 8, BLUE));
    fixture.add_file("notes.txt", b"not a picture");
    let target_image = fixture.set_target_image(&solid(10, 10, RED));
    let before = folder_contents(&fixture.source_library());

    fixture.build(&target_image, 1);
    fixture.build(&target_image, 1);

    assert_eq!(folder_contents(&fixture.source_library()), before);
}

#[test]
fn a_cache_folder_inside_the_source_library_is_never_written() {
    let fixture = Fixture::new();
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    let target_image = fixture.set_target_image(&solid(10, 10, RED));
    let before = folder_contents(&fixture.source_library());
    let settings = BuildSettings {
        tiles_across: 1,
        repeat_radius: DEFAULT_REPEAT_RADIUS,
        seed: 0,
    };
    let cache = AnalysisCache::in_folder(fixture.source_library().join("nested/cache"));

    for _ in 0..2 {
        build(
            &target_image,
            &fixture.source_library(),
            &settings,
            &cache,
            |_| {},
            &CancelToken::new(),
        )
        .unwrap();
    }

    assert_eq!(folder_contents(&fixture.source_library()), before);
}

#[test]
fn a_corrupt_cache_entry_is_recomputed_rather_than_failing_the_build() {
    let fixture = Fixture::new();
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    fixture.add_source_image("blue.png", &solid(8, 8, BLUE));
    let target = RgbImage::from_fn(20, 10, |x, _| if x < 10 { RED } else { BLUE });
    let target_image = fixture.set_target_image(&target);
    let first = fixture.build_reporting(&target_image, 2);

    let cached = folder_contents(&fixture.cache_folder());
    assert!(!cached.is_empty(), "the first build cached nothing");
    for (path, contents) in cached {
        std::fs::write(&path, &contents[..contents.len() / 2]).unwrap();
    }
    let second = fixture.build_reporting(&target_image, 2);
    let third = fixture.build_reporting(&target_image, 2);

    assert_eq!(second.report.stats.cache_hits, 0);
    assert_eq!(second.mosaic.rendered(), first.mosaic.rendered());
    assert_eq!(third.report.stats.cache_hits, 2);
}

/// Adds `count` slightly different shades of red to the Source Library.
fn add_reds(fixture: &Fixture, count: u8) {
    for i in 0..count {
        fixture.add_source_image(
            &format!("red_{i:02}.png"),
            &solid(4, 4, Rgb([255 - i, i, 0])),
        );
    }
}

/// How many Tiles apart `a` and `b` are, counting diagonal steps as one.
fn chebyshev_distance(a: Tile, b: Tile) -> u32 {
    a.column.abs_diff(b.column).max(a.row.abs_diff(b.row))
}

/// Pairs of distinct Tiles within `radius` of each other that the same Source Image fills.
fn repeats_within(mosaic: &Mosaic, radius: u32) -> Vec<(Tile, Tile)> {
    let tiles = tiles(mosaic);
    let id = |tile| mosaic.source_image_at(tile).unwrap().id();
    let mut repeats = Vec::new();
    for (i, &a) in tiles.iter().enumerate() {
        for &b in &tiles[i + 1..] {
            if chebyshev_distance(a, b) <= radius && id(a) == id(b) {
                repeats.push((a, b));
            }
        }
    }
    repeats
}

#[test]
fn with_a_big_enough_source_library_no_source_image_repeats_within_the_repeat_radius() {
    let fixture = Fixture::new();
    // Each Tile has at most 24 others within a radius of 2, so 25 Source Images always suffice.
    add_reds(&fixture, 25);
    let target_image = fixture.set_target_image(&solid(60, 60, RED));
    let settings = BuildSettings {
        tiles_across: 6,
        repeat_radius: 2,
        seed: 1,
    };

    let built = fixture.build_with_settings(&target_image, &settings);

    assert_eq!(repeats_within(&built.mosaic, 2), []);
    assert_eq!(built.report.warnings, []);
}

/// The Repeat Radius each Tile was filled with, given the radius asked for.
fn repeat_radius_used(built: &Built, asked: u32, tile: Tile) -> u32 {
    built
        .report
        .warnings
        .iter()
        .find_map(|warning| match warning {
            BuildWarning::RepeatRadiusRelaxed {
                tile: relaxed,
                repeat_radius,
            } if *relaxed == tile => Some(*repeat_radius),
            _ => None,
        })
        .unwrap_or(asked)
}

#[test]
fn with_one_source_image_every_tile_but_the_first_filled_relaxes_the_repeat_radius_to_0() {
    let fixture = Fixture::new();
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    let target_image = fixture.set_target_image(&solid(20, 20, RED));
    let settings = BuildSettings {
        tiles_across: 2,
        repeat_radius: 1,
        seed: 0,
    };

    let built = fixture.build_with_settings(&target_image, &settings);

    let relaxed_to: Vec<u32> = built
        .report
        .warnings
        .iter()
        .map(|warning| match warning {
            BuildWarning::RepeatRadiusRelaxed { repeat_radius, .. } => *repeat_radius,
            other => panic!("unexpected warning {other:?}"),
        })
        .collect();
    assert_eq!(relaxed_to, [0, 0, 0]);
}

#[test]
fn with_a_too_small_source_library_the_repeat_radius_is_relaxed_only_where_needed() {
    let fixture = Fixture::new();
    add_reds(&fixture, 4);
    let target_image = fixture.set_target_image(&solid(50, 50, RED));
    let settings = BuildSettings {
        tiles_across: 5,
        repeat_radius: 3,
        seed: 0,
    };

    let built = fixture.build_with_settings(&target_image, &settings);

    let tile_count = tiles(&built.mosaic).len();
    let relaxed = built.report.warnings.len();
    assert!(
        relaxed > 0 && relaxed < tile_count,
        "{relaxed} Tiles relaxed"
    );
    // The radius shrinks a step at a time, so it isn't simply dropped wherever it is in the way.
    assert!(
        tiles(&built.mosaic)
            .into_iter()
            .any(|tile| (1..3).contains(&repeat_radius_used(&built, 3, tile))),
        "{:?}",
        built.report.warnings
    );
    // Of two Tiles sharing a Source Image, the one filled later must have been allowed to.
    for (a, b) in repeats_within(&built.mosaic, 3) {
        let distance = chebyshev_distance(a, b);
        let allowed = repeat_radius_used(&built, 3, a).min(repeat_radius_used(&built, 3, b));
        assert!(allowed < distance, "{a:?} and {b:?} repeat");
    }
}

/// The Source Image filling every Tile, row by row.
fn mapping(mosaic: &Mosaic) -> Vec<SourceImageId> {
    tiles(mosaic)
        .into_iter()
        .map(|tile| mosaic.source_image_at(tile).unwrap().id())
        .collect()
}

fn seeded(seed: u64) -> BuildSettings {
    BuildSettings {
        tiles_across: 8,
        repeat_radius: 1,
        seed,
    }
}

#[test]
fn the_same_seed_gives_an_identical_mapping() {
    let fixture = Fixture::new();
    add_reds(&fixture, 40);
    let target_image = fixture.set_target_image(&horizontal_gradient(80, 80, 0, 255));

    let first = fixture.build_with_settings(&target_image, &seeded(7));
    let second = fixture.build_with_settings(&target_image, &seeded(7));

    assert_eq!(mapping(&first.mosaic), mapping(&second.mosaic));
}

#[test]
fn different_seeds_give_different_mappings() {
    let fixture = Fixture::new();
    add_reds(&fixture, 12);
    let target_image = fixture.set_target_image(&solid(80, 80, RED));

    let mappings: Vec<_> = (1..=3)
        .map(|seed| {
            mapping(
                &fixture
                    .build_with_settings(&target_image, &seeded(seed))
                    .mosaic,
            )
        })
        .collect();

    assert_ne!(mappings[0], mappings[1]);
    assert_ne!(mappings[0], mappings[2]);
    assert_ne!(mappings[1], mappings[2]);
}

#[test]
fn build_reports_progress_through_each_stage_in_order_to_completion() {
    let fixture = Fixture::new();
    fixture.add_source_image("red.png", &solid(8, 8, RED));
    fixture.add_source_image("blue.png", &solid(8, 8, BLUE));
    let target_image = fixture.set_target_image(&solid(40, 40, RED));
    let mut reports = Vec::new();

    fixture
        .build_with(
            &target_image,
            4,
            |progress| reports.push(progress),
            &CancelToken::new(),
        )
        .unwrap();

    let mut stages: Vec<Stage> = reports.iter().map(|progress| progress.stage).collect();
    stages.dedup();
    assert_eq!(
        stages,
        [
            Stage::AnalysingSourceImages,
            Stage::MatchingTiles,
            Stage::Rendering
        ]
    );
    for stage in stages {
        let last = reports
            .iter()
            .rfind(|progress| progress.stage == stage)
            .unwrap();
        assert_eq!(last.done, last.total, "{last:?}");
    }
    assert!(
        reports
            .iter()
            .all(|progress| progress.done <= progress.total)
    );
}

#[test]
fn cancellation_stops_the_build() {
    let fixture = Fixture::new();
    for name in ["a.png", "b.png", "c.png"] {
        fixture.add_source_image(name, &solid(8, 8, RED));
    }
    let target_image = fixture.set_target_image(&solid(40, 40, RED));
    let cancel = CancelToken::new();
    let mut reports_after_cancelling = 0;

    let result = fixture.build_with(
        &target_image,
        4,
        |_| {
            if cancel.is_cancelled() {
                reports_after_cancelling += 1;
            }
            cancel.cancel();
        },
        &cancel,
    );

    assert!(matches!(result, Err(BuildError::Cancelled)), "{result:?}");
    assert_eq!(reports_after_cancelling, 0);
}
