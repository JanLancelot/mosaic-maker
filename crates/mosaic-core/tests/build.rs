use std::path::Path;

use image::{Rgb, RgbImage};
use mosaic_core::{
    BuildError, BuildSettings, CancelToken, Mosaic, Progress, RENDERED_TILE_SIZE, Stage, Tile,
    build,
};
use tempfile::TempDir;

const RED: Rgb<u8> = Rgb([255, 0, 0]);
const GREEN: Rgb<u8> = Rgb([0, 255, 0]);
const BLUE: Rgb<u8> = Rgb([0, 0, 255]);

struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("library")).unwrap();
        Fixture { dir }
    }

    fn source_library(&self) -> std::path::PathBuf {
        self.dir.path().join("library")
    }

    fn add_source_image(&self, file_name: &str, image: &RgbImage) {
        image.save(self.source_library().join(file_name)).unwrap();
    }

    fn set_target_image(&self, image: &RgbImage) -> std::path::PathBuf {
        let path = self.dir.path().join("target.png");
        image.save(&path).unwrap();
        path
    }

    fn build(&self, target_image: &Path, tiles_across: u32) -> Mosaic {
        self.build_with(target_image, tiles_across, |_| {}, &CancelToken::new())
            .unwrap()
    }

    fn build_with(
        &self,
        target_image: &Path,
        tiles_across: u32,
        on_progress: impl FnMut(Progress),
        cancel: &CancelToken,
    ) -> Result<Mosaic, BuildError> {
        build(
            target_image,
            &self.source_library(),
            &BuildSettings { tiles_across },
            on_progress,
            cancel,
        )
    }
}

fn solid(width: u32, height: u32, color: Rgb<u8>) -> RgbImage {
    RgbImage::from_pixel(width, height, color)
}

fn source_file_name(mosaic: &Mosaic, tile: Tile) -> String {
    let source_image = mosaic.source_image_at(tile).unwrap();
    source_image
        .path()
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
