//! The Session, driven without a window: operations are queued and run when a test says, so
//! what the Session shows, refuses and reports can be checked step by step. What a build, an
//! Open or an Export does in its own right is tested in `mosaic-core`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use mosaic_core::{BuildSettings, Tile};

use crate::session::{Job, Operation, OriginalRead, Outcome, Refused, Runner, Session, Setup};
use crate::test_fixture::{Fixture, ROWS, TILES_ACROSS};

/// Large enough that the fixture's rendered Mosaic is never scaled down.
const MAX_TEXTURE_SIDE: u32 = 8192;

/// Holds jobs until the test runs them. Clones share the same queue.
#[derive(Clone, Default)]
struct QueuedRunner {
    jobs: Arc<Mutex<VecDeque<Job>>>,
}

impl Runner for QueuedRunner {
    fn spawn(&self, job: Job) {
        self.jobs.lock().unwrap().push_back(job);
    }
}

impl QueuedRunner {
    /// Runs every queued job to completion, in order.
    fn run_all(&self) {
        while let Some(job) = self.next() {
            job(&|| {});
        }
    }

    /// Ends every queued job as a panic on its thread would: dropped before it finishes.
    fn crash_all(&self) {
        self.jobs.lock().unwrap().clear();
    }

    fn next(&self) -> Option<Job> {
        self.jobs.lock().unwrap().pop_front()
    }
}

fn session(fixture: &Fixture) -> (Session, QueuedRunner) {
    let runner = QueuedRunner::default();
    let session = Session::new(Box::new(runner.clone()), fixture.cache(), fixture.setup());
    (session, runner)
}

#[test]
fn a_finished_build_shows_its_mosaic_hands_over_its_picture_once_and_reports() {
    let fixture = Fixture::new();
    let (mut session, runner) = session(&fixture);

    session.build(MAX_TEXTURE_SIDE).unwrap();
    assert!(session.running().is_some());
    runner.run_all();
    let rendered = session.poll().expect("the built Mosaic is newly shown");

    let shown = session.shown().expect("a Mosaic is shown");
    assert_eq!((shown.tiles_across(), shown.rows()), (TILES_ACROSS, ROWS));
    assert_eq!(rendered, *shown.rendered());
    assert!(session.poll().is_none());
    assert!(session.running().is_none());
    let Some(Outcome::Built(report)) = session.outcome() else {
        panic!("expected Built, got {:?}", session.outcome());
    };
    assert_eq!(report.stats.source_images, 4);
}

/// A Session showing the fixture's Mosaic, with nothing running.
fn built_session(fixture: &Fixture) -> (Session, QueuedRunner) {
    let (mut session, runner) = session(fixture);
    session.build(MAX_TEXTURE_SIDE).unwrap();
    runner.run_all();
    session.poll();
    (session, runner)
}

#[test]
fn a_build_cancelled_before_it_runs_ends_cancelled_and_the_earlier_mosaic_stays_shown() {
    let fixture = Fixture::new();
    let (mut session, runner) = built_session(&fixture);
    let earlier = Arc::clone(session.shown().unwrap());

    session.build(MAX_TEXTURE_SIDE).unwrap();
    session.cancel();
    runner.run_all();

    assert!(session.poll().is_none());
    assert_eq!(session.outcome(), Some(&Outcome::Cancelled));
    assert!(Arc::ptr_eq(session.shown().unwrap(), &earlier));
    assert!(session.running().is_none());
}

#[test]
fn a_failed_build_ends_failed_and_the_earlier_mosaic_stays_shown() {
    let fixture = Fixture::new();
    let (mut session, runner) = built_session(&fixture);
    let earlier = Arc::clone(session.shown().unwrap());
    let empty_library = fixture.path("empty");
    std::fs::create_dir(&empty_library).unwrap();
    session.setup_mut().source_library = Some(empty_library);

    session.build(MAX_TEXTURE_SIDE).unwrap();
    runner.run_all();

    assert!(session.poll().is_none());
    assert!(
        matches!(
            session.outcome(),
            Some(Outcome::Failed(Operation::Build, _))
        ),
        "expected a failed build, got {:?}",
        session.outcome()
    );
    assert!(Arc::ptr_eq(session.shown().unwrap(), &earlier));
}

#[test]
fn saving_or_exporting_with_no_mosaic_shown_is_refused() {
    let fixture = Fixture::new();
    let (mut session, _runner) = session(&fixture);

    assert_eq!(
        session.save(fixture.path("stripes.mosaic")),
        Err(Refused::NothingShown)
    );
    assert_eq!(
        session.export(fixture.path("stripes.png"), 16),
        Err(Refused::NothingShown)
    );
    assert!(session.running().is_none());
}

