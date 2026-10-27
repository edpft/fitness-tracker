//! FR-010: only one extraction run at a time.
//!
//! Two runs sharing a resumption point can advance it past records neither has
//! landed, which breaks resumption without anything looking wrong. So the
//! second run must fail rather than wait.

use std::collections::BTreeSet;
use std::error::Error;

use application::{NextLock, Part, RunLock, RunLockError, Taken};

use domain::landing::{EntityKind, LandingStream, SourceName};
use infrastructure::FileRunLock;
use tempfile::TempDir;

type Fallible<T> = Result<T, Box<dyn Error>>;

fn stream(entity: &str) -> Fallible<LandingStream> {
    Ok(LandingStream::new(
        SourceName::try_from("hevy")?,
        EntityKind::try_from(entity)?,
    ))
}

#[test]
fn a_second_run_is_refused_immediately() {
    let directory = TempDir::new().expect("a temporary directory");
    let lock = FileRunLock::beside(&directory.path().join("fitness.db"));
    let workouts = stream("workouts").expect("a stream");

    let held = RunLock::try_acquire(&lock, &workouts).expect("the first run takes it");

    // Refused, not queued. An operator who starts extraction twice by mistake
    // wants to be told, not to wait.
    let refused =
        RunLock::try_acquire(&lock, &workouts).expect_err("the second run must be refused");
    assert_eq!(refused, RunLockError::Held);

    drop(held);
}

/// The guard releases on drop, so a completed run leaves the way clear.
#[test]
fn the_lock_is_released_when_the_run_ends() {
    let directory = TempDir::new().expect("a temporary directory");
    let lock = FileRunLock::beside(&directory.path().join("fitness.db"));
    let workouts = stream("workouts").expect("a stream");

    drop(RunLock::try_acquire(&lock, &workouts).expect("the first run takes it"));

    RunLock::try_acquire(&lock, &workouts).expect("a released lock is available again");
}

/// Streams lock independently. Collecting Hevy workouts and Hevy body
/// measurements are separate runs, and blocking one on the other would be a
/// restriction nothing asks for.
#[test]
fn streams_do_not_block_each_other() {
    let directory = TempDir::new().expect("a temporary directory");
    let lock = FileRunLock::beside(&directory.path().join("fitness.db"));

    let workouts =
        RunLock::try_acquire(&lock, &stream("workouts").expect("a stream")).expect("workouts");
    let measurements = RunLock::try_acquire(&lock, &stream("measurements").expect("a stream"))
        .expect("a different stream must not be blocked");

    drop(workouts);
    drop(measurements);
}

/// Two stores in different directories are two systems, not one.
#[test]
fn separate_databases_lock_separately() {
    let one = TempDir::new().expect("a temporary directory");
    let two = TempDir::new().expect("a temporary directory");
    let workouts = stream("workouts").expect("a stream");

    let first = FileRunLock::beside(&one.path().join("fitness.db"));
    let second = FileRunLock::beside(&two.path().join("fitness.db"));

    let held = RunLock::try_acquire(&first, &workouts).expect("the first store");
    RunLock::try_acquire(&second, &workouts).expect("a different store must not be blocked");
    drop(held);
}

fn part(kind: fn(LandingStream) -> Part, entity: &str) -> Fallible<Part> {
    Ok(kind(stream(entity)?))
}

/// **A part of `next` at a time**, whichever frontend starts the run (#398,
/// #559): a run over a part another holds takes none of its parts and is told
/// which are held, a run over other parts goes at the same time, and neither
/// blocks the walks inside it, which take their streams' locks.
#[test]
fn a_run_of_next_takes_all_its_parts_or_none_and_its_walks_are_not_blocked() {
    let directory = TempDir::new().expect("a temporary directory");
    let lock = FileRunLock::beside(&directory.path().join("fitness.db"));
    let workouts = part(Part::Normalising, "workouts").expect("a part");
    let measurements = part(Part::Normalising, "measurements").expect("a part");

    let Ok(Taken::All(held)) =
        NextLock::try_acquire(&lock, &BTreeSet::from([workouts.clone(), Part::Delivering]))
    else {
        panic!("the first run takes its parts");
    };
    match NextLock::try_acquire(
        &lock,
        &BTreeSet::from([workouts.clone(), measurements.clone()]),
    ) {
        Ok(Taken::Held(contended)) => assert_eq!(contended, BTreeSet::from([workouts.clone()])),
        _ => panic!("a run over a part held is told which"),
    }
    let Ok(Taken::All(beside)) = NextLock::try_acquire(&lock, &BTreeSet::from([measurements]))
    else {
        panic!("a run over other parts goes at the same time, so the refused run let go");
    };
    RunLock::try_acquire(&lock, &stream("workouts").expect("a stream"))
        .expect("a walk inside the run is not blocked by it");

    drop(held);
    drop(beside);
    assert!(matches!(
        NextLock::try_acquire(&lock, &BTreeSet::from([workouts])),
        Ok(Taken::All(_))
    ));
}

/// **Waiting on a part ends when the run holding it does**, which is how a
/// page joins a run started in the terminal.
#[test]
fn waiting_on_a_part_ends_when_its_run_does() {
    let directory = TempDir::new().expect("a temporary directory");
    let lock = FileRunLock::beside(&directory.path().join("fitness.db"));
    let parts = BTreeSet::from([Part::Delivering]);
    let Ok(Taken::All(held)) = NextLock::try_acquire(&lock, &parts) else {
        panic!("the first run takes its parts");
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        let waiting = NextLock::released(&lock, &parts);
        let ending = async {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            drop(held);
        };
        let (released, ()) = tokio::join!(waiting, ending);
        released.expect("released once the run ends");
    });
}

/// **A page can ask which of its parts a run holds without taking any**
/// (#467): it is told the ones held, and a run starting after it finds every
/// part free.
#[test]
fn which_parts_are_held_is_asked_without_taking_them() {
    let directory = TempDir::new().expect("a temporary directory");
    let lock = FileRunLock::beside(&directory.path().join("fitness.db"));
    let workouts = part(Part::Normalising, "workouts").expect("a part");
    let measurements = part(Part::Normalising, "measurements").expect("a part");
    let read = BTreeSet::from([workouts.clone(), measurements]);

    assert_eq!(lock.held(&read).expect("tried"), BTreeSet::new());
    let Ok(Taken::All(running)) = NextLock::try_acquire(&lock, &BTreeSet::from([workouts.clone()]))
    else {
        panic!("asking took nothing, so the run takes its parts");
    };
    assert_eq!(lock.held(&read).expect("tried"), BTreeSet::from([workouts]));

    drop(running);
    assert!(matches!(
        NextLock::try_acquire(&lock, &read),
        Ok(Taken::All(_))
    ));
}
