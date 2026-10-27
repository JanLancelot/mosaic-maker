//! Smoke tests: they drive the window headlessly to check it is wired up, and save a
//! screenshot for PRs. What the window shows is computed and tested in `mosaic-core`.
//!
//! Open a collapsing section with `click_accesskit()`: a pointer `click()` doesn't open one
//! here.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use mosaic_core::{AnalysisCache, BuildSettings, Tile};
use tempfile::TempDir;

use super::{MosaicMakerApp, egui, egui_rect, max_texture_side};
use crate::session::{Session, Setup};
use crate::test_fixture::{Fixture, ROWS, TILES_ACROSS};

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

impl Fixture {
    /// The window with the Target Image and Source Library chosen, built with Generate.
    fn built_app(&self) -> Harness<'static, MosaicMakerApp> {
        let mut harness = window(self.cache(), self.setup());
        harness.get_by_label("Generate").click();
        // Otherwise screenshots show Generate hovered.
        harness.remove_cursor();
        run_until(&mut harness, "the build", |harness| {
            harness.query_by_label_contains("Built from").is_some()
        });
        harness
    }
}

fn window(cache: AnalysisCache, setup: Setup) -> Harness<'static, MosaicMakerApp> {
    Harness::builder()
        .with_size(egui::vec2(1100.0, 600.0))
        .with_pixels_per_point(2.0)
        .build_eframe(|cc| MosaicMakerApp::new(&cc.egui_ctx, cache, setup))
}

/// The window's Session, to start what its File menu would through file dialogs.
fn session<'a>(harness: &'a mut Harness<'_, MosaicMakerApp>) -> &'a mut Session {
    &mut harness.state_mut().session
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
    session(&mut harness)
        .save(fixture.path("stripes.mosaic"))
        .unwrap();
    run_until_status(&mut harness, "Saved to");
    std::fs::remove_dir_all(fixture.source_library()).unwrap();

    let max_texture_side = max_texture_side(&harness.ctx);
    session(&mut harness)
        .open(fixture.path("stripes.mosaic"), max_texture_side)
        .unwrap();
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
    let mut harness = window(
        AnalysisCache::in_folder(dir.path().join("cache")),
        nothing_chosen(),
    );
    let max_texture_side = max_texture_side(&harness.ctx);

    session(&mut harness).open(path, max_texture_side).unwrap();

    run_until_status(&mut harness, "Couldn't open");
    harness.get_by_label_contains("isn't a Mosaic");
}

#[test]
fn an_export_without_its_source_library_lists_the_tiles_drawn_from_thumbnails() {
    let fixture = Fixture::new();
    let mut harness = fixture.built_app();
    std::fs::remove_dir_all(fixture.source_library()).unwrap();
    let tile_size = harness.state().export_tile_size;

    session(&mut harness)
        .export(fixture.path("stripes.png"), tile_size)
        .unwrap();
    run_until_status(&mut harness, "Exported to");

    let tile_count = TILES_ACROSS * ROWS;
    harness
        .get_by_label(&format!("{tile_count} Tiles drawn from Thumbnails"))
        .click_accesskit();
    harness.run();
    harness.get_by_label("Column 0, row 0: red.png");
    save_screenshot(&mut harness, "export-tiles-from-thumbnails.png");
}

/// No Target Image or Source Library chosen yet, as when the app starts.
fn nothing_chosen() -> Setup {
    Setup {
        target_image: None,
        source_library: None,
        settings: BuildSettings {
            tiles_across: TILES_ACROSS,
            repeat_radius: 0,
            seed: 0,
        },
    }
}
