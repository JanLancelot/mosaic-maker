//! What the window works on: the setup inputs, the Mosaic shown, and the one operation that
//! may be running, with the outcome of the latest. Operations run through a [`Runner`], so the
//! Session never touches threads or the GUI itself.

use std::fmt::Display;
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
    /// that panics simply ends: the Session reports it as crashed.
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
    /// Built; [`Session::build_report`] says what the build worked around.
    Built,
    Saved(PathBuf),
    Opened(PathBuf),
    Exported(PathBuf, ExportReport),
    Cancelled,
    /// Failed for the reason given.
    Failed(Operation, String),
    /// Stopped by a panic, so there's no reason to give.
    Crashed(Operation),
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

/// A Mosaic the Session has just shown, handed to the UI once by [`Session::poll`].
pub struct NewlyShown {
    pub mosaic: Arc<Mosaic>,
    /// Its rendered picture, scaled to fit the texture limit it was opened or built for.
    pub rendered: RgbImage,
}

pub struct Session {
    runner: Box<dyn Runner>,
    cache: AnalysisCache,
    setup: Setup,
    shown: Option<Arc<Mosaic>>,
    build_report: Option<BuildReport>,
    running: Option<Running>,
    outcome: Option<Outcome>,
    newly_shown: Option<NewlyShown>,
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
    /// With a Mosaic to show in place of the shown one.
    ShowingMosaic {
        mosaic: Box<Mosaic>,
        rendered: RgbImage,
        /// What the build that made it worked around, for a built Mosaic.
        report: Option<BuildReport>,
        outcome: Outcome,
    },
    NothingToShow(Outcome),
    /// Failed for this reason.
    Failed(String),
}

impl Finished {
    /// Shows `mosaic`, its rendered picture scaled to fit within `max_texture_side`.
    fn showing(
        mosaic: Mosaic,
        max_texture_side: u32,
        report: Option<BuildReport>,
        outcome: Outcome,
    ) -> Self {
        Finished::ShowingMosaic {
            rendered: mosaic.rendered_within(max_texture_side),
            mosaic: Box::new(mosaic),
            report,
            outcome,
        }
    }

    fn failed(error: impl Display) -> Self {
        Finished::Failed(error.to_string())
    }
}

impl Session {
    pub fn new(runner: Box<dyn Runner>, cache: AnalysisCache, setup: Setup) -> Self {
        Self {
            runner,
            cache,
            setup,
            shown: None,
            build_report: None,
            running: None,
            outcome: None,
            newly_shown: None,
        }
    }

    /// The setup inputs, to edit. A running build has its own copy, so edits only affect
    /// the next.
    pub fn setup_mut(&mut self) -> &mut Setup {
        &mut self.setup
    }

    /// What the build that made the shown Mosaic worked around. A build clears it as it
    /// starts, and opening a Mosaic File clears it too; saving and exporting keep it.
    pub fn build_report(&self) -> Option<&BuildReport> {
        self.build_report.as_ref()
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
        self.build_report = None;
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
                    Finished::showing(mosaic, max_texture_side, Some(report), Outcome::Built)
                }
                Err(BuildError::Cancelled) => Finished::NothingToShow(Outcome::Cancelled),
                Err(error) => Finished::failed(error),
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
                    Finished::showing(mosaic, max_texture_side, None, Outcome::Opened(path))
                }
                Err(error) => Finished::failed(error),
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
                Ok(()) => Finished::NothingToShow(Outcome::Saved(path)),
                Err(error) => Finished::failed(error),
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
                Ok(report) => Finished::NothingToShow(Outcome::Exported(path, report)),
                Err(error) => Finished::failed(error),
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

    /// Takes in what the running operation has sent, and hands over a Mosaic newly shown since
    /// the last poll, for the UI to upload. Call it every frame.
    pub fn poll(&mut self) -> Option<NewlyShown> {
        if let Some(running) = &mut self.running {
            let ended = loop {
                match running.events.try_recv() {
                    Ok(Event::Progress(progress)) => running.progress = Some(progress),
                    Ok(Event::Finished(finished)) => break Some(Some(finished)),
                    Err(TryRecvError::Empty) => break None,
                    // The job panicked before it finished.
                    Err(TryRecvError::Disconnected) => break Some(None),
                }
            };
            if let Some(finished) = ended {
                let operation = self
                    .running
                    .take()
                    .expect("an operation is running")
                    .operation;
                self.outcome = Some(match finished {
                    Some(finished) => self.finish(operation, finished),
                    None => Outcome::Crashed(operation),
                });
            }
        }
        self.newly_shown.take()
    }

    /// Takes over what `operation` made, and says how it ended.
    fn finish(&mut self, operation: Operation, finished: Finished) -> Outcome {
        match finished {
            Finished::ShowingMosaic {
                mosaic,
                rendered,
                report,
                outcome,
            } => {
                if let Outcome::Opened(_) = outcome {
                    self.setup = Setup {
                        target_image: Some(mosaic.target_image().to_owned()),
                        source_library: Some(mosaic.source_library().to_owned()),
                        settings: mosaic.settings().clone(),
                    };
                }
                let mosaic = Arc::new(*mosaic);
                self.shown = Some(Arc::clone(&mosaic));
                self.build_report = report;
                self.newly_shown = Some(NewlyShown { mosaic, rendered });
                outcome
            }
            Finished::NothingToShow(outcome) => outcome,
            Finished::Failed(reason) => Outcome::Failed(operation, reason),
        }
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
