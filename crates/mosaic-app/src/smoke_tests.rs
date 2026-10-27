//! Smoke tests: they drive the window headlessly to check it is wired up, and save a
//! screenshot for PRs. What the window shows is computed and tested in `mosaic-core`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use image::{Rgb, RgbImage};
use mosaic_core::{AnalysisCache, Tile};
use tempfile::TempDir;

use super::{MosaicMakerApp, egui, egui_rect};

/// The Target Image is four stripes, each two Tiles wide, of these colors.
const STRIPES: [(&str, Rgb<u8>); 4] = [
    ("red.png", Rgb([220, 40, 40])),
    ("green.png", Rgb([40, 180, 70])),
    ("blue.png", Rgb([40, 90, 220])),
    ("yellow.png", Rgb([240, 200, 40])),
];
const TILES_ACROSS: u32 = 8;
const ROWS: u32 = 4;
const TILE_PIXELS: u32 = 10;

/// How long a build, a file or reading an Original may take before a test gives up.
const TIMEOUT: Duration = Duration::from_secs(60);

/// Where screenshots are saved, to attach to PRs: `screenshots` in the target folder.
fn screenshots_folder() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"),
        PathBuf::from,
    );
    target.join("screenshots")
}

/// A Source Library with one solid Source Image per stripe, and a striped Target Image.
struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let fixture = Fixture {
            dir: TempDir::new().unwrap(),
        };
        std::fs::create_dir(fixture.source_library()).unwrap();
        for (file_name, color) in STRIPES {
            RgbImage::from_pixel(64, 64, color)
                .save(fixture.source_library().join(file_name))
                .unwrap();
        }
        let stripe_pixels = TILES_ACROSS / STRIPES.len() as u32 * TILE_PIXELS;
        let target_image =
            RgbImage::from_fn(TILES_ACROSS * TILE_PIXELS, ROWS * TILE_PIXELS, |x, _| {
                STRIPES[(x / stripe_pixels) as usize].1
            });
        target_image.save(fixture.target_image()).unwrap();
        fixture
    }

    fn target_image(&self) -> PathBuf {
        self.dir.path().join("target.png")
    }

    fn source_library(&self) -> PathBuf {
        self.dir.path().join("library")
    }

    fn path(&self, file_name: &str) -> PathBuf {
        self.dir.path().join(file_name)
    }

    /// The window with the Target Image and Source Library chosen, built with Generate.
    fn built_app(&self) -> Harness<'static, MosaicMakerApp> {
        let app = MosaicMakerApp {
            target_image: Some(self.target_image()),
            source_library: Some(self.source_library()),
            tiles_across: TILES_ACROSS,
            // Each stripe is filled by its own Source Image throughout.
            repeat_radius: 0,
            seed: 0,
            cache: AnalysisCache::in_folder(self.dir.path().join("cache")),
            ..MosaicMakerApp::default()
        };
        let mut harness = window(app);
        harness.get_by_label("Generate").click();
        run_until(&mut harness, "the build", |harness| {
            harness.query_by_label_contains("Built from").is_some()
        });
        harness
    }
}

fn window(app: MosaicMakerApp) -> Harness<'static, MosaicMakerApp> {
    Harness::builder()
        .with_size(egui::vec2(1100.0, 600.0))
        .with_pixels_per_point(2.0)
        .build_eframe(|_| app)
}

