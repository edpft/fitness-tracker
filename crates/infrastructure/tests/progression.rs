//! The failure mechanism, wired up: a real entry test opens a real block.
//!
//! User story 3 through its ports. The mechanism's arithmetic is asserted in
//! `crates/domain/tests/progression.rs` — hand-built sequences including the
//! worked example load for load — and what needs a store is the wiring: that the
//! *record* decides which rung a date gets, and that the calendar week it falls in
//! does not.
//!
//! **The corpus supplies a genuine entry test.** The front squat of Friday
//! 2026-07-03 completed a single at 90 and then failed 95 for zero repetitions,
//! which user story 2 made a failed attempt in the record and which the fixture
//! programme's anchor records as its ceiling.
//!
//! **That is also why no in-block miss is asserted here.** It is the corpus's only
//! failure, and since 2026-08-19 a block opens *from* its entry test rather than
//! containing it — `Linear::new` refuses one that does not, because a block
//! holding its own entry test would read that failure twice. So the block starts
//! the Monday after, the record has no missed gating set inside it, and US3-5
//! stays where its arithmetic is: the domain suite.

mod support;

use application::{
    DeliveryReference, DestinationName, PrescriptionDeliveryStore as _, WorkoutPrescriber as _,
    prescribe::{Prescribing, PrescriptionPorts},
};
use domain::prescription::{PrescribedItem, SessionRole, SlotId, WeekIndex, WeekKind};
use infrastructure::{
    SqliteExerciseHistory, SqliteGenerationParameterStore, SqlitePrescribedWorkoutStore,
    SqlitePrescriptionDeliveryStore, SqliteProgrammeStore,
};
use jiff::civil::Date;
use sqlx::SqlitePool;
use support::{corpus, programme, store};

type Prescriber = Prescribing<
    SqliteExerciseHistory,
    SqliteProgrammeStore,
    SqliteGenerationParameterStore,
    SqlitePrescribedWorkoutStore,
>;

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// The Monday after the 3 July entry test. The block cannot start earlier: its
/// own test would fall inside it.
const BLOCK_START: Date = Date::constant(2026, 7, 6);

/// The two Hevy routines the record's July sessions were performed against.
///
/// **The published id is the only thing that links a performance to a
/// prescription**, so a gating session is one the record says was performed
/// against a session this programme prescribed as gating. The corpus already
/// carries both references: every session titled Heavy names one routine and
/// every Light one names the other, because the operator reused a routine per
/// role rather than publishing a fresh one each week.
///
/// One delivery per role is therefore what the record supports, and it is
/// enough. What the link supplies is *which session it was*; the load the gate
/// reads comes from the performance, as it always did.
const HEAVY_ROUTINE: &str = "11437699-cb70-4e0e-a77b-caa9fd5cdb24";
const LIGHT_ROUTINE: &str = "f3f2364c-4dd2-4ba5-9406-43035f99161d";

/// A store whose block opens on 2026-07-06, the Monday after its entry test.
async fn ready() -> Fallible<(Prescriber, tempfile::TempDir)> {
    let (prescriber, directory, _) = published().await?;
    Ok((prescriber, directory))
}

/// The same store, with the block's first two sessions published.
///
/// Published rather than merely drafted, because a drafted prescription has no
/// reference for a performance to name — the destination assigns it on delivery
/// (decision 0017), and until then nothing the operator trains can point at it.
async fn published() -> Fallible<(Prescriber, tempfile::TempDir, SqlitePool)> {
    let start = BLOCK_START;
    let (directory, pool) = store::with_programme(programme::programme_from(start)?).await?;
    let prescriber = Prescribing::new(PrescriptionPorts {
        history: SqliteExerciseHistory::new(pool.clone()),
        programmes: SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
        parameters: SqliteGenerationParameterStore::new(pool.clone()),
        prescriptions: SqlitePrescribedWorkoutStore::new(pool.clone(), "Europe/London".to_owned()),
    });

    // The block's first Monday and first Friday, which are its light and gating
    // sessions. Issuing them before anything reads the gate is the real order:
    // a session is published, then performed, then read back.
    deliver(
        &prescriber,
        &pool,
        Date::constant(2026, 7, 6),
        LIGHT_ROUTINE,
    )
    .await?;
    deliver(
        &prescriber,
        &pool,
        Date::constant(2026, 7, 10),
        HEAVY_ROUTINE,
    )
    .await?;

    Ok((prescriber, directory, pool))
}

