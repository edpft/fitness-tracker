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
    ExtractionRunLog as _, LandingStore as _, NormalisationSummary, ProgrammeAuthor as _,
    WorkoutNormaliser as _, WorkoutPrescriber as _,
    normalise::{Normalisation, NormalisationPorts},
    prescribe::{Authoring, Prescribing, PrescriptionPorts},
};
use domain::prescription::{
    Anchor, AnchorProvenance, DerivedFrom, EntryTest, PrescribedItem, SessionRole, SlotId,
    WeekKind, authored::Shape,
};
use infrastructure::{
    HevyWorkoutLandingReader, HevyWorkoutLandingStore, HevyWorkoutTranslator,
    SqliteExerciseHistory, SqliteExtractionRunLog, SqliteGenerationParameterStore,
    SqliteGymWorkoutStore, SqliteNormalisationRunLog, SqlitePrescribedWorkoutStore,
    SqlitePrescriptionDeliveryStore, SqliteProgrammeStore, SqliteRefusalStore, connect,
};
use jiff::civil::Date;
use sqlx::SqlitePool;
use support::{corpus, programme};

type Prescriber = Prescribing<
    SqliteExerciseHistory,
    SqliteProgrammeStore,
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
        SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(&test, &parameters)
    .await?;

    Ok((
        Prescribing::new(PrescriptionPorts {
            history: SqliteExerciseHistory::new(pool.clone()),
            programmes: SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
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
/// Shared by both halves of this file: what succeeds the fixture block differs,
/// and everything before that does not.
async fn corpus_store() -> Result<
    (
        domain::prescription::GenerationParameters,
        tempfile::TempDir,
        SqlitePool,
    ),
    Box<dyn std::error::Error>,
> {
    let directory = tempfile::tempdir()?;
    let pool: SqlitePool = connect(&directory.path().join("test.db")).await?;

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

    let parameters = programme::parameters()?;
    Authoring::new(
        SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(
        &programme::as_programme(programme::programme()?),
        &parameters,
    )
    .await?;

    Ok((parameters, directory, pool))
}

/// A test week, authored in its own right.
///
/// **It states its seventeen slots like any other programme.** Until 2026-09-06
/// a test document could name only what changed and take the rest from the
/// programme before it, which the store had to be asked about at authoring time.
/// The questions ask every slot unconditionally, so there was never anything
/// left to inherit — and the fixture fills here are the same ones the programme
/// before it was authored with.
fn test_programme() -> Result<domain::prescription::Mesocycle, Box<dyn std::error::Error>> {
    let answers = programme::authored(
        "entry-test",
        Date::constant(2026, 8, 31),
        Shape::Test {
            reps: domain::gym::RepCount::new(1)?,
            // What the programme before it stands at, which is the ordinary case
            // (decision 0013).
            target: domain::prescription::TestTarget::Inherited,
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

/// The heavy session is a ramp and then one autoregulated attempt.
///
/// **No top set and no back-offs**, which is what separates a test from a
/// climbing week: the load is open at the top because going past the target is
/// the outcome the week exists to produce.
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
    assert_eq!(working.len(), 1, "one attempt, and nothing after it");
    // **The attempt states the load it is an attempt at.** The ramp above is
    // built as a share of exactly this number, so a prescription that withheld
    // it was working the operator up to something it had already decided.
    // Nothing caps it — zero in reserve is what says going past is the point.
    assert!(
        working[0].prescription.load().is_some(),
        "the attempt names the load the ramp was built toward"
    );
    assert_eq!(
        working[0].prescription.effort(),
        Some(domain::gym::Rir::Zero),
        "nothing left in reserve is what makes it a test"
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
fn autumn_block() -> Result<domain::prescription::Mesocycle, Box<dyn std::error::Error>> {
    let answers = programme::authored(
        "autumn",
        Date::constant(2026, 8, 31),
        Shape::Block {
            gating: SessionRole::Heavy,
            weeks: 10,
            // What the operator expects to lift. Week one finds out; a result
            // that differs is answered by re-authoring, which decision 0012
            // makes a supersession.
            anchor: Anchor::new(
                "90".to_owned().try_into()?,
                None,
                AnchorProvenance::Asserted,
                Date::constant(2026, 7, 3),
            )?,
            entry_test: Some(EntryTest::new(
                domain::gym::RepCount::new(3)?,
                Some("60".to_owned().try_into()?),
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
        SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(&block, &parameters)
    .await?;

    Ok((
        Prescribing::new(PrescriptionPorts {
            history: SqliteExerciseHistory::new(pool.clone()),
            programmes: SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
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
    assert_eq!(working.len(), 1, "one attempt, and nothing after it");
    assert!(
        working[0].prescription.load().is_some(),
        "the attempt names the load the ramp was built toward"
    );
    assert_eq!(
        working[0].prescription.effort(),
        Some(domain::gym::Rir::Zero),
        "nothing left in reserve is what makes it a test"
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
        Some(domain::gym::Load::Absolute(domain::gym::Kg::from_grams(
            60_000
        ))),
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
