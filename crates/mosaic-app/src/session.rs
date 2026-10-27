//! What the window works on: the setup inputs, the Mosaic shown, and the one operation that
//! may be running, with the outcome of the latest. Operations run through a [`Runner`], so the
//! Session never touches threads or the GUI itself.

// The window moves onto the Session in the next commit.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use image::RgbImage;
use mosaic_core::{
    AnalysisCache, BuildError, BuildReport, BuildSettings, Built, CancelToken, ExportReport,
    Mosaic, Progress, Tile,
};

/// Work for a [`Runner`]. It is given a way to wake the UI while it runs, e.g. on progress.
pub type Job = Box<dyn FnOnce(&dyn Fn()) + Send>;

/// Runs the Session's operations away from the UI.
pub trait Runner {
    /// Runs `job`, then wakes the UI so it polls the Session for how the job ended. A job
    /// that panics simply ends: the Session reports it as failed.
    fn spawn(&self, job: Job);
}

/// The inputs a build starts from, as chosen in the setup panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setup {
    pub target_image: Option<PathBuf>,
    pub source_library: Option<PathBuf>,
    pub settings: BuildSettings,
}

/// What an operation was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    Build,
    /// Opening the Mosaic File at this path.
    Open(PathBuf),
    /// Saving the shown Mosaic to the Mosaic File at this path.
    Save(PathBuf),
    /// Exporting the shown Mosaic to a PNG at this path.
    Export(PathBuf),
}

/// How the latest operation ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Built(BuildReport),
    Saved(PathBuf),
    Opened(PathBuf),
    Exported(PathBuf, ExportReport),
    Cancelled,
    Failed(Operation, String),
}

/// Why an operation wasn't started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Another operation is running.
    Busy,
    /// The Target Image or Source Library hasn't been chosen.
    SetupIncomplete,
    /// Saving and exporting need a Mosaic shown.
    NothingShown,
}

/// A Source Image's Original being read away from the UI.
pub struct ReadingOriginal(Receiver<Option<RgbImage>>);

/// How reading an Original ended.
#[derive(Debug)]
pub enum OriginalRead {
    Reachable(RgbImage),
    /// Its file was moved, renamed, edited or deleted since the Mosaic was built.
    Unreachable,
}

impl ReadingOriginal {
    /// How the read ended, or `None` while it is still running. Poll it every frame until it
    /// has ended, then stop.
    pub fn poll(&self) -> Option<OriginalRead> {
        match self.0.try_recv() {
            Ok(Some(original)) => Some(OriginalRead::Reachable(original)),
            // A read that panicked can't have reached the Original either.
            Ok(None) | Err(TryRecvError::Disconnected) => Some(OriginalRead::Unreachable),
            Err(TryRecvError::Empty) => None,
        }
    }
}

pub struct Session {
    runner: Box<dyn Runner>,
    cache: AnalysisCache,
    setup: Setup,
    shown: Option<Arc<Mosaic>>,
    running: Option<Running>,
    outcome: Option<Outcome>,
    /// The rendered picture of a Mosaic shown since the last [`Session::poll`].
    newly_shown: Option<RgbImage>,
}

struct Running {
    operation: Operation,
    /// Stops the operation, for one that can be cancelled.
    cancel: Option<CancelToken>,
    events: Receiver<Event>,
    progress: Option<Progress>,
}

/// What a job sends back while it runs.
enum Event {
    Progress(Progress),
    Finished(Finished),
}

/// How a job ended.
enum Finished {
    /// With a Mosaic to show in place of the shown one, and its rendered picture.
    Showing(Box<Mosaic>, RgbImage, Outcome),
    Ended(Outcome),
    /// Failed for this reason.
    Failed(String),
}

impl Session {
    pub fn new(runner: Box<dyn Runner>, cache: AnalysisCache, setup: Setup) -> Self {
        Self {
            runner,
            cache,
            setup,
            shown: None,
            running: None,
            outcome: None,
            newly_shown: None,
        }
    }

