//! Mosaic Maker's window: gathers input, runs a build off the UI thread, and shows the Mosaic.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use eframe::egui;
use image::imageops::{self, FilterType};
use mosaic_core::{BuildError, BuildSettings, CancelToken, Mosaic, Progress, Stage};

const DEFAULT_TILES_ACROSS: u32 = 60;

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
    running_build: Option<RunningBuild>,
    mosaic: Option<egui::TextureHandle>,
    status: Option<String>,
}

impl Default for MosaicMakerApp {
    fn default() -> Self {
        Self {
            target_image: None,
            source_library: None,
            tiles_across: DEFAULT_TILES_ACROSS,
            running_build: None,
            mosaic: None,
            status: None,
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
    /// The rendered Mosaic, ready to upload as a texture.
    Finished(Result<egui::ColorImage, BuildError>),
}

/// How a build ended, as far as the window is concerned.
enum Outcome {
    Built(egui::ColorImage),
    Cancelled,
    Failed(String),
}

impl eframe::App for MosaicMakerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.receive_build_events(ui.ctx());
        egui::Panel::left("setup").show(ui, |ui| self.setup_panel(ui));
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

        ui.label("Tiles Across");
        ui.add_enabled(
            idle,
            egui::DragValue::new(&mut self.tiles_across).range(1..=300),
        );
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
    }

    fn mosaic_panel(&self, ui: &mut egui::Ui) {
        match &self.mosaic {
            Some(texture) => {
                ui.centered_and_justified(|ui| {
                    ui.add(egui::Image::new(texture).shrink_to_fit());
                });
            }
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
        };
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
                |progress| {
                    // The receiver only goes away when the app closes.
                    let _ = sender.send(BuildEvent::Progress(progress));
                    ctx.request_repaint();
                },
                &build_cancel,
            );
            let image = result.map(|mosaic| displayable_image(&mosaic, max_texture_side));
            let _ = sender.send(BuildEvent::Finished(image));
            ctx.request_repaint();
        });
        self.running_build = Some(RunningBuild {
            cancel,
            events,
            progress: None,
        });
        self.status = None;
    }

    fn receive_build_events(&mut self, ctx: &egui::Context) {
        let Some(running_build) = &mut self.running_build else {
            return;
        };
        let finished = loop {
            match running_build.events.try_recv() {
                Ok(BuildEvent::Progress(progress)) => running_build.progress = Some(progress),
                Ok(BuildEvent::Finished(Ok(image))) => break Some(Outcome::Built(image)),
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
            Outcome::Built(image) => {
                self.mosaic = Some(ctx.load_texture("mosaic", image, egui::TextureOptions::LINEAR));
                self.status = None;
            }
            Outcome::Cancelled => self.status = Some("Build cancelled.".to_owned()),
            Outcome::Failed(error) => {
                self.status = Some(format!("Build failed: {error}."));
            }
        }
    }
}

fn display_path(path: Option<&Path>) -> String {
    path.map_or_else(
        || "None chosen".to_owned(),
        |path| path.display().to_string(),
    )
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
    let size = [image.width() as usize, image.height() as usize];
    egui::ColorImage::from_rgb(size, image.as_raw())
}
