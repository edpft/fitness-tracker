//! A test as a programme in its own right, end to end (decision 0013).
//!
//! **Two halves, and they need different things.** What a test *document* says
//! is a question for the reader alone — no store, no record — so those tests
//! build a `Document` and assert on the programme it produces. What a test
//! *week* prescribes is a question about the record, so those go through the
//! store with the corpus in it, which is why this file is at the adapter's ring
//! rather than in `application`.
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
    ProgrammeStore as _, Reissue, WorkoutNormaliser as _, WorkoutPrescriber as _,
    normalise::{Normalisation, NormalisationPorts},
    prescribe::{Authoring, Prescribing, PrescriptionPorts},
};
use domain::prescription::{DerivedFrom, PrescribedItem, SlotId, WeekKind};
use infrastructure::{
    Document, HevyWorkoutLandingReader, HevyWorkoutLandingStore, HevyWorkoutTranslator,
    SqliteExerciseHistory, SqliteExtractionRunLog, SqliteGenerationParameterStore,
    SqliteGymWorkoutStore, SqliteNormalisationRunLog, SqlitePrescribedWorkoutStore,
    SqliteProgrammeStore, SqliteRefusalStore, connect,
};
use jiff::civil::Date;
use sqlx::SqlitePool;
use support::{corpus, programme};

type Prescriber = Prescribing<
    SqliteExerciseHistory,
    SqliteProgrammeStore,
    SqliteGenerationParameterStore,
    SqlitePrescribedWorkoutStore,
>;

/// The test document, as the operator would write it.
///
/// **Nineteen lines and no `[fills]`**, which is the whole of what decision 0013
/// bought: the week runs the same seventeen slots as the programme before it,
/// and saying so takes no lines at all.
const TEST_DOCUMENT: &str = r#"
[programme]
name             = "entry-test"
template         = "test"
primary          = "knee_dominant"
primary_exercise = "front-squat"
reps             = 1
start            = "2026-08-31"

[programme.weekdays]
monday = "light"
friday = "heavy"
"#;

/// A block that measures its own entry, in the week after the fixture block.
///
/// **Ten phase weeks, and eleven calendar weeks.** `duration_weeks` counts
/// phases whether or not there is an entry test; the week in front is added by
/// the presence of the `[programme.entry_test]` table and by nothing else.
const BLOCK_DOCUMENT: &str = r#"
[programme]
name             = "autumn"
template         = "block"
primary          = "knee_dominant"
primary_exercise = "front-squat"
gating_role      = "heavy"
start            = "2026-08-31"
duration_weeks   = 10

[programme.weekdays]
monday = "light"
friday = "heavy"

# What the operator expects to lift. Week one finds out; a result that differs is
# answered by re-authoring, which decision 0012 makes a supersession.
[programme.anchor]
load       = "90kg"
provenance = "asserted"
from       = "2026-07-03"

[programme.entry_test]
reps  = 3
light = "60kg"

# A block states every slot itself. Only a test inherits, and only because a test
# is two sessions rather than a programme.
[fills]
knee_dominant                = "front-squat"
upper_push                   = "chest-dip"
upper_pull                   = "neutral-grip-pull-up"
hip_dominant                 = "nordic-hamstrings-curls"
biceps                       = "preacher-curl-barbell"
triceps                      = "overhead-triceps-extension-cable"
wrist_flexion                = "wrist-flexion-dumbbell"
wrist_extension              = "wrist-extension-dumbbell"
core                         = "bent-over-cable-chop"
handstand_hold               = "handstand-hold"
dead_hang                    = "dead-hang"
hip_flexor_stretch           = "couch-stretch"
hip_external_rotator_stretch = "ninety-ninety"
hamstring_stretch            = "standing-straddle-fold"
groin_stretch                = "squatting-groin-stretch"

[fills.plyometric]
exercise = "pogo"
sets     = 3
reps     = 20

[fills.power]
exercise = "box-jump"
sets     = 3
reps     = 5
"#;