    pub fn setup(&self) -> &Setup {
        &self.setup
    }

    /// The setup inputs, to edit. A running build has its own copy, so edits only affect
    /// the next.
    pub fn setup_mut(&mut self) -> &mut Setup {
        &mut self.setup
    }

    /// The Mosaic shown on the canvas.
    pub fn shown(&self) -> Option<&Arc<Mosaic>> {
        self.shown.as_ref()
    }

    /// The operation running now, if any.
    pub fn running(&self) -> Option<&Operation> {
        self.running.as_ref().map(|running| &running.operation)
    }

    /// How far the running build has got, once it has said.
    pub fn progress(&self) -> Option<Progress> {
        self.running.as_ref().and_then(|running| running.progress)
    }

    /// How the latest operation ended. Starting another clears it.
    pub fn outcome(&self) -> Option<&Outcome> {
        self.outcome.as_ref()
    }

    /// Builds a Mosaic from the setup inputs, to show in place of any shown now. Its rendered
    /// picture is scaled down to fit within `max_texture_side` pixels square.
    pub fn build(&mut self, max_texture_side: u32) -> Result<(), Refused> {
        self.refuse_if_busy()?;
        let (Some(target_image), Some(source_library)) = (
            self.setup.target_image.clone(),
            self.setup.source_library.clone(),
        ) else {
            return Err(Refused::SetupIncomplete);
        };
        let settings = self.setup.settings.clone();
        let cache = self.cache.clone();
        let cancel = CancelToken::new();
        let build_cancel = cancel.clone();
        self.start(Operation::Build, Some(cancel), move |send, wake| {
            let result = mosaic_core::build(
                &target_image,
                &source_library,
                &settings,
                &cache,
                |progress| {
                    send(Event::Progress(progress));
                    wake();
                },
                &build_cancel,
            );
            match result {
                Ok(Built { mosaic, report }) => {
                    let rendered = mosaic.rendered_within(max_texture_side).into_owned();
                    Finished::Showing(Box::new(mosaic), rendered, Outcome::Built(report))
                }
                Err(BuildError::Cancelled) => Finished::Ended(Outcome::Cancelled),
                Err(error) => Finished::Failed(error.to_string()),
            }
        });
        Ok(())
    }

    /// Opens the Mosaic File at `path`, to show in place of any Mosaic shown now, and sets the
    /// setup inputs to those it was built from. Its rendered picture is scaled down to fit
    /// within `max_texture_side` pixels square.
    pub fn open(&mut self, path: PathBuf, max_texture_side: u32) -> Result<(), Refused> {
        self.refuse_if_busy()?;
        self.start(
            Operation::Open(path.clone()),
            None,
            move |_, _| match Mosaic::open(&path) {
                Ok(mosaic) => {
                    let rendered = mosaic.rendered_within(max_texture_side).into_owned();
                    Finished::Showing(Box::new(mosaic), rendered, Outcome::Opened(path))
                }
                Err(error) => Finished::Failed(error.to_string()),
            },
        );
        Ok(())
    }

    /// Saves the shown Mosaic to the Mosaic File at `path`.
    pub fn save(&mut self, path: PathBuf) -> Result<(), Refused> {
        let mosaic = self.mosaic_to_write()?;
        self.start(
            Operation::Save(path.clone()),
            None,
            move |_, _| match mosaic.save(&path) {
                Ok(()) => Finished::Ended(Outcome::Saved(path)),
                Err(error) => Finished::Failed(error.to_string()),
            },
        );
        Ok(())
    }

    /// Exports the shown Mosaic to a PNG at `path`, each Tile drawn `tile_size` pixels square.
    pub fn export(&mut self, path: PathBuf, tile_size: u32) -> Result<(), Refused> {
        let mosaic = self.mosaic_to_write()?;
        self.start(
            Operation::Export(path.clone()),
            None,
            move |_, _| match mosaic.export_png(&path, tile_size) {
                Ok(report) => Finished::Ended(Outcome::Exported(path, report)),
                Err(error) => Finished::Failed(error.to_string()),
            },
        );
        Ok(())
    }

