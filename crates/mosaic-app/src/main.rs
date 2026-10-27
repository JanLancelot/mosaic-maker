//! Mosaic Maker's window: gathers input, runs operations through a [`Session`], and shows the
//! Mosaic on a canvas to pan, zoom and inspect. Its File menu saves, opens and exports Mosaics.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, RandomState};
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use eframe::egui;
use image::RgbImage;
use mosaic_core::{
    AnalysisCache, BuildReport, BuildSettings, BuildStats, DEFAULT_REPEAT_RADIUS, ExportReport,
    Mosaic, Progress, RENDERED_TILE_SIZE, ScreenPoint, ScreenRect, SourceImage, SourceImageId,
    Stage, Tile, Viewport,
};

use session::{
    Job, NewlyShown, Operation, OriginalRead, Outcome, ReadingOriginal, Runner, Session, Setup,
};

const DEFAULT_TILES_ACROSS: u32 = 60;

/// The Tile Size Export PNG starts with.
const DEFAULT_EXPORT_TILE_SIZE: u32 = 64;

/// How much one point of scrolling zooms the canvas.
const SCROLL_ZOOM_SPEED: f32 = 0.005;

/// Tiles drawn larger than this, in physical pixels, are drawn from their Thumbnails, since the
/// rendered Mosaic (`RENDERED_TILE_SIZE` pixels square per Tile) would look soft. Any smaller, and so
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

mod session;

#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod smoke_tests;
#[cfg(test)]
mod test_fixture;

fn main() -> eframe::Result {
    eframe::run_native(
        "Mosaic Maker",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            let setup = Setup {
                target_image: None,
                source_library: None,
                settings: BuildSettings {
                    tiles_across: DEFAULT_TILES_ACROSS,
                    repeat_radius: DEFAULT_REPEAT_RADIUS,
                    seed: random_seed(),
                },
            };
            Ok(Box::new(MosaicMakerApp::new(
                &cc.egui_ctx,
                AnalysisCache::in_user_cache_folder(),
                setup,
            )))
        }),
    )
}

struct MosaicMakerApp {
    session: Session,
    /// The Session's shown Mosaic as the canvas draws it, once it has been uploaded.
    shown: Option<ShownMosaic>,
    /// The Tile Size for Export PNG.
    export_tile_size: u32,
    export_dialog_open: bool,
}

/// Runs the Session's jobs on threads, repainting the window as they go and when they end.
struct ThreadRunner {
    ctx: egui::Context,
}

