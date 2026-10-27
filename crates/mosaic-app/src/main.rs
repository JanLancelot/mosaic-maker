//! Mosaic Maker's window: gathers input, runs a build off the UI thread, and shows the Mosaic
//! on a canvas to pan, zoom and inspect.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, RandomState};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use eframe::egui;
use image::RgbImage;
use image::imageops::{self, FilterType};
use mosaic_core::{
    AnalysisCache, BuildError, BuildReport, BuildSettings, BuildStats, Built, CancelToken,
    DEFAULT_REPEAT_RADIUS, Mosaic, Progress, RENDERED_TILE_SIZE, ScreenPoint, ScreenRect,
    SourceImage, SourceImageId, Stage, Tile, Viewport,
};

const DEFAULT_TILES_ACROSS: u32 = 60;

/// How much one point of scrolling zooms the canvas.
const SCROLL_ZOOM_SPEED: f32 = 0.005;

/// Tiles drawn larger than this, in physical pixels, are drawn from their Thumbnails, since the
/// rendered Mosaic (`RENDERED_TILE_SIZE` pixels per Tile) would look soft. Any smaller, and so
/// many Tiles are on screen that drawing each one would make panning stutter.
const DETAILED_TILE_PIXELS: f32 = 1.5 * RENDERED_TILE_SIZE as f32;

/// The most Thumbnails uploaded to the GPU in one frame, so zooming in never stalls a frame.
const THUMBNAIL_UPLOADS_PER_FRAME: usize = 48;

/// Beyond this many Thumbnail textures, those not drawn in the latest frame are dropped.
const MAX_THUMBNAIL_TEXTURES: usize = 1024;

/// The longest side, in pixels, the inspector shows an Original at.
const ORIGINAL_SIZE: u32 = 1024;

/// The whole of a texture.
const FULL_UV: egui::Rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));

/// Images are painted with this tint, which leaves their pixels as they are (ADR 0002).
const UNTINTED: egui::Color32 = egui::Color32::WHITE;

#[cfg(test)]
mod smoke_tests;

fn main() -> eframe::Result {
    eframe::run_native(
        "Mosaic Maker",
        eframe::NativeOptions::default(),
        Box::new(|_| Ok(Box::new(MosaicMakerApp::default()))),
    )
}

struct MosaicMakerApp {
    target_image: Option<PathBuf>,
    source_library: Option<PathBuf>,
    tiles_across: u32,
    repeat_radius: u32,
    seed: u64,
    cache: AnalysisCache,
    running_build: Option<RunningBuild>,
    shown: Option<ShownMosaic>,
    status: Option<String>,
    /// What the last finished build worked around.
    report: Option<BuildReport>,
}

impl Default for MosaicMakerApp {
    fn default() -> Self {
        Self {
            target_image: None,
            source_library: None,
            tiles_across: DEFAULT_TILES_ACROSS,
            repeat_radius: DEFAULT_REPEAT_RADIUS,
            seed: random_seed(),
            cache: AnalysisCache::in_user_cache_folder(),
            running_build: None,
            shown: None,
            status: None,
            report: None,
        }
    }
}

struct RunningBuild {
    cancel: CancelToken,
    events: Receiver<BuildEvent>,
    progress: Option<Progress>,
}

enum BuildEvent {
    Progress(Progress),
    /// The finished build, with its rendered Mosaic ready to upload as a texture.
    Finished(Result<(Built, egui::ColorImage), BuildError>),
}

/// How a build ended, as far as the window is concerned.
enum Outcome {
    Built(Built, egui::ColorImage),
    Cancelled,
    Failed(String),
}

/// The Mosaic on the canvas, and how it is being inspected.
struct ShownMosaic {
    mosaic: Arc<Mosaic>,
    rendered: egui::TextureHandle,
    /// Fitted to the canvas the first time it is drawn.
    viewport: Option<Viewport>,
    thumbnails: ThumbnailTextures,
    pinned: Option<Pinned>,
}

/// The Source Image the inspector is pinned to.
struct Pinned {
    /// The Tile that was clicked.
    tile: Tile,
    /// Every Tile the Source Image fills, all highlighted.
    tiles: Vec<Tile>,
    original: Original,
    reveal_error: Option<String>,
}

/// The pinned Source Image's Original, as the inspector shows it.
enum Original {
    /// Being read on another thread; the Thumbnail stands in meanwhile.
    Loading(Receiver<Option<egui::ColorImage>>),
    Shown(egui::TextureHandle),
    /// The Thumbnail stands in.
    Unreachable,
}