/// Issue the session for a date and record what the destination called it.
async fn deliver(
    prescriber: &Prescriber,
    pool: &SqlitePool,
    date: Date,
    reference: &str,
) -> Fallible<()> {
    let issued = prescriber.prescribe(date, application::Reissue::No).await?;
    let deliveries = SqlitePrescriptionDeliveryStore::new(pool.clone());
    deliveries
        .record(
            issued.id,
            &DestinationName::try_from("hevy".to_owned())?,
            &DeliveryReference::try_from(reference.to_owned())?,
            jiff::Timestamp::UNIX_EPOCH,
        )
        .await?;
    Ok(())
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
        let issued = run!($prescriber.prescribe($date, application::Reissue::No));
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

/// The block opens below what its entry test failed, through the real store.
///
/// **The sharp assertion.** The entry test of 3 July completed 90 and failed 95,
/// so the block opens at −10% of that failed 95 — 85.5, which the plate grid
/// takes to 85 — and climbs back through it. What is prescribed for the block's
/// first gating Friday is the ladder's own first rung, because since 2026-08-20
/// the drop *is* the opening rather than a climb-in before it.
///
/// The load being *below* the anchor of 90 is the whole point: it is what tells
/// this model apart from the one it replaced, which opened at 95 and made week
/// one heavier than the tested maximum.
#[test]
fn the_block_opens_below_what_its_entry_test_failed() {
    let (prescriber, _directory) = ready!();
    let (issued, load) = top_set!(&prescriber, Date::constant(2026, 7, 10));

    assert_eq!(issued.workout.session_role(), SessionRole::Heavy);
    assert_eq!(
        issued.workout.week(),
        WeekKind::Climbing(WeekIndex::FIRST),
        "the calendar still says which week of the block this is"
    );

    let Ok(parameters) = programme::parameters() else {
        panic!("the fixtures build")
    };
    let Ok(programme) = programme::programme_from(BLOCK_START) else {
        panic!("the programme builds")
    };
    let (Ok(ladder), Ok(steps)) = (programme.ladder(&parameters), programme.steps(&parameters))
    else {
        panic!("the ladder builds")
    };

    let Ok(opening) = "85".to_owned().try_into().map(domain::gym::Load::Absolute) else {
        panic!("85 is a mass")
    };
    assert_eq!(
        load,
        Some(opening),
        "the block opens at the failed load dropped by the entry drop"
    );
    assert_eq!(
        load,
        ladder
            .heavy_top_set(WeekIndex::FIRST, steps)
            .map(domain::gym::Load::Absolute),
        "and that is the ladder's own first rung: the drop is the opening"
    );
}

/// The Friday session performed on Saturday morning still gates.
///
/// **The defect this replaced, stated as a test.** The gate used to ask the
/// calendar what a past performance was — `place(performance.on)` — and the
/// calendar answers for the day a session was *prescribed* for. So a heavy
/// session trained the next morning was a date the calendar refused, and it fell
/// out of the ladder entirely: the climb did not advance, and the following
/// Monday was prescribed a rung too low.
///
/// Nothing about the performance changes here but its clock. The same session,
/// against the same routine, with the same top set, moved from Friday evening to
/// Saturday morning — and the Monday after is prescribed exactly what it is when
/// the session was on time. Read through the calendar this asserts 75 against a
/// 72.5 that the session going missing would produce.
///
/// This is decision 0018's third bullet, and § 12: the performance is the fact.
#[test]
fn a_gating_session_performed_the_next_morning_still_gates() {
    let (prescriber, _directory, pool) = match corpus::block_on(published()) {
        Ok(Ok(ready)) => ready,
        Ok(Err(error)) => panic!("the corpus lands, derives and authors: {error}"),
        Err(error) => panic!("a runtime is available: {error}"),
    };

    // 18:25 on Friday 10 July becomes 09:25 on Saturday the 11th — a day the
    // block prescribes nothing at all.
    let moved = run!(async {
        sqlx::query!(
            "UPDATE gym_workout \
             SET started_at_utc = replace(started_at_utc, '2026-07-10T18', '2026-07-11T09') \
             WHERE started_at_utc LIKE '2026-07-10T18%'"
        )
        .execute(&pool)
        .await
    });
    // Without this the session stays on its Friday and the assertion below
    // passes for the wrong reason.
    assert_eq!(
        moved.rows_affected(),
        1,
        "the Friday session is the one that moved"
    );

    let (issued, light) = top_set!(&prescriber, Date::constant(2026, 7, 13));
    assert_eq!(issued.workout.session_role(), SessionRole::Light);

    let Ok(gated) = "75".to_owned().try_into().map(domain::gym::Load::Absolute) else {
        panic!("75 is a mass")
    };
    assert_eq!(
        light,
        Some(gated),
        "the session was performed, so it gates — whatever day it landed on"
    );
}

/// A session the gate does not watch does not move the progression (US3-10).
///
/// **Counted out, because the numbers are what carry the assertion.** The block
/// opens climbing in at 90 toward the 95 its entry test failed. By Monday
/// 2026-07-13 it has had one Friday — 10 July, completed — so the climb has
/// advanced once, at the second reset's +2.5kg, to 92.5. The light session's top
/// set is 85% of that, which the grid puts at 75.
///
/// It has also had a Monday, 6 July, trained and completed. **If the light
/// session gated too the climb would have advanced twice**, reaching the third
/// rung at 90 and prescribing 85% of it — which is 77.5. Asserting 75 rather
/// than 77.5 is the assertion that only the gating role gates, and it is checked
/// through the load actually prescribed rather than through the mechanism's own
/// state.
#[test]
fn only_the_gating_role_gates() {
    let (prescriber, _directory) = ready!();
    let (issued, light) = top_set!(&prescriber, Date::constant(2026, 7, 13));
    assert_eq!(issued.workout.session_role(), SessionRole::Light);

    let (Ok(gated), Ok(ungated)) = (
        "75".to_owned().try_into().map(domain::gym::Load::Absolute),
        "77.5"
            .to_owned()
            .try_into()
            .map(domain::gym::Load::Absolute),
    ) else {
        panic!("both are masses")
    };
    assert_ne!(gated, ungated, "the two positions must be distinguishable");
    assert_eq!(
        light,
        Some(gated),
        "one Friday has gone by, and the Monday before it does not advance the climb"
    );
}
