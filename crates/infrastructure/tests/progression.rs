//! The failure mechanism, wired up: a real miss holds a real ladder.
//!
//! User story 3 through its ports. The mechanism's arithmetic is asserted in
//! `crates/domain/tests/progression.rs` — nine hand-built sequences including the
//! worked example load for load — and what needs a store is the wiring: that the
//! *record* decides which rung a date gets, and that the calendar week it falls in
//! does not.
//!
//! **The corpus supplies a genuine miss**, which is why this needs no synthetic
//! session. The front squat of Friday 2026-07-03 was 95kg for zero repetitions,
//! and user story 2 made it a failed attempt in the record. Friday is the fixture
//! programme's gating day, so a block containing that Friday has a missed gating
//! top set in its first week.

mod support;

use application::{
    WorkoutPrescriber as _,
    prescribe::{Prescribing, PrescriptionPorts},
};
use domain::prescription::{PrescribedItem, SessionRole, SlotId, WeekIndex, WeekKind};
use infrastructure::{
    SqliteExerciseHistory, SqliteGenerationParameterStore, SqlitePrescribedWorkoutStore,
    SqliteProgrammeStore,
};
use jiff::civil::Date;
use support::{corpus, programme, store};

type Prescriber = Prescribing<
    SqliteExerciseHistory,
    SqliteProgrammeStore,
    SqliteGenerationParameterStore,
    SqlitePrescribedWorkoutStore,
>;

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// A store whose block opens on 2026-06-29, so the 3rd of July is inside it.
async fn ready() -> Fallible<(Prescriber, tempfile::TempDir)> {
    let start = Date::constant(2026, 6, 29);
    let (directory, pool) = store::with_programme(programme::programme_from(start)?).await?;
    Ok((
        Prescribing::new(PrescriptionPorts {
            history: SqliteExerciseHistory::new(pool.clone()),
            programmes: SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
            parameters: SqliteGenerationParameterStore::new(pool.clone()),
            prescriptions: SqlitePrescribedWorkoutStore::new(pool, "Europe/London".to_owned()),
        }),
        directory,
    ))
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

/// The primary's top set in an issued session.
macro_rules! top_set {
    ($prescriber:expr, $date:expr) => {{
        let issued = run!($prescriber.prescribe($date));
        let Some(PrescribedItem::Exercise { exercise, .. }) =
            issued.workout.shape().item_for(SlotId::KneeDominant)
        else {
            panic!("{} issues the primary slot", $date)
        };
        let domain::prescription::PrescribedExercise::ForReps { sets, .. } = exercise else {
            panic!("the front squat is counted in repetitions")
        };
        // The ramp is warm-ups, then the top set, then the back-offs. The top set
        // is the first working one.
        let Some(top) = sets.iter().find(|set| !set.warmup) else {
            panic!("the primary has a working set")
        };
        // Copied out before `issued` moves, since the sets borrow from it.
        let load = top.prescription.load();
        (issued, load)
    }};
}

/// The ladder holds after a missed gating top set, and the calendar does not.
///
/// **The sharp assertion.** 2026-07-10 is the *second* Friday of this block, so the
/// calendar places it in week 2 — and the record says the first Friday's top set
/// was missed, so the ladder is still at week 1. What is prescribed is week 1's
/// load. Before US3 this was week 2's, which is what makes the two numbers
/// different enough to tell apart.
#[test]
fn a_missed_gating_session_holds_the_prescribed_ladder() {
    let (prescriber, _directory) = ready!();
    let (issued, load) = top_set!(&prescriber, Date::constant(2026, 7, 10));

    assert_eq!(issued.workout.session_role(), SessionRole::Heavy);
    assert_eq!(
        issued.workout.week(),
        WeekKind::Climbing(WeekIndex::FIRST.next()),
        "the calendar still says which week of the block this is"
    );

    let (Ok(anchor), Ok(parameters)) = (programme::anchor(), programme::parameters()) else {
        panic!("the fixtures build")
    };
    let Ok(programme) = programme::programme_from(Date::constant(2026, 6, 29)) else {
        panic!("the programme builds")
    };
    let Ok(ladder) = programme.ladder(&parameters) else {
        panic!("the ladder builds")
    };
    let held = ladder.heavy_top_set(anchor.load(), WeekIndex::FIRST, parameters.plate_increment);
    let advanced = ladder.heavy_top_set(
        anchor.load(),
        WeekIndex::FIRST.next(),
        parameters.plate_increment,
    );
    assert_ne!(held, advanced, "the two rungs must be distinguishable");

    assert_eq!(
        load,
        held.map(domain::gym::Load::Absolute),
        "a miss re-issues the rung, and 95kg for zero reps on 3 July was a miss"
    );
}

/// A session the gate does not watch does not move the ladder (US3-10).
///
/// **Counted out, because the numbers are what carry the assertion.** By Monday
/// 2026-07-13 this block has had two Fridays — 3 July missed, 10 July completed —
/// so the ladder held at week one and then advanced to week two. It has also had a
/// Monday, 6 July, which was trained and completed. If the light session gated too
/// the position would be week *three*.
///
/// So asserting week two rather than week three is the assertion that only the
/// gating role gates, and it is checked through the load actually prescribed rather
/// than through the mechanism's own state.
#[test]
fn only_the_gating_role_gates() {
    let (prescriber, _directory) = ready!();
    let (issued, light) = top_set!(&prescriber, Date::constant(2026, 7, 13));
    assert_eq!(issued.workout.session_role(), SessionRole::Light);

    let (Ok(anchor), Ok(parameters)) = (programme::anchor(), programme::parameters()) else {
        panic!("the fixtures build")
    };
    let Ok(programme) = programme::programme_from(Date::constant(2026, 6, 29)) else {
        panic!("the programme builds")
    };
    let Ok(ladder) = programme.ladder(&parameters) else {
        panic!("the ladder builds")
    };
    let light_of = |week: WeekIndex| {
        ladder
            .light_top_set(
                anchor.load(),
                week,
                parameters.plate_increment,
                parameters.light_of_heavy,
            )
            .map(domain::gym::Load::Absolute)
    };

    let gated = light_of(WeekIndex::FIRST.next());
    let ungated = light_of(WeekIndex::FIRST.next().next());
    assert_ne!(gated, ungated, "the two rungs must be distinguishable");
    assert_eq!(
        light, gated,
        "two Fridays have gone by, one of them missed, and the Monday between them \
         is not a rung"
    );
}

/// Asking twice does not advance the position.
///
/// **The regression test against reintroducing stored position state.** The ladder
/// is walked out of the record on every read, so two reads of one date are two
/// reads of the same record — and the second returns what was issued rather than
/// issuing again.
#[test]
fn asking_twice_does_not_double_advance() {
    let (prescriber, _directory) = ready!();
    let date = Date::constant(2026, 7, 10);

    let first = run!(prescriber.prescribe(date));
    let second = run!(prescriber.prescribe(date));

    assert!(first.freshly_issued);
    assert!(!second.freshly_issued, "the second read issues nothing");
    assert_eq!(
        first.workout.shape(),
        second.workout.shape(),
        "and the loads are the ones already issued"
    );
}