/// Thumbnails uploaded as textures for drawing Tiles up close, a few per frame.
#[derive(Default)]
struct ThumbnailTextures {
    textures: HashMap<SourceImageId, egui::TextureHandle>,
    uploads_left: usize,
    /// Whether a Thumbnail wanted this frame had to wait for a later one.
    waiting: bool,
    drawn: HashSet<SourceImageId>,
}

impl eframe::App for MosaicMakerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.receive_build_events(ui.ctx());
        egui::Panel::left("setup").show(ui, |ui| self.setup_panel(ui));
        if let Some(shown) = &mut self.shown
            && shown.pinned.is_some()
        {
            egui::Panel::right("inspector")
                .default_size(320.0)
                .show(ui, |ui| shown.inspector(ui));
        }
        egui::CentralPanel::default_margins().show(ui, |ui| self.mosaic_panel(ui));
    }
}

impl MosaicMakerApp {
    fn setup_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mosaic Maker");
        ui.add_space(8.0);
        let idle = self.running_build.is_none();

        ui.label("Target Image");
        if ui
            .add_enabled(idle, egui::Button::new("Choose Target Image…"))
            .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Images", &["jpg", "jpeg", "png", "webp"])
                .pick_file()
        {
            self.target_image = Some(path);
        }
        ui.label(display_path(self.target_image.as_deref()));
        ui.add_space(8.0);

        ui.label("Source Library");
        if ui
            .add_enabled(idle, egui::Button::new("Choose Source Library…"))
            .clicked()
            && let Some(path) = rfd::FileDialog::new().pick_folder()
        {
            self.source_library = Some(path);
        }
        ui.label(display_path(self.source_library.as_deref()));
        ui.add_space(8.0);

        let label = ui.label("Tiles Across");
        ui.add_enabled(
            idle,
            egui::DragValue::new(&mut self.tiles_across).range(1..=300),
        )
        .labelled_by(label.id);
        ui.add_space(8.0);

        let label = ui.label("Repeat Radius");
        ui.add_enabled(
            idle,
            egui::DragValue::new(&mut self.repeat_radius).range(0..=20),
        )
        .labelled_by(label.id);
        ui.add_space(8.0);

        let label = ui.label("Seed");
        ui.horizontal(|ui| {
            ui.add_enabled(idle, egui::DragValue::new(&mut self.seed))
                .labelled_by(label.id);
            if ui
                .add_enabled(idle, egui::Button::new("Randomise"))
                .clicked()
            {
                self.seed = random_seed();
            }
        });
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let ready = self.target_image.is_some() && self.source_library.is_some();
            if ui
                .add_enabled(idle && ready, egui::Button::new("Generate"))
                .clicked()
            {
                self.start_build(ui.ctx());
            }
            if let Some(running_build) = &self.running_build
                && ui.button("Cancel").clicked()
            {
                running_build.cancel.cancel();
            }
        });

        if let Some(running_build) = &self.running_build {
            ui.add_space(8.0);
            ui.add(progress_bar(running_build.progress));
        }
        if let Some(status) = &self.status {
            ui.add_space(8.0);
            ui.label(status);
        }
        if let Some(report) = &self.report {
            ui.add_space(8.0);
            show_build_report(ui, report);
        }
    }

    fn mosaic_panel(&mut self, ui: &mut egui::Ui) {
        match &mut self.shown {
            Some(shown) => shown.canvas(ui),
            None => {
                ui.centered_and_justified(|ui| {
                    ui.label("Choose a Target Image and a Source Library, then Generate.");
                });
            }
        }
    }

    fn start_build(&mut self, ctx: &egui::Context) {
        let (Some(target_image), Some(source_library)) =
            (self.target_image.clone(), self.source_library.clone())
        else {
            return;
        };
        let settings = BuildSettings {
            tiles_across: self.tiles_across,
            repeat_radius: self.repeat_radius,
            seed: self.seed,
        };
        let cache = self.cache.clone();
        let cancel = CancelToken::new();
        let (sender, events) = channel();
        let max_texture_side = ctx.input(|input| input.max_texture_side) as u32;
        let ctx = ctx.clone();
        let build_cancel = cancel.clone();
        std::thread::spawn(move || {
            let result = mosaic_core::build(
                &target_image,
                &source_library,
                &settings,
                &cache,
                |progress| {
                    // The receiver only goes away when the app closes.
                    let _ = sender.send(BuildEvent::Progress(progress));
                    ctx.request_repaint();
                },
                &build_cancel,
            );
            let finished = result.map(|built| {
                let image = displayable_image(&built.mosaic, max_texture_side);
                (built, image)
            });
            let _ = sender.send(BuildEvent::Finished(finished));
            ctx.request_repaint();
        });
        self.running_build = Some(RunningBuild {
            cancel,
            events,
            progress: None,
        });
        self.status = None;
        self.report = None;
    }

    fn receive_build_events(&mut self, ctx: &egui::Context) {
        let Some(running_build) = &mut self.running_build else {
            return;
        };
        let finished = loop {
            match running_build.events.try_recv() {
                Ok(BuildEvent::Progress(progress)) => running_build.progress = Some(progress),
                Ok(BuildEvent::Finished(Ok((built, image)))) => {
                    break Some(Outcome::Built(built, image));
                }
                Ok(BuildEvent::Finished(Err(BuildError::Cancelled))) => {
                    break Some(Outcome::Cancelled);
                }
                Ok(BuildEvent::Finished(Err(error))) => {
                    break Some(Outcome::Failed(error.to_string()));
                }
                Err(TryRecvError::Empty) => break None,
                // The build thread panicked before sending its result.
                Err(TryRecvError::Disconnected) => {
                    break Some(Outcome::Failed("the build stopped unexpectedly".to_owned()));
                }
            }
        };
        let Some(outcome) = finished else {
            return;
        };
        self.running_build = None;
        match outcome {
            Outcome::Built(built, image) => {
                self.shown = Some(ShownMosaic {
                    mosaic: Arc::new(built.mosaic),
                    rendered: ctx.load_texture("mosaic", image, egui::TextureOptions::LINEAR),
                    viewport: None,
                    thumbnails: ThumbnailTextures::default(),
                    pinned: None,
                });
                self.status = None;
                self.report = Some(built.report);
            }
            Outcome::Cancelled => self.status = Some("Build cancelled.".to_owned()),
            Outcome::Failed(error) => {
                self.status = Some(format!("Build failed: {error}."));
            }
        }
    }
}

