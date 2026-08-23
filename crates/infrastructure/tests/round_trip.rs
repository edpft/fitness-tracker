//! The adapter half of the round trip: a stored performance read back and
//! projected.
//!
//! The projection itself is a `domain` function and its property is asserted
//! there, over sessions generated against the template
//! (`domain/tests/projection.rs`). What needs an adapter suite is reading a
//! whole `GymWorkout` back out of the five tables it was written to, and what
//! the projection loses when it does.
//!
//! **Nothing here compares the record against a regenerated prescription.** The
//! corpus predates the template: it records a programme run by hand whose
//! template changed while it ran, so agreement or disagreement with it measures
//! history rather than the model. The forward invariant — a session prescribed
//! and then performed satisfies its prescription — is a property of sessions run
//! on this platform, and none exist yet.

mod support;

use application::{
    PerformedWorkoutReader as _,
    prescribe::{Prescribing, PrescriptionPorts},
};
use domain::{
    gym::{GymWorkout, Kg, Load, NonEmpty, RepCount},
    prescription::{
        PrescribedExercise, PrescribedItem, PrescribedSet, ProjectionGap, SlotId, Target,
        WorkoutShape, project, satisfies,
    },
};
use infrastructure::{
    SqliteExerciseHistory, SqliteGenerationParameterStore, SqlitePerformedWorkoutReader,
    SqlitePrescribedWorkoutStore, SqliteProgrammeStore,
};
use jiff::civil::Date;
use sqlx::SqlitePool;
use support::{corpus, store};

type Prescriber = Prescribing<
    SqliteExerciseHistory,
    SqliteProgrammeStore,
    SqliteGenerationParameterStore,
    SqlitePrescribedWorkoutStore,
>;

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// The store, a reader over it and a prescriber against it.
async fn ready() -> Fallible<(SqlitePerformedWorkoutReader, Prescriber, tempfile::TempDir)> {
    let (directory, pool): (tempfile::TempDir, SqlitePool) = store::derived_and_authored().await?;
    let reader = SqlitePerformedWorkoutReader::new(pool.clone());
    let prescriber = Prescribing::new(PrescriptionPorts {
        history: SqliteExerciseHistory::new(pool.clone()),
        programmes: SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
        parameters: SqliteGenerationParameterStore::new(pool.clone()),
        prescriptions: SqlitePrescribedWorkoutStore::new(pool, "Europe/London".to_owned()),
    });
    Ok((reader, prescriber, directory))
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

/// The one workout performed on a date.
///
/// A macro rather than a function for the reason `run!` is one: `panic` is
/// `forbid` and clippy's test exemption covers a `#[test]` body, not a helper
/// defined alongside it. Fragmentation would show up here as more than one
/// workout, and does not on these dates.
macro_rules! performance {
    ($reader:expr, $date:expr) => {{
        let workouts: Vec<GymWorkout> = run!($reader.between($date, $date));
        match workouts.into_iter().next() {
            Some(workout) => workout,
            None => panic!("{} has a performed session", $date),
        }
    }};
}

/// A date's performance projected, against the prescription generated for it.
/// SC-010f: the 95kg failure projects with the gap that names it.
///
/// **The sharpest loss in the round trip, and it is new information.** The
/// performed record holds the load that was on the bar and nothing about the
/// repetitions being attempted, because nothing recorded them. So the projection
/// cannot say what the set was for, and says so rather than inventing a count —
/// which is what told us the performed model cannot fully describe a missed set.
#[test]
fn a_failed_attempt_projects_a_gap() {
    let (reader, _prescriber, _directory) = ready!();
    let performed: GymWorkout = performance!(&reader, Date::constant(2026, 7, 3));
    let projection = project(&performed);

    let unknown: Vec<&ProjectionGap> = projection
        .gaps
        .iter()
        .filter(|gap| matches!(gap, ProjectionGap::IntendedMeasureUnknown { .. }))
        .collect();
    assert_eq!(unknown.len(), 1, "one attempt, one gap");

    let Some(ProjectionGap::IntendedMeasureUnknown { load, .. }) = unknown.first().copied() else {
        panic!("the gap was just counted")
    };
    assert_eq!(
        *load,
        Load::Absolute(Kg::from_grams(95_000)),
        "the load survives the projection; the count is what does not"
    );
}

/// SC-010d: satisfaction is direction-aware.
///
/// A performed six satisfies a prescribed four-to-six; a prescribed six is not
/// satisfied by a performed four-to-six. Only one of the two is an instruction,
/// which is why equality on `WorkoutShape` is the wrong relation and this is a
/// relation rather than a comparison.
#[test]
fn satisfaction_is_direction_aware() {
    let (Ok(four), Ok(six)) = (reps(4), reps(6)) else {
        panic!("both are repetition counts")
    };
    let Ok(two) = reps(2) else {
        panic!("two is a repetition count")
    };
    let Ok(exact) = shape_of(Target::Exactly(six)) else {
        panic!("an exact six builds")
    };
    let Ok(range) = shape_of(Target::spanning(four, two)) else {
        panic!("a shape is built from the four-to-six range")
    };

    assert_eq!(
        satisfies(&exact, &range),
        Vec::new(),
        "a performed six satisfies a prescribed four-to-six"
    );
    assert!(
        !satisfies(&range, &exact).is_empty(),
        "a performed four-to-six does not satisfy a prescribed six"
    );
    // And the ordinary case still holds in both directions.
    assert_eq!(satisfies(&exact, &exact), Vec::new());
}

fn reps(count: u32) -> Fallible<RepCount> {
    Ok(RepCount::new(count)?)
}

/// A one-item shape carrying one set, so a measure can be compared in isolation.
fn shape_of(measure: Target<RepCount>) -> Fallible<WorkoutShape> {
    let set = PrescribedSet::fixed(Load::Absolute(Kg::from_grams(60_000)), measure);
    Ok(WorkoutShape::new(NonEmpty::of(
        PrescribedItem::Exercise {
            slot: SlotId::KneeDominant,
            exercise: PrescribedExercise::ForReps {
                exercise: domain::gym::exercise::RepsExercise::try_from("front-squat".to_owned())?,
                sets: NonEmpty::of(set, Vec::new()),
            },
        },
        Vec::new(),
    )))
}
