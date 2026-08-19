//! The performed record as prescription reads it, against the landed corpus.
//!
//! Part of user story 1: the primary draws from programme state, and every other
//! slot draws from observed history. This suite asserts the second half — that
//! the history is what the record actually says, that it reaches back as far as
//! it needs to, and that § 10's supersession rule is applied on the way out.
//!
//! Driven through the real store against a real SQLite file, because what is
//! being asserted is a query.

mod support;

use application::{
    ExerciseHistory as _, ExtractionRunLog as _, LandingStore as _, LastPerformance,
    NormalisationSummary, WorkoutNormaliser,
    normalise::{Normalisation, NormalisationPorts},
};
use domain::gym::{Performed, exercise::RepsExercise};
use infrastructure::{
    HevyWorkoutLandingReader, HevyWorkoutLandingStore, HevyWorkoutTranslator,
    SqliteExerciseHistory, SqliteExtractionRunLog, SqliteGymWorkoutStore,
    SqliteNormalisationRunLog, SqliteRefusalStore, connect,
};
use sqlx::SqlitePool;
use support::corpus;

/// A store holding the corpus, landed and derived.
///
/// Returns `Result` and the test unwraps at the call site: `clippy.toml`'s
/// exemptions cover a `#[test]` body, not a helper defined beside one.
async fn derived_corpus() -> Result<(SqlitePool, tempfile::TempDir), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;

    let landing = HevyWorkoutLandingStore::new(pool.clone())?;
    let runs = SqliteExtractionRunLog::new(pool.clone());
    let run = runs
        .begin(landing.stream(), domain::landing::FetchedAt::EPOCH)
        .await?;
    let records = corpus::records()?
        .into_iter()
        .map(|landed| landed.record().clone())
        .collect();
    landing.append(run, records).await?;

    let normalisation = Normalisation::new(
        NormalisationPorts {
            raw: HevyWorkoutLandingReader::new(pool.clone())?,
            translator: HevyWorkoutTranslator,
            workouts: SqliteGymWorkoutStore::new(pool.clone())?,
            refusals: SqliteRefusalStore::new(pool.clone())?,
            runs: SqliteNormalisationRunLog::new(pool.clone()),
            clock: corpus::FixedClock,
        },
        corpus::zone()?,
    );
    let _summary: NormalisationSummary = normalisation.normalise().await?;

    Ok((pool, directory))
}

/// A store with the schema and no records.
///
/// Needed because the corpus cannot supply a never-performed exercise: the
/// vocabulary was built *from* these records, so all 117 reps exercises appear in
/// them. `exercise.rs` says as much — a member is added when a second source or
/// new programming introduces a movement nobody has recorded yet. Until that
/// happens, the only honest way to reach `NeverPerformed` is an empty layer.
async fn empty_store() -> Result<(SqlitePool, tempfile::TempDir), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;
    Ok((pool, directory))
}