impl Runner for ThreadRunner {
    fn spawn(&self, job: Job) {
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let wake = || ctx.request_repaint();
            // A job that panics ends here; the Session notices when the window repaints.
            let _ = std::panic::catch_unwind(AssertUnwindSafe(|| job(&wake)));
            ctx.request_repaint();
        });
    }
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
    /// Being read; the Thumbnail stands in meanwhile.
    Loading(ReadingOriginal),
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
        if let Some(newly_shown) = self.session.poll() {
            self.show(ui.ctx(), newly_shown);
        }
        if let Some(shown) = &mut self.shown {
            shown.receive_original(ui.ctx());
        }
        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        if self.export_dialog_open {
            self.export_dialog(ui.ctx());
        }
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
    fn new(ctx: &egui::Context, cache: AnalysisCache, setup: Setup) -> Self {
        let runner = ThreadRunner { ctx: ctx.clone() };
        Self {
            session: Session::new(Box::new(runner), cache, setup),
            shown: None,
            export_tile_size: DEFAULT_EXPORT_TILE_SIZE,
            export_dialog_open: false,
        }
    }

    /// Whether nothing is being built, saved, opened or exported.
    fn idle(&self) -> bool {
        self.session.running().is_none()
    }

    /// Puts the Session's newly shown Mosaic on the canvas, uploading its rendered picture.
    fn show(&mut self, ctx: &egui::Context, newly_shown: NewlyShown) {
        let NewlyShown { mosaic, rendered } = newly_shown;
        self.shown = Some(ShownMosaic {
            mosaic,
            rendered: ctx.load_texture(
                "mosaic",
                color_image(&rendered),
                egui::TextureOptions::LINEAR,
            ),
            viewport: None,
            thumbnails: ThumbnailTextures::default(),
            pinned: None,
        });
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let idle = self.idle();
        let saveable = idle && self.shown.is_some();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                // The Session would refuse what these buttons are disabled for, so its
                // refusals aren't shown.
                if ui.add_enabled(idle, egui::Button::new("Open…")).clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Mosaic", &["mosaic"])
                        .pick_file()
                {
                    let _ = self.session.open(path, max_texture_side(ui.ctx()));
                }
                if ui
                    .add_enabled(saveable, egui::Button::new("Save…"))
                    .clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Mosaic", &["mosaic"])
                        .set_file_name(self.suggested_file_name("mosaic"))
                        .save_file()
                {
                    let _ = self.session.save(path);
                }
                ui.separator();
                if ui
                    .add_enabled(saveable, egui::Button::new("Export PNG…"))
                    .clicked()
                {
                    self.export_dialog_open = true;
                }
            });
        });
    }

    /// Asks for the Tile Size to export at, then where to.
    fn export_dialog(&mut self, ctx: &egui::Context) {
        let mut export = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::unique("export")).show(ctx, |ui| {
            ui.heading("Export PNG");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let label = ui.label("Tile Size");
                ui.add(
                    egui::DragValue::new(&mut self.export_tile_size)
                        .range(1..=512)
                        .suffix(" px"),
                )
                .labelled_by(label.id);
            });
            ui.weak("Tiles are drawn from their Originals, or from Thumbnails where those are unreachable.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                export = ui.button("Export…").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if export || cancel || modal.should_close() {
            self.export_dialog_open = false;
        }
        if export
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("PNG", &["png"])
                .set_file_name(self.suggested_file_name("png"))
                .save_file()
        {
            // Export PNG is only offered while the Session would accept it.
            let _ = self.session.export(path, self.export_tile_size);
        }
    }

    /// A file name for saving or exporting the shown Mosaic: its Target Image's, with
    /// `extension`.
    fn suggested_file_name(&self, extension: &str) -> String {
        let stem = self
            .shown
            .as_ref()
            .and_then(|shown| shown.mosaic.target_image().file_stem())
            .map_or_else(|| "Mosaic".into(), |stem| stem.to_string_lossy());
        format!("{stem}.{extension}")
    }

    fn setup_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mosaic Maker");
        ui.add_space(8.0);
        let idle = self.idle();
        let setup = self.session.setup_mut();

        ui.label("Target Image");
        if ui
            .add_enabled(idle, egui::Button::new("Choose Target Image…"))
            .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Images", &["jpg", "jpeg", "png", "webp"])
                .pick_file()
        {
            setup.target_image = Some(path);
        }
        ui.label(display_path(setup.target_image.as_deref()));
        ui.add_space(8.0);

        ui.label("Source Library");
        if ui
            .add_enabled(idle, egui::Button::new("Choose Source Library…"))
            .clicked()
            && let Some(path) = rfd::FileDialog::new().pick_folder()
        {
            setup.source_library = Some(path);
        }
        ui.label(display_path(setup.source_library.as_deref()));
        ui.add_space(8.0);

        let settings = &mut setup.settings;
        let label = ui.label("Tiles Across");
        ui.add_enabled(
            idle,
            egui::DragValue::new(&mut settings.tiles_across).range(1..=300),
        )
        .labelled_by(label.id);
        ui.add_space(8.0);

        let label = ui.label("Repeat Radius");
        ui.add_enabled(
            idle,
            egui::DragValue::new(&mut settings.repeat_radius).range(0..=20),
        )
        .labelled_by(label.id);
        ui.add_space(8.0);

        let label = ui.label("Seed");
        ui.horizontal(|ui| {
            ui.add_enabled(idle, egui::DragValue::new(&mut settings.seed))
                .labelled_by(label.id);
            if ui
                .add_enabled(idle, egui::Button::new("Randomise"))
                .clicked()
            {
                settings.seed = random_seed();
            }
        });
        ui.add_space(8.0);

        let ready = setup.target_image.is_some() && setup.source_library.is_some();
        let building = self.session.running() == Some(&Operation::Build);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(idle && ready, egui::Button::new("Generate"))
                .clicked()
            {
                // Generate is only enabled while the Session would accept it.
                let _ = self.session.build(max_texture_side(ui.ctx()));
            }
            if building && ui.button("Cancel").clicked() {
                self.session.cancel();
            }
        });

        if building {
            ui.add_space(8.0);
            ui.add(progress_bar(self.session.progress()));
        }
        if let Some(status) = self.session.outcome().and_then(status) {
            ui.add_space(8.0);
            ui.label(status);
        }
        if let (Some(Outcome::Exported(_, report)), Some(shown)) =
            (self.session.outcome(), &self.shown)
        {
            show_export_report(ui, report, &shown.mosaic);
        }
        if let Some(report) = self.session.build_report() {
            ui.add_space(8.0);
            show_build_report(ui, report);
        }
    }

    fn mosaic_panel(&mut self, ui: &mut egui::Ui) {
        match &mut self.shown {
            Some(shown) => shown.canvas(ui, &self.session),
            None => {
                ui.centered_and_justified(|ui| {
                    ui.label("Choose a Target Image and a Source Library, then Generate.");
                });
            }
        }
    }
}