impl ShownMosaic {
    /// Draws the Mosaic, and pans, zooms, hovers and pins as the pointer asks.
    fn canvas(&mut self, ui: &mut egui::Ui) {
        let (response, painter) =
            ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
        let screen = screen_rect(response.rect);
        let viewport = self
            .viewport
            .get_or_insert_with(|| Viewport::fitting(&self.mosaic, screen));
        if response.dragged() {
            let delta = response.drag_delta();
            viewport.pan(delta.x, delta.y);
        }
        if let Some(pointer) = response.hover_pos() {
            let (pinch, scroll) =
                ui.input(|input| (input.zoom_delta(), input.smooth_scroll_delta().y));
            let factor = pinch * (scroll * SCROLL_ZOOM_SPEED).exp();
            if factor != 1.0 {
                viewport.zoom_about(screen_point(pointer), factor);
            }
        }
        let viewport = viewport.clone();
        let tile_under =
            |pos: Option<egui::Pos2>| pos.and_then(|pos| viewport.tile_at(screen_point(pos)));

        if response.clicked() {
            match tile_under(response.interact_pointer_pos()) {
                Some(tile) => self.pin(ui.ctx(), tile),
                None => self.pinned = None,
            }
        }
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.pinned = None;
        }

        let ctx = ui.ctx();
        self.thumbnails.begin_frame();
        painter.image(
            self.rendered.id(),
            egui_rect(viewport.mosaic_rect()),
            FULL_UV,
            UNTINTED,
        );
        // Up close, visible Tiles are drawn again, from their Thumbnails.
        if viewport.tile_side() * ctx.pixels_per_point() > DETAILED_TILE_PIXELS {
            for tile in viewport.visible_tiles(screen) {
                let source_image = source_image_at(&self.mosaic, tile);
                if let Some(texture) = self.thumbnails.texture_if_ready(ctx, source_image) {
                    painter.image(
                        texture.id(),
                        egui_rect(viewport.tile_rect(tile)),
                        FULL_UV,
                        UNTINTED,
                    );
                }
            }
        }
        if let Some(pinned) = &self.pinned {
            for &tile in &pinned.tiles {
                let rect = egui_rect(viewport.tile_rect(tile));
                if rect.intersects(response.rect) {
                    highlight(&painter, rect);
                }
            }
        }
        let hovered = tile_under(response.hover_pos());
        if let Some(tile) = hovered {
            painter.rect_stroke(
                egui_rect(viewport.tile_rect(tile)),
                0.0,
                egui::Stroke::new(1.0, egui::Color32::WHITE),
                egui::StrokeKind::Inside,
            );
        }

