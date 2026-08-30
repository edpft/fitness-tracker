//! Performed against prescribed, through the store that holds both.
//!
//! The comparison itself is `domain`'s and its properties are asserted there
//! (`domain/tests/projection.rs`). What needs an adapter suite is the pairing:
//! which performance answers which prescription, and how the answer differs when
//! the record names the session and when it does not.
//!
//! **Both halves of the pairing are exercised against one store.** A published
//! session and an unpublished one are the same corpus workout and the same
//! issued prescription; the only difference is whether a delivery was recorded.
//! That is what makes the pair a test of the pairing rather than of two
//! unrelated fixtures.

mod support;

use application::{
    DeliveryReference, DestinationName, PrescriptionDeliveryStore as _, WorkoutPrescriber as _,
    compare::{Comparing, ComparisonPorts, Pairing},
    prescribe::{Prescribing, PrescriptionPorts},
};
use infrastructure::{
    SqliteExerciseHistory, SqliteGenerationParameterStore, SqlitePerformedWorkoutReader,
    SqlitePrescribedWorkoutStore, SqlitePrescriptionDeliveryStore, SqliteProgrammeStore,
};
use jiff::civil::Date;
use sqlx::SqlitePool;
use support::{corpus, programme, store};

type Prescriber = Prescribing<
    SqliteExerciseHistory,
    SqliteProgrammeStore,
    SqliteGenerationParameterStore,
    SqlitePrescribedWorkoutStore,
    SqlitePrescriptionDeliveryStore,
>;

type Comparer = Comparing<SqlitePrescribedWorkoutStore, SqlitePerformedWorkoutReader>;

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// The block opens on the Monday after its 3 July entry test.
const BLOCK_START: Date = Date::constant(2026, 7, 6);

/// The block's first Friday: prescribed heavy, and trained in the corpus.
const GATING: Date = Date::constant(2026, 7, 10);

/// The Hevy routine the corpus's Heavy sessions were performed against.
const HEAVY_ROUTINE: &str = "11437699-cb70-4e0e-a77b-caa9fd5cdb24";

async fn ready() -> Fallible<(Prescriber, Comparer, tempfile::TempDir, SqlitePool)> {
    let (directory, pool) = store::with_programme(programme::programme_from(BLOCK_START)?).await?;
    let prescriber = Prescribing::new(PrescriptionPorts {
        history: SqliteExerciseHistory::new(pool.clone()),
        programmes: SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
        parameters: SqliteGenerationParameterStore::new(pool.clone()),
        prescriptions: SqlitePrescribedWorkoutStore::new(pool.clone(), "Europe/London".to_owned()),
        lifecycle: SqlitePrescriptionDeliveryStore::new(pool.clone()),
    });
    let comparer = Comparing::new(ComparisonPorts {
        prescriptions: SqlitePrescribedWorkoutStore::new(pool.clone(), "Europe/London".to_owned()),
        workouts: SqlitePerformedWorkoutReader::new(pool.clone()),
    });
    Ok((prescriber, comparer, directory, pool))
}