/// The largest texture the GPU takes, as a side in pixels.
fn max_texture_side(ctx: &egui::Context) -> u32 {
    ctx.input(|input| input.max_texture_side) as u32
}

/// What the status line says about how the latest operation ended, if anything. A build
/// says it through its report instead.
fn status(outcome: &Outcome) -> Option<String> {
    Some(match outcome {
        Outcome::Built => return None,
        Outcome::Saved(path) => format!("Saved to {}.", path.display()),
        Outcome::Opened(path) => format!("Opened {}.", path.display()),
        Outcome::Exported(path, _) => format!("Exported to {}.", path.display()),
        Outcome::Cancelled => "Build cancelled.".to_owned(),
        Outcome::Failed(operation, reason) => match operation {
            Operation::Build => format!("Build failed: {reason}."),
            Operation::Open(path) => format!("Couldn't open {}: {reason}.", path.display()),
            Operation::Save(path) => format!("Couldn't save {}: {reason}.", path.display()),
            Operation::Export(path) => format!("Couldn't export {}: {reason}.", path.display()),
        },
        Outcome::Crashed(Operation::Build) => {
            "Build failed: the build stopped unexpectedly.".to_owned()
        }
        Outcome::Crashed(_) => "Something went wrong with the file.".to_owned(),
    })
}

impl ShownMosaic {
    /// Draws the Mosaic, and pans, zooms, hovers and pins as the pointer asks.
    fn canvas(&mut self, ui: &mut egui::Ui, session: &Session) {
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
                Some(tile) => self.pin(session, tile),
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
    fn pin(&mut self, session: &Session, tile: Tile) {
        let id = source_image_at(&self.mosaic, tile).id();
        if let Some(pinned) = &self.pinned
            && source_image_at(&self.mosaic, pinned.tile).id() == id
        {
            return;
        }
        self.pinned = Some(Pinned {
            tile,
            tiles: self.mosaic.tiles_filled_by(id),
            original: Original::Loading(session.read_original(tile, ORIGINAL_SIZE)),
            reveal_error: None,
        });
    }

    /// Shows the pinned Source Image's Original once it has been read.
    fn receive_original(&mut self, ctx: &egui::Context) {
        let Some(pinned) = &mut self.pinned else {
            return;
        };
        let Original::Loading(reading) = &pinned.original else {
            return;
        };
        match reading.poll() {
            Some(OriginalRead::Reachable(image)) => {
                pinned.original = Original::Shown(ctx.load_texture(
                    "original",
                    color_image(&image),
                    egui::TextureOptions::LINEAR,
                ));
            }
            Some(OriginalRead::Unreachable) => pinned.original = Original::Unreachable,
            None => {}
        }
    }

    /// The pinned Source Image: its Original, where that lives, and how much it is used.
    fn inspector(&mut self, ui: &mut egui::Ui) {
        let Some(pinned) = &mut self.pinned else {
            return;
        };
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
        let unreachable = matches!(pinned.original, Original::Unreachable);
        if ui
            .add_enabled(!unreachable, egui::Button::new("Reveal in Finder"))
            .clicked()
        {
            pinned.reveal_error = reveal_in_finder(&path).err().map(|error| error.to_string());
        }
        if unreachable {
            ui.weak(
                "There's nothing to reveal: the Original was moved, renamed, edited or \
                 deleted since the Mosaic was built.",
            );
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

/// Which Tiles the last Export drew from Thumbnails, if any.
fn show_export_report(ui: &mut egui::Ui, report: &ExportReport, mosaic: &Mosaic) {
    let tiles = &report.tiles_from_thumbnails;
    let heading = match tiles.len() {
        0 => return,
        1 => "1 Tile drawn from its Thumbnail".to_owned(),
        count => format!("{count} Tiles drawn from Thumbnails"),
    };
    ui.add_space(8.0);
    ui.collapsing(heading, |ui| {
        ui.weak("Their Originals are unreachable, so the Export used Thumbnails instead.");
        let row_height = ui.text_style_height(&egui::TextStyle::Body);
        // Only the rows in view are laid out, since a large Mosaic has thousands of Tiles.
        egui::ScrollArea::vertical().max_height(200.0).show_rows(
            ui,
            row_height,
            tiles.len(),
            |ui, rows| {
                for &tile in &tiles[rows] {
                    ui.label(format!(
                        "Column {}, row {}: {}",
                        tile.column,
                        tile.row,
                        source_image_at(mosaic, tile).file_name()
                    ));
                }
            },
        );
    });
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