        if let Some(tile) = hovered
            && !response.dragged()
        {
            let source_image = source_image_at(&self.mosaic, tile);
            let thumbnail = self.thumbnails.texture(ctx, source_image);
            response.on_hover_ui_at_pointer(|ui| {
                ui.image(&thumbnail);
                ui.label(source_image.file_name());
            });
        }
        if self.thumbnails.end_frame() {
            ctx.request_repaint();
        }
    }

    /// Pins the inspector to the Source Image filling `tile`, and starts reading its Original.
    fn pin(&mut self, ctx: &egui::Context, tile: Tile) {
        let id = source_image_at(&self.mosaic, tile).id();
        if let Some(pinned) = &self.pinned
            && source_image_at(&self.mosaic, pinned.tile).id() == id
        {
            return;
        }
        let (sender, original) = channel();
        let mosaic = Arc::clone(&self.mosaic);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let source_image = source_image_at(&mosaic, tile);
            let original = mosaic
                .original(source_image, ORIGINAL_SIZE)
                .map(|image| color_image(&image));
            // The receiver goes away if the inspector was unpinned meanwhile.
            let _ = sender.send(original);
            ctx.request_repaint();
        });
        self.pinned = Some(Pinned {
            tile,
            tiles: self.mosaic.tiles_filled_by(id),
            original: Original::Loading(original),
            reveal_error: None,
        });
    }

    /// The pinned Source Image: its Original, where that lives, and how much it is used.
    fn inspector(&mut self, ui: &mut egui::Ui) {
        let Some(pinned) = &mut self.pinned else {
            return;
        };
        if let Original::Loading(receiver) = &pinned.original {
            match receiver.try_recv() {
                Ok(Some(image)) => {
                    pinned.original = Original::Shown(ui.ctx().load_texture(
                        "original",
                        image,
                        egui::TextureOptions::LINEAR,
                    ));
                }
                Ok(None) | Err(TryRecvError::Disconnected) => {
                    pinned.original = Original::Unreachable
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        let source_image = source_image_at(&self.mosaic, pinned.tile);

        ui.heading("Source Image");
        ui.add_space(8.0);
        let (shown, caption) = match &pinned.original {
            Original::Shown(texture) => (texture.clone(), None),
            Original::Loading(_) => (
                self.thumbnails.texture(ui.ctx(), source_image),
                Some("Loading the Original…"),
            ),
            Original::Unreachable => (
                self.thumbnails.texture(ui.ctx(), source_image),
                Some("The Original is unreachable, so this is its Thumbnail."),
            ),
        };
        let side = ui.available_width();
        ui.add(egui::Image::new(&shown).max_size(egui::vec2(side, side)));
        if let Some(caption) = caption {
            ui.weak(caption);
        }
        ui.add_space(8.0);

        ui.strong(source_image.file_name());
        let path = self.mosaic.path_of(source_image);
        ui.label(path.display().to_string());
        ui.label(match pinned.tiles.len() {
            1 => "Fills 1 Tile.".to_owned(),
            count => format!("Fills {count} Tiles."),
        });
        if ui.button("Reveal in Finder").clicked() {
            pinned.reveal_error = reveal_in_finder(&path).err().map(|error| error.to_string());
        }
        if let Some(error) = &pinned.reveal_error {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("Couldn't reveal it: {error}."),
            );
        }
        ui.add_space(8.0);
        ui.weak("Press Escape or click empty space to unpin.");
    }
}

/// The Source Image filling `tile`, which the caller knows is on `mosaic`.
fn source_image_at(mosaic: &Mosaic, tile: Tile) -> &SourceImage {
    mosaic
        .source_image_at(tile)
        .expect("only Tiles on the Mosaic are drawn or inspected")
}

impl ThumbnailTextures {
    fn begin_frame(&mut self) {
        self.uploads_left = THUMBNAIL_UPLOADS_PER_FRAME;
        self.waiting = false;
        self.drawn.clear();
    }

    /// The texture of `source_image`'s Thumbnail, uploaded now if it isn't yet.
    fn texture(&mut self, ctx: &egui::Context, source_image: &SourceImage) -> egui::TextureHandle {
        let id = source_image.id();
        self.drawn.insert(id);
        self.textures
            .entry(id)
            .or_insert_with(|| {
                ctx.load_texture(
                    format!("thumbnail {id}"),
                    color_image(source_image.thumbnail()),
                    egui::TextureOptions::LINEAR,
                )
            })
            .clone()
    }

    /// Like [`Self::texture`], but a texture not uploaded yet is only uploaded while this
    /// frame's uploads last.
    fn texture_if_ready(
        &mut self,
        ctx: &egui::Context,
        source_image: &SourceImage,
    ) -> Option<egui::TextureHandle> {
        if !self.textures.contains_key(&source_image.id()) {
            if self.uploads_left == 0 {
                self.waiting = true;
                return None;
            }
            self.uploads_left -= 1;
        }
        Some(self.texture(ctx, source_image))
    }

    /// Lets go of textures no longer needed. Returns whether Thumbnails are still waiting to
    /// be uploaded, so another frame is needed.
    fn end_frame(&mut self) -> bool {
        if self.textures.len() > MAX_THUMBNAIL_TEXTURES {
            self.textures.retain(|id, _| self.drawn.contains(id));
        }
        self.waiting
    }
}

/// Outlines a highlighted Tile, dark around bright so it shows on any picture.
fn highlight(painter: &egui::Painter, rect: egui::Rect) {
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, egui::Color32::BLACK),
        egui::StrokeKind::Outside,
    );
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 210, 0)),
        egui::StrokeKind::Inside,
    );
}