macro_rules! ready {
    () => {
        match corpus::block_on(ready()) {
            Ok(Ok(ready)) => ready,
            Ok(Err(error)) => panic!("the corpus lands, derives and authors: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

macro_rules! run {
    ($body:expr) => {
        match corpus::block_on($body) {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => panic!("the operation succeeds: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

/// Whatever the operation answered, error included.
macro_rules! attempt {
    ($body:expr) => {
        match corpus::block_on($body) {
            Ok(answer) => answer,
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

/// Issue the session for a date, and record what the destination called it.
fn publish(prescriber: &Prescriber, pool: &SqlitePool, date: Date, reference: &str) {
    let issued = run!(prescriber.prescribe(date));
    let deliveries = SqlitePrescriptionDeliveryStore::new(pool.clone());
    run!(async {
        deliveries
            .record(
                issued.id,
                &DestinationName::try_from("hevy".to_owned())?,
                &DeliveryReference::try_from(reference.to_owned())?,
                jiff::Timestamp::UNIX_EPOCH,
            )
            .await
            .map_err(Box::<dyn std::error::Error>::from)
    });
}

/// Move the block's first Friday session to the Saturday morning after it.
fn trained_the_next_morning(pool: &SqlitePool) {
    let moved = run!(async {
        sqlx::query!(
            "UPDATE gym_workout \
             SET started_at_utc = replace(started_at_utc, '2026-07-10T18', '2026-07-11T09') \
             WHERE started_at_utc LIKE '2026-07-10T18%'"
        )
        .execute(pool)
        .await
    });
    assert_eq!(
        moved.rows_affected(),
        1,
        "the Friday session is the one that moved"
    );
}

/// A published session is found by its id, whatever day it was trained on.
///
/// **The case a comparison most needs.** The session was moved to the Saturday
/// morning after it, so nothing about the day it was prescribed for locates it —
/// and it is still found, because the record names the session it answered.
#[test]
fn a_published_session_is_compared_through_its_id() {
    let (prescriber, comparer, _directory, pool) = ready!();
    publish(&prescriber, &pool, GATING, HEAVY_ROUTINE);
    trained_the_next_morning(&pool);

    let comparison = run!(comparer.compare(GATING));

    assert_eq!(comparison.prescribed_for, GATING);
    assert_eq!(
        comparison.performed_on,
        Date::constant(2026, 7, 11),
        "the session was trained the next morning, and the comparison says so"
    );
    let Ok(reference) = DeliveryReference::try_from(HEAVY_ROUTINE.to_owned()) else {
        panic!("the routine id is a reference")
    };
    assert_eq!(comparison.pairing, Pairing::Published(reference));
}

/// An unpublished session is paired with the day's training, and says so.
///
/// The same store and the same prescription as above, with no delivery recorded.
/// The pairing is an assumption rather than a fact, which is why it is a variant
/// the caller has to read rather than a `None` it can ignore.
#[test]
fn an_unpublished_session_is_compared_by_the_day() {
    let (prescriber, comparer, _directory, _pool) = ready!();
    run!(prescriber.prescribe(GATING));

    let comparison = run!(comparer.compare(GATING));

    assert_eq!(comparison.pairing, Pairing::Dated);
    assert_eq!(comparison.performed_on, GATING);
}

/// Two sessions on a day and nothing naming the prescription is refused.
///
/// **Not resolved by taking the first.** A comparison run against the wrong
/// workout reports divergences that are really a mismatch, which reads exactly
/// like a session that went badly. Declining to answer is the honest result, and
/// publishing the session is the remedy the error names.
#[test]
fn two_sessions_on_a_day_are_refused_rather_than_guessed_between() {
    let (prescriber, comparer, _directory, pool) = ready!();
    run!(prescriber.prescribe(GATING));

    // The Monday after, moved onto the Friday, so the day holds two sessions.
    let moved = run!(async {
        sqlx::query!(
            "UPDATE gym_workout \
             SET started_at_utc = replace(started_at_utc, '2026-07-13T18', '2026-07-10T20') \
             WHERE started_at_utc LIKE '2026-07-13T18%'"
        )
        .execute(&pool)
        .await
    });
    assert_eq!(moved.rows_affected(), 1, "the Monday session moved");

    match attempt!(comparer.compare(GATING)) {
        Err(application::ComparisonError::AmbiguousDay { date, count }) => {
            assert_eq!(date, GATING);
            assert_eq!(count, 2);
        }
        other => panic!("two sessions on the day is refused, got {other:?}"),
    }
}

/// A date nobody prescribed for has nothing to compare against.
///
/// Deliberately not "so one was derived": issuing a prescription in order to
/// have something to compare with would invent the expectation being tested.
#[test]
fn a_date_nobody_prescribed_for_is_refused() {
    let (_prescriber, comparer, _directory, _pool) = ready!();

    match attempt!(comparer.compare(GATING)) {
        Err(application::ComparisonError::NothingIssued { date }) => assert_eq!(date, GATING),
        other => panic!("an unprescribed date is refused, got {other:?}"),
    }
}

/// A session prescribed and not trained is refused, and it is not a fault.
#[test]
fn a_prescribed_session_nobody_performed_is_refused() {
    let (prescriber, comparer, _directory, _pool) = ready!();

    // Programmed, inside the block, and absent from the corpus.
    let untrained = Date::constant(2026, 7, 24);
    run!(prescriber.prescribe(untrained));

    match attempt!(comparer.compare(untrained)) {
        Err(application::ComparisonError::NotPerformed { date }) => assert_eq!(date, untrained),
        other => panic!("an unperformed session is refused, got {other:?}"),
    }
}
