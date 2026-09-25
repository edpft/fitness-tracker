//! A test as a programme in its own right, end to end (decision 0013).
//!
//! **What a test week prescribes is a question about the record**, so these go
//! through the store with the corpus in it, which is why this file is at the
//! adapter's ring rather than in `application`.
//!
//! One test went with the document format on 2026-09-06: a `gating_role` on a
//! test was a field the reader had to refuse, and `Shape::Test` has nowhere to
//! put one.
//!
//! The week under test runs the Monday and the Friday, and only one of them is
//! the test:
//!
//! ```text
//! Monday 31 August    the previous programme's light session
//! Friday 4 September  the test
//! ```

mod support;

use application::{
    ExtractionRunLog as _, LandingStore as _, NormalisationSummary, PlanAuthor as _,
    WorkoutNormaliser as _, WorkoutPrescriber as _,
    normalise::{Normalisation, NormalisationPorts},
    prescribe::{Authoring, Prescribing, PrescriptionPorts},
};
use domain::prescription::{
    Anchor, AnchorProvenance, DerivedFrom, EntryTest, PrescribedItem, SlotId, WeekKind,
    authored::Shape,
};
use domain::provider::{ExternalProgramme, ProvidedFrom};
use domain::schedule::{Relative, SessionRole};
use infrastructure::{
    HevySessionAccountReader, HevySessionTranslator, HevyWorkoutLandingStore,
    SqliteExerciseHistory, SqliteExtractionRunLog, SqliteGenerationParameterStore,
    SqliteGymMesocycleStore, SqliteGymSessionStore, SqliteNormalisationRunLog, SqlitePlanStore,
    SqlitePrescribedWorkoutStore, SqlitePrescriptionDeliveryStore, SqliteRefusalStore, connect,
};
use jiff::civil::Date;
use sqlx::SqlitePool;
use support::{corpus, programme};

type Prescriber = Prescribing<
    SqliteExerciseHistory,
    SqliteGymMesocycleStore,
    SqliteGenerationParameterStore,
    SqlitePrescribedWorkoutStore,
    SqlitePrescriptionDeliveryStore,
>;

/// A store holding the corpus, derived, with the fixture programme authored and
/// a standalone test in the week after it.
///
/// The fixture block runs eight weeks from 2026-07-06, so it ends on Sunday
/// 30 August and this week is adjacent rather than overlapping — which the
/// overlap rule would otherwise refuse.
async fn ready() -> Result<(Prescriber, tempfile::TempDir), Box<dyn std::error::Error>> {
    let (parameters, directory, pool) = corpus_store().await?;

    let test = test_programme()?;
    Authoring::new(
        SqlitePlanStore::new(pool.clone(), corpus::zone()?),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(
        &programme::named_plan("entry-test", vec![test])?,
        &parameters,
    )
    .await?;

    Ok((
        Prescribing::new(PrescriptionPorts {
            history: SqliteExerciseHistory::new(pool.clone()),
            programmes: SqliteGymMesocycleStore::new(pool.clone(), corpus::zone()?),
            parameters: SqliteGenerationParameterStore::new(pool.clone()),
            prescriptions: SqlitePrescribedWorkoutStore::new(
                pool.clone(),
                "Europe/London".to_owned(),
            ),
            lifecycle: SqlitePrescriptionDeliveryStore::new(pool),
        }),
        directory,
    ))
}

/// The corpus, landed and derived, with the fixture linear programme authored.
///
/// Shared by every part of this file that wants a predecessor: what succeeds the
/// fixture block differs, and everything before that does not.
async fn corpus_store() -> Result<
    (
        domain::prescription::GenerationParameters,
        tempfile::TempDir,
        SqlitePool,
    ),
    Box<dyn std::error::Error>,
> {
    let (parameters, directory, pool) = landed_store().await?;
    Authoring::new(
        SqlitePlanStore::new(pool.clone(), corpus::zone()?),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(&programme::as_plan(programme::programme()?)?, &parameters)
    .await?;

    Ok((parameters, directory, pool))
}

/// The corpus, landed and derived, and **nothing authored**.
///
/// The store a plan's first week sees: a record of what has been trained, and no
/// programme before this one to inherit anything from.
async fn landed_store() -> Result<
    (
        domain::prescription::GenerationParameters,
        tempfile::TempDir,
        SqlitePool,
    ),
    Box<dyn std::error::Error>,
> {
    let directory = tempfile::tempdir()?;
    let pool: SqlitePool = connect(&directory.path().join("test.db")).await?;
    // A block's calendar is rebuilt from the operator's week on every read
    // (issue #63), so a store with no week in it cannot hold a plan.
    programme::record_the_week(&pool).await?;

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
            raw: HevySessionAccountReader::new(pool.clone())?,
            translator: HevySessionTranslator,
            workouts: SqliteGymSessionStore::new(pool.clone())?,
            refusals: SqliteRefusalStore::new(pool.clone(), HevyWorkoutLandingStore::STREAM)?,
            runs: SqliteNormalisationRunLog::new(pool.clone()),
            clock: corpus::FixedClock,
        },
        corpus::zone()?,
    );
    let _summary: NormalisationSummary = normalisation.normalise().await?;

    Ok((programme::parameters()?, directory, pool))
}