#[test]
fn opening_restores_the_setup_it_was_built_from_and_replaces_the_shown_mosaic() {
    let fixture = Fixture::new();
    let (mut saved, runner) = built_session(&fixture);
    let path = fixture.path("stripes.mosaic");
    saved.save(path.clone()).unwrap();
    runner.run_all();
    saved.poll();
    assert_eq!(saved.outcome(), Some(&Outcome::Saved(path.clone())));

    let (mut session, runner) = built_session(&fixture);
    let earlier = Arc::clone(session.shown().unwrap());
    *session.setup_mut() = Setup {
        target_image: None,
        source_library: None,
        settings: BuildSettings {
            tiles_across: 3,
            repeat_radius: 5,
            seed: 42,
        },
    };
    session.open(path.clone(), MAX_TEXTURE_SIDE).unwrap();
    runner.run_all();

    let rendered = session.poll().expect("the opened Mosaic is newly shown");
    assert_eq!(session.outcome(), Some(&Outcome::Opened(path)));
    assert_eq!(*session.setup(), fixture.setup());
    let shown = session.shown().unwrap();
    assert!(!Arc::ptr_eq(shown, &earlier));
    assert_eq!(rendered, *shown.rendered());
}

#[test]
fn starting_an_operation_replaces_the_previous_outcome_and_its_export_report() {
    let fixture = Fixture::new();
    let (mut session, runner) = built_session(&fixture);
    std::fs::remove_dir_all(fixture.source_library()).unwrap();
    let png = fixture.path("stripes.png");
    session.export(png.clone(), 16).unwrap();
    runner.run_all();
    session.poll();
    let Some(Outcome::Exported(path, report)) = session.outcome() else {
        panic!("expected Exported, got {:?}", session.outcome());
    };
    assert_eq!(*path, png);
    assert_eq!(
        report.tiles_from_thumbnails.len(),
        (TILES_ACROSS * ROWS) as usize
    );

    let mosaic_file = fixture.path("stripes.mosaic");
    session.save(mosaic_file.clone()).unwrap();
    assert_eq!(session.outcome(), None);
    runner.run_all();
    session.poll();

    assert_eq!(session.outcome(), Some(&Outcome::Saved(mosaic_file)));
}

#[test]
fn starting_anything_while_an_operation_runs_is_refused_as_busy() {
    let fixture = Fixture::new();
    let (mut session, runner) = built_session(&fixture);
    let mosaic_file = fixture.path("stripes.mosaic");
    session.save(mosaic_file.clone()).unwrap();

    assert_eq!(session.build(MAX_TEXTURE_SIDE), Err(Refused::Busy));
    assert_eq!(
        session.open(mosaic_file.clone(), MAX_TEXTURE_SIDE),
        Err(Refused::Busy)
    );
    assert_eq!(
        session.save(fixture.path("other.mosaic")),
        Err(Refused::Busy)
    );
    assert_eq!(
        session.export(fixture.path("stripes.png"), 16),
        Err(Refused::Busy)
    );
    assert_eq!(
        session.running(),
        Some(&Operation::Save(mosaic_file.clone()))
    );

    runner.run_all();
    session.poll();
    assert_eq!(session.outcome(), Some(&Outcome::Saved(mosaic_file)));
    assert!(session.running().is_none());
}

#[test]
fn a_job_that_panics_ends_failed_and_the_session_is_idle_again() {
    let fixture = Fixture::new();
    let (mut session, runner) = built_session(&fixture);
    let mosaic_file = fixture.path("stripes.mosaic");
    session.save(mosaic_file.clone()).unwrap();

    runner.crash_all();
    session.poll();

    assert!(
        matches!(session.outcome(), Some(Outcome::Failed(Operation::Save(path), _)) if *path == mosaic_file),
        "expected a failed save, got {:?}",
        session.outcome()
    );
    assert!(session.running().is_none());
    session.save(mosaic_file).unwrap();
}

#[test]
fn an_original_reads_as_its_picture_while_its_file_is_there_and_as_unreachable_once_gone() {
    let fixture = Fixture::new();
    let (session, runner) = built_session(&fixture);
    let tile = Tile { column: 0, row: 0 };

    let reading = session.read_original(tile, 1024);
    assert!(reading.poll().is_none());
    runner.run_all();
    let Some(OriginalRead::Reachable(original)) = reading.poll() else {
        panic!("expected the Original to be read");
    };
    assert_eq!(original.dimensions(), (64, 64));

    std::fs::remove_dir_all(fixture.source_library()).unwrap();
    let reading = session.read_original(tile, 1024);
    runner.run_all();
    assert!(matches!(reading.poll(), Some(OriginalRead::Unreachable)));
}