/// Steps the window in real time until `done`, since builds, files and Originals are read
/// on other threads.
fn run_until(
    harness: &mut Harness<'_, MosaicMakerApp>,
    what: &str,
    done: impl Fn(&Harness<'_, MosaicMakerApp>) -> bool,
) {
    let deadline = Instant::now() + TIMEOUT;
    while !done(harness) {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
        harness.step();
    }
}

/// Clicks the middle of `tile` on the canvas, then moves the pointer away.
fn click_tile(harness: &mut Harness<'_, MosaicMakerApp>, tile: Tile) {
    let viewport = harness
        .state()
        .shown
        .as_ref()
        .and_then(|shown| shown.viewport.as_ref())
        .expect("a built Mosaic is on the canvas");
    let pos = egui_rect(viewport.tile_rect(tile)).center();
    harness.hover_at(pos);
    harness.drag_at(pos);
    harness.drop_at(pos);
    harness.run();
}

fn inspector_is_open(harness: &Harness<'_, MosaicMakerApp>) -> bool {
    harness.query_by_label("Source Image").is_some()
}

#[test]
fn clicking_a_tile_pins_its_source_image_and_escape_unpins_it() {
    let fixture = Fixture::new();
    let mut harness = fixture.built_app();
    assert!(!inspector_is_open(&harness));

    click_tile(&mut harness, Tile { column: 0, row: 0 });
    assert!(inspector_is_open(&harness));
    harness.get_by_label("red.png");
    harness.get_by_label(&format!("Fills {} Tiles.", 2 * ROWS));

    harness.key_press(egui::Key::Escape);
    harness.run();
    assert!(!inspector_is_open(&harness));
    assert!(harness.query_by_label("red.png").is_none());
}

#[test]
fn a_screenshot_is_saved_with_a_tile_pinned() {
    let fixture = Fixture::new();
    let mut harness = fixture.built_app();
    click_tile(&mut harness, Tile { column: 4, row: 1 });
    run_until(&mut harness, "the Original", |harness| {
        harness.query_by_label("Loading the Original…").is_none()
    });

    save_screenshot(&mut harness, "tile-pinned.png");
}

fn save_screenshot(harness: &mut Harness<'_, MosaicMakerApp>, file_name: &str) {
    let screenshot = harness.render().expect("the window renders");
    std::fs::create_dir_all(screenshots_folder()).unwrap();
    let path = screenshots_folder().join(file_name);
    screenshot.save(&path).unwrap();
    println!("Saved {}", path.display());
}

/// Steps the window until its status line contains `status`.
fn run_until_status(harness: &mut Harness<'_, MosaicMakerApp>, status: &str) {
    run_until(harness, status, |harness| {
        harness.query_by_label_contains(status).is_some()
    });
}

#[test]
fn a_reopened_mosaic_whose_source_library_is_gone_shows_thumbnails_and_cant_reveal_them() {
    let fixture = Fixture::new();
    let mut harness = fixture.built_app();
    let ctx = harness.ctx.clone();
    harness
        .state_mut()
        .save_to(&ctx, fixture.path("stripes.mosaic"));
    run_until_status(&mut harness, "Saved to");
    std::fs::remove_dir_all(fixture.source_library()).unwrap();

    harness
        .state_mut()
        .open_from(&ctx, fixture.path("stripes.mosaic"));
    run_until_status(&mut harness, "Opened");
    click_tile(&mut harness, Tile { column: 4, row: 1 });
    run_until(&mut harness, "the Original", |harness| {
        harness.query_by_label("Loading the Original…").is_none()
    });

    harness.get_by_label("blue.png");
    harness.get_by_label("The Original is unreachable, so this is its Thumbnail.");
    let reveal = harness.get_by_label("Reveal in Finder");
    assert!(reveal.accesskit_node().is_disabled());
    harness.get_by_label_contains("There's nothing to reveal");
    save_screenshot(&mut harness, "reopened-original-unreachable.png");
}

#[test]
fn opening_a_corrupt_file_shows_why_it_failed() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("corrupt.mosaic");
    std::fs::write(&path, b"not a Mosaic").unwrap();
    let mut harness = window(MosaicMakerApp::default());
    let ctx = harness.ctx.clone();

    harness.state_mut().open_from(&ctx, path);

    run_until_status(&mut harness, "Couldn't open");
    harness.get_by_label_contains("isn't a Mosaic");
}