/// A store holding the corpus, derived, with the fixture programme authored and
/// a standalone test in the week after it.
///
/// The fixture block runs eight weeks from 2026-07-06, so it ends on Sunday
/// 30 August and this week is adjacent rather than overlapping — which the
/// overlap rule would otherwise refuse.
async fn ready() -> Result<(Prescriber, tempfile::TempDir), Box<dyn std::error::Error>> {
    let (parameters, directory, pool) = corpus_store().await?;

    // The test inherits the fills of the programme it follows, resolved here,
    // against what the store already holds.
    let test = test_programme(&pool, &parameters).await?;
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
            prescriptions: SqlitePrescribedWorkoutStore::new(pool, "Europe/London".to_owned()),
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

/// The test document, read over the fills the store already holds.
async fn test_programme(
    pool: &SqlitePool,
    parameters: &domain::prescription::GenerationParameters,
) -> Result<domain::prescription::Programme, Box<dyn std::error::Error>> {
    let document: Document = toml::from_str(TEST_DOCUMENT)?;
    let store = SqliteProgrammeStore::new(pool.clone(), corpus::zone()?);
    let inherited = store
        .preceding(document.start()?)
        .await?
        .map(|(_, programme)| programme);
    Ok(document.programme(
        parameters,
        corpus::zone()?.as_time_zone(),
        inherited
            .as_ref()
            .map(domain::prescription::Programme::fills),
        &[],
    )?)
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
    let issued = run!(prescriber.prescribe(test_day(), Reissue::No));

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
    assert!(
        working[0].prescription.load().is_none(),
        "the attempt is autoregulated: its load is what the day allows"
    );
}

/// The other session of the week is the previous programme's, not a second test.
#[test]
fn the_light_session_is_the_predecessors() {
    let (prescriber, _directory) = prescriber!();
    let issued = run!(prescriber.prescribe(other_day(), Reissue::No));

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

/// Every slot the document leaves out comes from the programme before it.
///
/// This is what stops a test being a whole programme's worth of authoring. The
/// document above names no fills at all, and the week still runs the same
/// seventeen slots with the same exercises in them.
#[test]
fn a_test_inherits_every_slot_it_does_not_state() {
    let (prescriber, _directory) = prescriber!();
    let issued = run!(prescriber.prescribe(test_day(), Reissue::No));

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
        "the fixture's plyometric fill, and this document states none"
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

/// A field the template has no use for is refused, not ignored.
///
/// A `gating_role` on a test is the operator believing something untrue of this
/// programme — there is no ladder to gate — and reading past it silently is how
/// a document and what it authors come apart.
#[test]
fn a_test_that_names_a_gating_role_is_refused() {
    // The extra key belongs to the `[programme]` table, so it goes before the
    // weekday table rather than after it.
    let document = TEST_DOCUMENT.replace(
        "start            = \"2026-08-31\"",
        "start            = \"2026-08-31\"\ngating_role      = \"heavy\"",
    );
    let Ok(document) = toml::from_str::<Document>(&document) else {
        panic!("the amended document is valid TOML")
    };
    let (Ok(parameters), Ok(zone)) = (programme::parameters(), corpus::zone()) else {
        panic!("the fixture parameters and zone build")
    };
    let Ok(fills) = programme::fills() else {
        panic!("the fixture fills build")
    };
    match document.programme(&parameters, zone.as_time_zone(), Some(&fills), &[]) {
        Err(infrastructure::DocumentError::Invalid { field, .. }) => {
            assert_eq!(field, "programme.gating_role");
        }
        Ok(_) => panic!("a test with a gating role must not author"),
        Err(other) => panic!("the refusal names the field, got {other}"),
    }
}

// ---------------------------------------------------------------------------
// A block that measures its own entry (decision 0016, as amended).

/// A store with the fixture block, and an autumn block that tests its own entry.
async fn with_block() -> Result<(Prescriber, tempfile::TempDir), Box<dyn std::error::Error>> {
    let (_, directory, pool) = corpus_store().await?;
    let parameters = programme::parameters()?;
    let document: Document = toml::from_str(BLOCK_DOCUMENT)?;
    let block = document.programme(&parameters, corpus::zone()?.as_time_zone(), None, &[])?;
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
            prescriptions: SqlitePrescribedWorkoutStore::new(pool, "Europe/London".to_owned()),
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
    let issued = run!(prescriber.prescribe(Date::constant(2026, 9, 4), Reissue::No));

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
        working[0].prescription.load().is_none(),
        "the attempt is autoregulated"
    );
}

/// The other session of that week runs the load the block states for it.
///
/// Authored rather than derived, because the lift's maximum is what the week is
/// about to measure — so there is nothing to take a share of.
#[test]
fn the_entry_test_weeks_other_session_runs_the_authored_load() {
    let (prescriber, _directory) = blocked!();
    let issued = run!(prescriber.prescribe(Date::constant(2026, 8, 31), Reissue::No));

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
        "the 60kg the document states, not a share of anything"
    );
}

/// The phases start the week after the entry test.
#[test]
fn the_phases_start_behind_the_entry_test() {
    let (prescriber, _directory) = blocked!();
    // The Friday of week two: the first week of accumulation, which runs sets
    // across rather than one attempt.
    let issued = run!(prescriber.prescribe(Date::constant(2026, 9, 11), Reissue::No));

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