/// A test week, authored in its own right.
///
/// **It states its seventeen slots like any other programme.** Until 2026-09-06
/// a test document could name only what changed and take the rest from the
/// programme before it, which the store had to be asked about at authoring time.
/// The questions ask every slot unconditionally, so there was never anything
/// left to inherit — and the fixture fills here are the same ones the programme
/// before it was authored with.
fn test_programme() -> Result<domain::prescription::GymMesocycle, Box<dyn std::error::Error>> {
    let answers = programme::authored(
        Date::constant(2026, 8, 31),
        Shape::Test {
            reps: domain::measure::RepCount::new(1)?,
            // A week that exists only to measure a lift before something else
            // begins was written by nobody.
            provided: None,
            // Nothing asserted: what the lift is at by then is read off the
            // record, which is the ordinary case.
            asserted: None,
        },
    )?;
    Ok(programme::authoring(answers, &[])??)
}

macro_rules! prescriber {
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

const fn test_day() -> Date {
    // Friday of the test week: the heavy session, and the test itself.
    Date::constant(2026, 9, 4)
}

const fn other_day() -> Date {
    // Monday of the test week: the previous programme's light session.
    Date::constant(2026, 8, 31)
}

/// The heavy session is a ramp and then three attempts, the last at the target.
///
/// **No top set and no back-offs**, which is what separates a test from a
/// climbing week, and nothing past the target (#136): it is a guess, so the two
/// attempts below it are what a miss leaves behind.
#[test]
fn the_heavy_session_is_the_test() {
    let (prescriber, _directory) = prescriber!();
    let issued = run!(prescriber.prescribe(test_day()));

    assert_eq!(issued.workout.week(), WeekKind::Test);
    assert!(
        matches!(issued.workout.derived_from(), DerivedFrom::Target(_)),
        "a test derives from what the record put it at, not from an anchor"
    );

    let Some(PrescribedItem::Exercise { exercise, .. }) =
        issued.workout.shape().item_for(SlotId::KneeDominant)
    else {
        panic!("the tested lift is a single exercise")
    };
    let domain::prescription::PrescribedExercise::ForReps { sets, .. } = exercise else {
        panic!("the front squat is counted in repetitions")
    };

    let warmups = sets.iter().filter(|set| set.warmup).count();
    let working: Vec<_> = sets.iter().filter(|set| !set.warmup).collect();
    assert_eq!(warmups, 4, "the authored ramp is four steps");
    assert_eq!(working.len(), 3, "three attempts, and nothing after them");
    // **Every attempt states its load**, and only the last asks for
    // everything: the two below it are steps toward the target.
    assert!(
        working.iter().all(|set| set.prescription.load().is_some()),
        "every attempt names its load"
    );
    let efforts: Vec<_> = working
        .iter()
        .map(|set| set.prescription.effort())
        .collect();
    assert_eq!(
        efforts,
        vec![None, None, Some(domain::gym::Rir::Zero)],
        "nothing left in reserve is asked of the target alone"
    );
}

/// The other session of the week is the previous programme's, not a second test.
#[test]
fn the_light_session_is_the_predecessors() {
    let (prescriber, _directory) = prescriber!();
    let issued = run!(prescriber.prescribe(other_day()));

    let Some(PrescribedItem::Exercise { exercise, .. }) =
        issued.workout.shape().item_for(SlotId::KneeDominant)
    else {
        panic!("the primary is a single exercise")
    };
    let domain::prescription::PrescribedExercise::ForReps { sets, .. } = exercise else {
        panic!("the front squat is counted in repetitions")
    };

    let working: Vec<_> = sets.iter().filter(|set| !set.warmup).collect();
    assert!(
        working.len() > 1,
        "a top set and its back-offs, not one attempt: got {} working sets",
        working.len()
    );
    assert!(
        working.iter().all(|set| set.prescription.load().is_some()),
        "every working set of an ordinary session carries a load"
    );
}

/// A test week runs the whole template, not just the lift being measured.
///
/// **What it is not is a session with one exercise in it.** The heavy day is the
/// attempt and everything around it is the programme's ordinary seventeen slots,
/// which is what makes a test week a week rather than a measurement.
#[test]
fn a_test_week_issues_every_slot() {
    let (prescriber, _directory) = prescriber!();
    let issued = run!(prescriber.prescribe(test_day()));

    // The plyometric slot is static — authored outright, no history read — so it
    // is derivable whatever the record holds, which makes it the one that can
    // assert the *exercise* rather than merely the slot's presence.
    let Some(PrescribedItem::Exercise { exercise, .. }) =
        issued.workout.shape().item_for(SlotId::Plyometric)
    else {
        panic!("the plyometric slot is issued, inherited from the block before it")
    };
    assert_eq!(
        exercise.exercise_key(),
        "pogo",
        "the plyometric fill this test week was authored with"
    );

    // Every other slot is either issued or reported. A slot that inherited
    // nothing would be neither: it would be silently absent, which is the
    // failure FR-011 exists to make impossible.
    for slot in [
        SlotId::UpperPush,
        SlotId::UpperPull,
        SlotId::HipDominant,
        SlotId::HandstandHold,
        SlotId::GroinStretch,
    ] {
        assert!(
            issued.workout.shape().item_for(slot).is_some()
                || issued
                    .underivable
                    .iter()
                    .any(|missing| missing.slot == slot),
            "{slot} is either issued or reported, never silently absent"
        );
    }
}

// ---------------------------------------------------------------------------
// A block that measures its own entry (decision 0016, as amended).

/// A block that measures its own entry, in the week after the fixture block.
///
/// **Ten phase weeks, and eleven calendar weeks.** The number counts phases
/// whether or not there is an entry test; the week in front is added by the
/// presence of the entry test and by nothing else.
fn autumn_block() -> Result<domain::prescription::GymMesocycle, Box<dyn std::error::Error>> {
    let answers = programme::authored(
        Date::constant(2026, 8, 31),
        Shape::Block {
            gating: SessionRole::new(Relative::Higher, Relative::Lower),
            weeks: 10,
            entry_test: Some(EntryTest::new(
                domain::measure::RepCount::new(3)?,
                Some("60".to_owned().try_into()?),
                // What the operator expects to lift, because nothing before this
                // block measured the lift. Week one finds out.
                Some(Anchor::new(
                    "90".to_owned().try_into()?,
                    None,
                    AnchorProvenance::Asserted,
                    Date::constant(2026, 7, 3),
                )?),
            )?),
        },
    )?;
    Ok(programme::authoring(answers, &[])??)
}

/// A store with the fixture block, and an autumn block that tests its own entry.
async fn with_block() -> Result<(Prescriber, tempfile::TempDir), Box<dyn std::error::Error>> {
    let (_, directory, pool) = corpus_store().await?;
    let parameters = programme::parameters()?;
    let block = autumn_block()?;
    Authoring::new(
        SqlitePlanStore::new(pool.clone(), corpus::zone()?),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(&programme::named_plan("autumn", vec![block])?, &parameters)
    .await?;

    Ok((
        Prescribing::new(PrescriptionPorts {
            history: SqliteExerciseHistory::new(pool.clone()),
            programmes: SqliteGymMesocycleStore::new(pool.clone(), corpus::zone()?),
            parameters: SqliteGenerationParameterStore::new(pool.clone()),
            prescriptions: SqlitePrescribedWorkoutStore::new(
                pool.clone(),
                "Europe/London".to_owned(),
            ),
            lifecycle: SqlitePrescriptionDeliveryStore::new(pool),
        }),
        directory,
    ))
}

macro_rules! blocked {
    () => {
        match corpus::block_on(with_block()) {
            Ok(Ok(ready)) => ready,
            Ok(Err(error)) => panic!("the corpus lands, derives and authors: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

/// The block's first week is its entry test, ramped toward what it expects.
///
/// **Toward the anchor at the test's repetition count, and nothing else is
/// read.** A triple works up to the 3RM the operator expects rather than to a
/// one-rep maximum nobody is attempting, and no other programme is consulted:
/// what the block expects is the block's own statement.
#[test]
fn a_blocks_entry_test_ramps_toward_what_it_expects() {
    let (prescriber, _directory) = blocked!();
    let issued = run!(prescriber.prescribe(Date::constant(2026, 9, 4)));

    assert_eq!(issued.workout.week(), WeekKind::Test);
    let Some(PrescribedItem::Exercise { exercise, .. }) =
        issued.workout.shape().item_for(SlotId::KneeDominant)
    else {
        panic!("the tested lift is a single exercise")
    };
    let domain::prescription::PrescribedExercise::ForReps { sets, .. } = exercise else {
        panic!("the front squat is counted in repetitions")
    };
    let working: Vec<_> = sets.iter().filter(|set| !set.warmup).collect();
    assert_eq!(working.len(), 3, "three attempts, and nothing after them");
    assert!(
        working.iter().all(|set| set.prescription.load().is_some()),
        "every attempt names its load"
    );
    assert_eq!(
        working.last().and_then(|set| set.prescription.effort()),
        Some(domain::gym::Rir::Zero),
        "nothing left in reserve is asked of the target"
    );
}

/// The other session of that week runs the load the block states for it.
///
/// Authored rather than derived, because the lift's maximum is what the week is
/// about to measure — so there is nothing to take a share of.
#[test]
fn the_entry_test_weeks_other_session_runs_the_authored_load() {
    let (prescriber, _directory) = blocked!();
    let issued = run!(prescriber.prescribe(Date::constant(2026, 8, 31)));

    let Some(PrescribedItem::Exercise { exercise, .. }) =
        issued.workout.shape().item_for(SlotId::KneeDominant)
    else {
        panic!("the primary is a single exercise")
    };
    let domain::prescription::PrescribedExercise::ForReps { sets, .. } = exercise else {
        panic!("the front squat is counted in repetitions")
    };
    let working: Vec<_> = sets.iter().filter(|set| !set.warmup).collect();
    let Some(first) = working.first() else {
        panic!("the session has a working set")
    };
    assert_eq!(
        first.prescription.load(),
        Some(domain::gym::Load::Absolute(
            domain::measure::Kg::from_grams(60_000)
        )),
        "the 60kg the block states, not a share of anything"
    );
}

/// The phases start the week after the entry test.
#[test]
fn the_phases_start_behind_the_entry_test() {
    let (prescriber, _directory) = blocked!();
    // The Friday of week two: the first week of accumulation, which runs sets
    // across rather than one attempt.
    let issued = run!(prescriber.prescribe(Date::constant(2026, 9, 11)));

    assert!(
        matches!(issued.workout.week(), WeekKind::Climbing(_)),
        "week two is a working week, not a second test"
    );
    let Some(PrescribedItem::Exercise { exercise, .. }) =
        issued.workout.shape().item_for(SlotId::KneeDominant)
    else {
        panic!("the primary is a single exercise")
    };
    let domain::prescription::PrescribedExercise::ForReps { sets, .. } = exercise else {
        panic!("the front squat is counted in repetitions")
    };
    let working = sets.iter().filter(|set| !set.warmup).count();
    assert!(
        working > 1,
        "accumulation runs sets across, got {working} working sets"
    );
}

// ---------------------------------------------------------------------------
// A test week its publisher wrote (issue #120).

/// What the test is an attempt at, and what the taper before it is a share of.
///
/// The number the operator declared for the autumn plan's opening week: *"I
/// asserted that I think i'm going to hit 95 for 1 in that test."* An anchor is
/// tested or declared, and this is the second — the estimate the wizard showed
/// him was a guide to declare against, not a source in its own right.
const TARGET_GRAMS: u64 = 95_000;

/// The chart's µ4 first session is `3 × 3 @ 75%`, and the barbell steps in
/// 2.5kg: 75% of 95kg is 71.25kg, which is not on the grid, and the share is
/// taken downward.
const TAPER_GRAMS: u64 = 70_000;

/// A test week taken from a published programme, target declared.
///
/// **µ4, because that is the only microcycle an entry test can be.** *Squat 2x
/// Int* µ4 is a taper and a one-repetition maximum; the test is the second of
/// those, and the week's other session is the first.
fn provided_test() -> Result<domain::prescription::GymMesocycle, Box<dyn std::error::Error>> {
    let published = ExternalProgramme::new(
        "Stronger By Science".to_owned().try_into()?,
        "Squat 2x Int".to_owned().try_into()?,
    );
    let answers = programme::authored(
        Date::constant(2026, 8, 31),
        Shape::Test {
            reps: domain::measure::RepCount::new(1)?,
            provided: Some(ProvidedFrom::new(published, vec![4])?),
            // Asserted, because a plan's opening week has nothing before it to
            // read an anchor from.
            asserted: Some(Anchor::new(
                domain::measure::Kg::from_grams(TARGET_GRAMS),
                None,
                AnchorProvenance::Asserted,
                Date::constant(2026, 8, 31),
            )?),
        },
    )?;
    Ok(programme::authoring(answers, &[])??)
}

/// A store whose only programme is a published test week — the shape of a plan
/// on the day it opens.
async fn with_provided_test(
    predecessor: bool,
) -> Result<(Prescriber, tempfile::TempDir), Box<dyn std::error::Error>> {
    let (parameters, directory, pool) = if predecessor {
        corpus_store().await?
    } else {
        landed_store().await?
    };
    let plan = programme::named_plan("published-entry-test", vec![provided_test()?])?;
    Authoring::new(
        SqlitePlanStore::new(pool.clone(), corpus::zone()?),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(&plan, &parameters)
    .await?;

    Ok((
        Prescribing::new(PrescriptionPorts {
            history: SqliteExerciseHistory::new(pool.clone()),
            programmes: SqliteGymMesocycleStore::new(pool.clone(), corpus::zone()?),
            parameters: SqliteGenerationParameterStore::new(pool.clone()),
            prescriptions: SqlitePrescribedWorkoutStore::new(
                pool.clone(),
                "Europe/London".to_owned(),
            ),
            lifecycle: SqlitePrescriptionDeliveryStore::new(pool),
        }),
        directory,
    ))
}

macro_rules! published {
    ($predecessor:expr) => {
        match corpus::block_on(with_provided_test($predecessor)) {
            Ok(Ok(ready)) => ready,
            Ok(Err(error)) => panic!("the corpus lands, derives and authors: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

/// The primary's working sets, panicking with the reason if it was withheld.
///
/// **A macro rather than a function**, because the panic exemptions
/// `clippy.toml` grants reach `#[test]` functions and not helpers defined
/// beside them — the same reason `run!` above is one.
macro_rules! primary_of {
    ($issued:expr) => {{
        let issued = &$issued;
        if let Some(missing) = issued
            .underivable
            .iter()
            .find(|missing| missing.slot == SlotId::KneeDominant)
        {
            panic!(
                "the lift the week is about is prescribed: {}",
                missing.reason
            )
        }
        let Some(PrescribedItem::Exercise { exercise, .. }) =
            issued.workout.shape().item_for(SlotId::KneeDominant)
        else {
            panic!("the primary is a single exercise")
        };
        let domain::prescription::PrescribedExercise::ForReps { sets, .. } = exercise else {
            panic!("the front squat is counted in repetitions")
        };
        let working: Vec<_> = sets.iter().filter(|set| !set.warmup).collect();
        working
    }};
}

/// The other session of a published test week is the chart's taper, off the
/// target.
///
/// **The default output of `fitness plan`, and the defect this file exists
/// for.** Its opening mesocycle is always a standalone test week with nothing
/// before it, and the weekday map always gives that week a light session — so
/// the primary was refused as underivable while the fifteen accessories around
/// it were issued and delivered. A session missing the lift the plan is about,
/// with nothing refused, is the worst shape a failure can take.
#[test]
fn a_published_test_weeks_other_session_is_the_charts_taper() {
    let (prescriber, _directory) = published!(false);
    let issued = run!(prescriber.prescribe(other_day()));

    let working = primary_of!(issued);
    assert_eq!(working.len(), 3, "µ4 day one is three sets");
    for set in &working {
        assert_eq!(
            set.prescription.load(),
            Some(domain::gym::Load::Absolute(
                domain::measure::Kg::from_grams(TAPER_GRAMS)
            )),
            "75% of the target, on the barbell's grid"
        );
    }
}

/// And the test itself is unchanged: the last attempt is still at the target.
#[test]
fn a_published_test_week_still_tests_on_the_heavy_session() {
    let (prescriber, _directory) = published!(false);
    let issued = run!(prescriber.prescribe(test_day()));

    let working = primary_of!(issued);
    assert_eq!(working.len(), 3, "three attempts, and nothing after them");
    assert_eq!(
        working.last().and_then(|set| set.prescription.load()),
        Some(domain::gym::Load::Absolute(
            domain::measure::Kg::from_grams(TARGET_GRAMS)
        )),
        "the last attempt is at what the operator asserted"
    );
}

/// A published week states its own other session, so the predecessor's is not
/// consulted even where there is one.
///
/// **Both days are shares of one number**, which is what makes the week cohere:
/// a taper taken off the predecessor's ladder and a test taken off the target
/// would be two programmes sharing a week.
#[test]
fn the_published_taper_does_not_defer_to_a_predecessor() {
    let (prescriber, _directory) = published!(true);
    let issued = run!(prescriber.prescribe(other_day()));

    let working = primary_of!(issued);
    assert_eq!(working.len(), 3, "µ4 day one is three sets");
    assert_eq!(
        working[0].prescription.load(),
        Some(domain::gym::Load::Absolute(
            domain::measure::Kg::from_grams(TAPER_GRAMS)
        )),
        "the chart's share of the target, not where the ladder before it stands"
    );
}

// ---------------------------------------------------------------------------
// A holding week after a test the gym completed (#190).

type HoldingPrescriber = Prescribing<
    SqliteExerciseHistory,
    application::reschedule::Rescheduled<SqliteGymMesocycleStore, SqlitePlanStore>,
    SqliteGenerationParameterStore,
    SqlitePrescribedWorkoutStore,
    SqlitePrescriptionDeliveryStore,
>;

/// A test in the week of 29 June, which holds the corpus's one failed attempt,
/// and the week after it held because the bike missed its own test.
async fn held_after_the_test()
-> Result<(HoldingPrescriber, tempfile::TempDir), Box<dyn std::error::Error>> {
    use application::DiaryStore as _;

    let (parameters, directory, pool) = landed_store().await?;
    let answers = programme::authored(
        Date::constant(2026, 6, 29),
        Shape::Test {
            reps: domain::measure::RepCount::new(1)?,
            provided: None,
            asserted: None,
        },
    )?;
    let test = programme::authoring(answers, &[])??;
    Authoring::new(
        SqlitePlanStore::new(pool.clone(), corpus::zone()?),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(&programme::named_plan("held", vec![test])?, &parameters)
    .await?;

    let diary = infrastructure::SqliteDiaryStore::new(pool.clone())
        .diary()
        .await?;
    let reschedule = application::reschedule::Reschedule::new(
        vec![domain::planner::Rerun {
            monday: Date::constant(2026, 6, 29),
            holding: Some(domain::schedule::Discipline::Gym),
        }],
        diary,
    );

    Ok((
        Prescribing::new(PrescriptionPorts {
            history: SqliteExerciseHistory::new(pool.clone()),
            programmes: application::reschedule::Rescheduled::new(
                SqliteGymMesocycleStore::new(pool.clone(), corpus::zone()?),
                SqlitePlanStore::new(pool.clone(), corpus::zone()?),
                reschedule,
            ),
            parameters: SqliteGenerationParameterStore::new(pool.clone()),
            prescriptions: SqlitePrescribedWorkoutStore::new(
                pool.clone(),
                "Europe/London".to_owned(),
            ),
            lifecycle: SqlitePrescriptionDeliveryStore::new(pool),
        }),
        directory,
    ))
}

/// The primary's first working set: the top set, for a linear session.
fn top_set(workout: &domain::prescription::PrescribedWorkout) -> Option<domain::gym::Load> {
    let Some(PrescribedItem::Exercise { exercise, .. }) =
        workout.shape().item_for(SlotId::KneeDominant)
    else {
        return None;
    };
    let domain::prescription::PrescribedExercise::ForReps { sets, .. } = exercise else {
        return None;
    };
    sets.iter()
        .find(|set| !set.warmup)
        .and_then(|set| set.prescription.load())
}

/// **The holding week is one week of the linear template, opened from the test
/// just done** (decision 0009). The test of 3 July failed a load, so the week
/// opens below it by the entry drop, and the anchor it descends from is the
/// test's own result rather than anything asserted.
#[test]
fn a_holding_week_opens_the_linear_template_from_the_test_just_done() {
    let (prescriber, _directory) = match corpus::block_on(held_after_the_test()) {
        Ok(Ok(ready)) => ready,
        Ok(Err(error)) => panic!("the corpus lands, derives and authors: {error}"),
        Err(error) => panic!("a runtime is available: {error}"),
    };
    let heavy = run!(prescriber.prescribe(Date::constant(2026, 7, 10)));
    let light = run!(prescriber.prescribe(Date::constant(2026, 7, 6)));

    assert_eq!(heavy.workout.week(), WeekKind::Holding);
    assert_eq!(light.workout.week(), WeekKind::Holding);

    let DerivedFrom::Anchor(anchor) = heavy.workout.derived_from() else {
        panic!("a holding week descends from an anchor, not a target")
    };
    assert_eq!(anchor.provenance(), AnchorProvenance::Tested);
    let failed = anchor.failed().expect("the test of 3 July failed a load");

    let parameters = programme::parameters().expect("the fixture parameters are valid");
    let steps = parameters
        .scales
        .for_exercise(domain::gym::Exercise::Reps(
            domain::gym::RepsExercise::FrontSquat,
        ))
        .expect("the front squat has a scale");
    let opening = steps.quantise_loaded(parameters.entry_drop.applied_to(failed));

    assert_eq!(
        top_set(&heavy.workout),
        Some(domain::gym::Load::Absolute(opening)),
        "the heavy single opens below the failed load by the entry drop"
    );
    let lighter = top_set(&light.workout);
    assert!(
        matches!(lighter, Some(domain::gym::Load::Absolute(load)) if load < opening),
        "the light session is a share of the heavy one: {lighter:?}"
    );
}