/// Shows the file at `path` in Finder, or its folder in the platform's file manager.
fn reveal_in_finder(path: &Path) -> std::io::Result<()> {
    let mut command;
    if cfg!(target_os = "macos") {
        command = Command::new("open");
        command.arg("-R").arg(path);
    } else {
        command = Command::new("xdg-open");
        command.arg(path.parent().unwrap_or(path));
    }
    let ran = command.output()?;
    if ran.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&ran.stderr);
        let reason = stderr.trim();
        Err(std::io::Error::other(if reason.is_empty() {
            ran.status.to_string()
        } else {
            reason.to_owned()
        }))
    }
}

fn screen_point(pos: egui::Pos2) -> ScreenPoint {
    ScreenPoint { x: pos.x, y: pos.y }
}

fn screen_rect(rect: egui::Rect) -> ScreenRect {
    ScreenRect {
        min: screen_point(rect.min),
        max: screen_point(rect.max),
    }
}

fn egui_rect(rect: ScreenRect) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(rect.min.x, rect.min.y),
        egui::pos2(rect.max.x, rect.max.y),
    )
}

fn color_image(image: &RgbImage) -> egui::ColorImage {
    let size = [image.width() as usize, image.height() as usize];
    egui::ColorImage::from_rgb(size, image.as_raw())
}

fn display_path(path: Option<&Path>) -> String {
    path.map_or_else(
        || "None chosen".to_owned(),
        |path| path.display().to_string(),
    )
}

/// A fresh seed, different on every call.
fn random_seed() -> u64 {
    RandomState::new().hash_one(std::time::SystemTime::now())
}

/// A summary of the last build: how many Source Images it found, and what it worked around.
fn show_build_report(ui: &mut egui::Ui, report: &BuildReport) {
    let BuildStats {
        source_images,
        cache_hits,
    } = report.stats;
    ui.label(format!(
        "Built from {source_images} Source Images ({cache_hits} already analysed)."
    ));

    let skipped: Vec<_> = report.skipped_files().collect();
    if !skipped.is_empty() {
        ui.collapsing(format!("{} files skipped", skipped.len()), |ui| {
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for warning in skipped {
                        ui.label(warning.to_string());
                    }
                });
        });
    }

    let relaxed = report.relaxed_tile_count();
    if relaxed > 0 {
        ui.label(format!(
            "Not enough Source Images to keep the Repeat Radius everywhere: it was relaxed \
             for {relaxed} Tiles."
        ));
    }
}

fn progress_bar(progress: Option<Progress>) -> egui::ProgressBar {
    let Some(progress) = progress else {
        return egui::ProgressBar::new(0.0).text("Starting…");
    };
    let stage = match progress.stage {
        Stage::AnalysingSourceImages => "Analysing Source Images",
        Stage::MatchingTiles => "Matching Tiles",
        Stage::Rendering => "Rendering Mosaic",
    };
    let fraction = progress.done as f32 / progress.total.max(1) as f32;
    egui::ProgressBar::new(fraction)
        .text(format!("{stage}: {} / {}", progress.done, progress.total))
}

/// The rendered Mosaic, scaled down if it exceeds the GPU's texture size limit.
fn displayable_image(mosaic: &Mosaic, max_texture_side: u32) -> egui::ColorImage {
    let rendered = mosaic.rendered();
    let (width, height) = rendered.dimensions();
    let scaled;
    let image = if width.max(height) > max_texture_side {
        let scale = f64::from(max_texture_side) / f64::from(width.max(height));
        let scaled_width = ((f64::from(width) * scale) as u32).max(1);
        let scaled_height = ((f64::from(height) * scale) as u32).max(1);
        scaled = imageops::resize(rendered, scaled_width, scaled_height, FilterType::Triangle);
        &scaled
    } else {
        rendered
    };
    color_image(image)
}