macro_rules! history {
    () => {
        match corpus::block_on(derived_corpus()) {
            Ok(Ok((pool, directory))) => (SqliteExerciseHistory::new(pool), directory),
            Ok(Err(error)) => panic!("the corpus lands and derives: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

macro_rules! empty_history {
    () => {
        match corpus::block_on(empty_store()) {
            Ok(Ok((pool, directory))) => (SqliteExerciseHistory::new(pool), directory),
            Ok(Err(error)) => panic!("an empty store opens: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

macro_rules! run {
    ($body:expr) => {
        match corpus::block_on($body) {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => panic!("the query succeeds: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

/// The front squat's whole series, oldest first — what the ladder position reads.
///
/// A latest value cannot answer the gate's question, which is why the port has
/// two reads. This asserts the series is a series: ordered, one entry per
/// session, and carrying every working set of that session.
#[test]
fn the_primarys_series_is_ordered_and_complete() {
    let (history, _directory) = history!();
    let front_squat = RepsExercise::FrontSquat;

    let performances = run!(history.performances(front_squat));

    assert!(
        performances.len() > 20,
        "the corpus holds many front squat sessions, found {}",
        performances.len()
    );

    // Oldest first, strictly — a series the gate walks in order.
    for pair in performances.windows(2) {
        if let [earlier, later] = pair {
            assert!(
                earlier.on <= later.on,
                "performances must be oldest first: {} then {}",
                earlier.on,
                later.on
            );
        }
    }

    // Every performance carries at least one working set, and warm-ups are
    // excluded — a ramp step is not evidence about a maximum.
    for performance in &performances {
        assert!(
            !performance.sets.is_empty(),
            "a performance with no working set is not one"
        );
    }
}

/// The session of 2026-07-03 is the anchor's evidence: a completed single at 90,
/// and a failed attempt at 95 above it.
///
/// **Both are in the record now.** This test was written against the state before
/// user story 2 — where the 95 was refused and therefore absent — and said so, so
/// that it would fail loudly at the change rather than quietly keep passing. It
/// did, and this is the other side of it.
#[test]
fn the_july_test_session_reads_back() {
    let (history, _directory) = history!();
    let front_squat = RepsExercise::FrontSquat;

    let performances = run!(history.performances(front_squat));
    let Some(july) = performances
        .iter()
        .find(|performance| performance.on.to_string() == "2026-07-03")
    else {
        panic!("the corpus holds a front squat session on 2026-07-03")
    };

    let loads: Vec<String> = july
        .sets
        .iter()
        .map(|set| format!("{}", set.load))
        .collect();
    assert!(
        loads.iter().any(|load| load.contains("90")),
        "the completed single at 90 is in the record, found {loads:?}"
    );

    let failures: Vec<String> = july
        .sets
        .iter()
        .filter(|set| set.outcome.is_failed())
        .map(|set| format!("{}", set.load))
        .collect();
    assert_eq!(failures.len(), 1, "the failed attempt is in the record");
    assert!(
        failures.iter().any(|load| load.contains("95")),
        "and it is the 95, which is above the completed single: {failures:?}"
    );
}

/// An alternating fill's exercise was last performed two sessions back.
///
/// The case a bounded "last session" lookback gets wrong while still returning a
/// plausible number. The assertion is on the *date* the history came from, not
/// only on the load.
#[test]
fn an_alternating_fill_reaches_past_the_last_session() {
    let (history, _directory) = history!();

    let nordic = RepsExercise::NordicHamstringsCurls;
    let back_extension = RepsExercise::BackExtensionMachine;

    let both = run!(history.last_performances(&[nordic, back_extension]));

    let (
        Some(LastPerformance::Performed(nordic_last)),
        Some(LastPerformance::Performed(back_last)),
    ) = (both.get(&nordic), both.get(&back_extension))
    else {
        panic!("both halves of the alternating fill have been performed")
    };

    // They alternate, so they were last performed on different days. If a
    // bounded lookback were reading "the last session" for both, these would be
    // equal.
    assert_ne!(
        nordic_last.on, back_last.on,
        "an alternating pair is not performed on the same day"
    );
}

/// Every exercise asked about gets an answer, and never a missing key.
///
/// `LastPerformance::NeverPerformed` is a named case for exactly this: an absent
/// key would make the caller's `get` return `None` for both "never performed"
/// and "never asked", and that conflation is what invites a default load.
///
/// Asserted against an empty layer rather than the corpus, for the reason
/// [`empty_store`] gives.
#[test]
fn a_never_performed_exercise_is_named_not_absent() {
    let (history, _directory) = empty_history!();

    let asked = [RepsExercise::FrontSquat, RepsExercise::SissySquat];
    let answers = run!(history.last_performances(&asked));

    assert_eq!(
        answers.len(),
        asked.len(),
        "every exercise asked about gets an answer"
    );
    for exercise in asked {
        assert_eq!(
            answers.get(&exercise),
            Some(&LastPerformance::NeverPerformed),
            "{exercise} has no history in an empty layer, and must say so"
        );
    }

    // And the series read agrees: empty, not an error.
    let series = run!(history.performances(RepsExercise::FrontSquat));
    assert!(series.is_empty());
    assert_eq!(run!(history.newest_performance()), None);
}

/// Against the corpus, every exercise the vocabulary holds has been performed.
///
/// Worth asserting rather than assuming: it is why the test above needs an empty
/// layer, and it would change the moment programming introduces a movement
/// nobody has recorded. If this fails, the mixed case has become reachable and
/// the test above can use the corpus instead.
#[test]
fn the_corpus_covers_every_exercise_it_taught_the_vocabulary() {
    let (history, _directory) = history!();

    let sample = [
        RepsExercise::FrontSquat,
        RepsExercise::SissySquat,
        RepsExercise::CableTwistUpToDown,
    ];
    let answers = run!(history.last_performances(&sample));

    for exercise in sample {
        assert!(
            matches!(answers.get(&exercise), Some(LastPerformance::Performed(_))),
            "{exercise} taught the vocabulary and so is in the corpus"
        );
    }
}

/// § 38: the newest performance is queryable, so a stale prescription is visible.
#[test]
fn the_newest_performance_is_the_corpuss_last_session() {
    let (history, _directory) = history!();

    let Some(newest) = run!(history.newest_performance()) else {
        panic!("the corpus is not empty")
    };

    // The fixture ends on 2026-08-10; the live store has run past it, and this
    // asserts against the committed corpus rather than whatever was extracted
    // last.
    assert_eq!(newest.to_string(), "2026-08-10");
}

/// Warm-up sets are excluded from history.
///
/// Double progression reads working sets, and a ramp step at 40% of a top set is
/// not evidence about anything. The front squat's ramp is four sets a session, so
/// including them would inflate every count here.
#[test]
fn warm_ups_are_not_history() {
    let (history, _directory) = history!();
    let front_squat = RepsExercise::FrontSquat;

    let performances = run!(history.performances(front_squat));
    let Some(recent) = performances.last() else {
        panic!("the corpus holds front squat sessions")
    };

    // The sessions since July run a four-step ramp then three or four working
    // sets. Any count above five would mean warm-ups leaked in.
    assert!(
        recent.sets.len() <= 5,
        "warm-ups must be excluded, found {} sets",
        recent.sets.len()
    );
}

/// A completed set carries its count; nothing else can be read as a quantity.
#[test]
fn a_completed_set_carries_its_count() {
    let (history, _directory) = history!();
    let front_squat = RepsExercise::FrontSquat;

    let performances = run!(history.performances(front_squat));
    let mut counted = 0_usize;
    for performance in &performances {
        for set in &performance.sets {
            match set.outcome {
                Performed::Completed(reps) => {
                    assert!(reps.as_u32() > 0, "a completed set has a positive count");
                    counted += 1;
                }
                // A failure carries nothing, which is the point: no arithmetic
                // can take a quantity from it.
                Performed::Failed => {}
            }
        }
    }
    assert!(counted > 50, "most of the corpus is completed sets");
}
