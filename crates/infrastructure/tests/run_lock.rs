//! FR-010: only one extraction run at a time.
//!
//! Two runs sharing a resumption point can advance it past records neither has
//! landed, which breaks resumption without anything looking wrong. So the
//! second run must fail rather than wait.

use std::error::Error;

use application::{RunLock, RunLockError};
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

    let held = lock.try_acquire(&workouts).expect("the first run takes it");

    // Refused, not queued. An operator who starts extraction twice by mistake
    // wants to be told, not to wait.
    let refused = lock
        .try_acquire(&workouts)
        .expect_err("the second run must be refused");
    assert_eq!(refused, RunLockError::Held);

    drop(held);
}

/// The guard releases on drop, so a completed run leaves the way clear.
#[test]
fn the_lock_is_released_when_the_run_ends() {
    let directory = TempDir::new().expect("a temporary directory");
    let lock = FileRunLock::beside(&directory.path().join("fitness.db"));
    let workouts = stream("workouts").expect("a stream");

    drop(lock.try_acquire(&workouts).expect("the first run takes it"));

    lock.try_acquire(&workouts)
        .expect("a released lock is available again");
}

/// Streams lock independently. Collecting Hevy workouts and Hevy body
/// measurements are separate runs, and blocking one on the other would be a
/// restriction nothing asks for.
#[test]
fn streams_do_not_block_each_other() {
    let directory = TempDir::new().expect("a temporary directory");
    let lock = FileRunLock::beside(&directory.path().join("fitness.db"));

    let workouts = lock
        .try_acquire(&stream("workouts").expect("a stream"))
        .expect("workouts");
    let measurements = lock
        .try_acquire(&stream("measurements").expect("a stream"))
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

    let held = first.try_acquire(&workouts).expect("the first store");
    second
        .try_acquire(&workouts)
        .expect("a different store must not be blocked");
    drop(held);
}