    /// The shown Mosaic, for an operation that writes it to a file.
    fn mosaic_to_write(&self) -> Result<Arc<Mosaic>, Refused> {
        self.refuse_if_busy()?;
        self.shown.clone().ok_or(Refused::NothingShown)
    }

    /// Reads the Original of the Source Image filling `tile` on the shown Mosaic, scaled down
    /// to fit within `max_side` pixels square. It runs alongside any operation.
    pub fn read_original(&self, tile: Tile, max_side: u32) -> ReadingOriginal {
        let (sender, original) = channel();
        let mosaic = self.shown.clone();
        self.runner.spawn(Box::new(move |_| {
            let read = mosaic.as_ref().and_then(|mosaic| {
                mosaic
                    .source_image_at(tile)
                    .and_then(|source_image| mosaic.original(source_image, max_side))
            });
            // The receiver goes away if the inspector was unpinned meanwhile.
            let _ = sender.send(read);
        }));
        ReadingOriginal(original)
    }

    /// Stops the running build. Other operations can't be cancelled, so they carry on.
    pub fn cancel(&self) {
        if let Some(cancel) = self
            .running
            .as_ref()
            .and_then(|running| running.cancel.as_ref())
        {
            cancel.cancel();
        }
    }

    /// Takes in what the running operation has sent, and returns the rendered picture of a
    /// Mosaic newly shown since the last poll, for the UI to upload. Call it every frame.
    pub fn poll(&mut self) -> Option<RgbImage> {
        if let Some(running) = &mut self.running {
            let finished = loop {
                match running.events.try_recv() {
                    Ok(Event::Progress(progress)) => running.progress = Some(progress),
                    Ok(Event::Finished(finished)) => break Some(finished),
                    Err(TryRecvError::Empty) => break None,
                    // The job panicked before it finished.
                    Err(TryRecvError::Disconnected) => {
                        break Some(Finished::Failed("it stopped unexpectedly".to_owned()));
                    }
                }
            };
            if let Some(finished) = finished {
                let operation = self
                    .running
                    .take()
                    .expect("an operation is running")
                    .operation;
                self.finish(operation, finished);
            }
        }
        self.newly_shown.take()
    }

    fn finish(&mut self, operation: Operation, finished: Finished) {
        self.outcome = Some(match finished {
            Finished::Showing(mosaic, rendered, outcome) => {
                if let Outcome::Opened(_) = outcome {
                    self.setup = Setup {
                        target_image: Some(mosaic.target_image().to_owned()),
                        source_library: Some(mosaic.source_library().to_owned()),
                        settings: mosaic.settings().clone(),
                    };
                }
                self.shown = Some(Arc::new(*mosaic));
                self.newly_shown = Some(rendered);
                outcome
            }
            Finished::Ended(outcome) => outcome,
            Finished::Failed(reason) => Outcome::Failed(operation, reason),
        });
    }

    fn refuse_if_busy(&self) -> Result<(), Refused> {
        match self.running {
            Some(_) => Err(Refused::Busy),
            None => Ok(()),
        }
    }

    /// Runs `job` as `operation`, replacing the latest outcome.
    fn start(
        &mut self,
        operation: Operation,
        cancel: Option<CancelToken>,
        job: impl FnOnce(&dyn Fn(Event), &dyn Fn()) -> Finished + Send + 'static,
    ) {
        let (sender, events) = channel();
        self.runner.spawn(Box::new(move |wake| {
            // The receiver only goes away when the app closes.
            let send = |event| {
                let _ = sender.send(event);
            };
            let finished = job(&send, wake);
            send(Event::Finished(finished));
        }));
        self.running = Some(Running {
            operation,
            cancel,
            events,
            progress: None,
        });
        self.outcome = None;
    }
}
