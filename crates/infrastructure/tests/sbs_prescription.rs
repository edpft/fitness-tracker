//! What an SBS cycle prescribes once its repetition-maximum days have run.
//!
//! **The operator's own week of 2 October 2026, moved onto the fixture's
//! calendar** (#360). He worked up to eight repetitions at 75kg, went to 77.5kg
//! and got five, and took his back-offs at 75kg. His 8RM was 75kg; the build
//! read 77.5kg, put the training maximum at 97.5kg instead of 95kg, and
//! prescribed him 82.5kg on the Monday where the chart asks for 80kg.
//!
//! `domain/tests/sbs.rs` pins the rule against the workbook. What is pinned
//! here is the wiring either side of it: that the application hands the chart
//! every completed set rather than picking one itself, and that the kilograms
//! which come out the far end are the ones the chart intends.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

mod support;

use application::{
    CanonicalGymSessionStore as _, MesocycleStore as _, PlanAuthor as _, WorkoutPrescriber as _,
    prescribe::{Authoring, Prescribing, PrescriptionPorts},
};
use domain::{
    canonical::{Attributed, NormalisedSessionId, Occurred},
    gym::{
        CanonicalExercise, CanonicalGymSession, CanonicalItem, CanonicalSet, Identified, Load,
        Performed, SetKind,
        exercise::{Exercise, RepsExercise},
    },
    measure::{Kg, RepCount},
    normalised::{OperatorZone, StartedAt},
    prescription::{
        Anchor, AnchorProvenance, Block, GymMesocycle, PrescribedExercise, PrimaryPattern, Skip,
        Test, Tested, authored::Shape, target::Prescribed,
    },
    provider::{ExternalProgramme, ProgrammeName, ProvidedFrom, Provider},
    sequence::NonEmpty,
};
use infrastructure::{
    SqliteCanonicalGymSessionStore, SqliteExerciseHistory, SqliteGenerationParameterStore,
    SqliteGymMesocycleStore, SqlitePlanStore, SqlitePrescribedWorkoutStore,
    SqlitePrescriptionDeliveryStore,
};
use jiff::civil::Date;
use sqlx::SqlitePool;
use support::{corpus, programme};

type Failure = Box<dyn std::error::Error>;

/// Monday of the cycle's first week. Week 1 day 1 is the percentage day; its
/// Friday is the 8RM.
const fn opens() -> Date {
    Date::constant(2026, 9, 14)
}

/// The 8RM day of week 1.
const fn rep_max_day() -> Date {
    Date::constant(2026, 9, 18)
}

/// Week 2's percentage day, which is what this suite asks for.
const fn week_two() -> Date {
    Date::constant(2026, 9, 21)
}

/// *Squat 2x Int*, published by Stronger By Science: the whole four-week chart.
fn provided() -> Result<ProvidedFrom, Failure> {
    Ok(ProvidedFrom::new(
        ExternalProgramme::new(
            Provider::try_from("Stronger By Science".to_owned())?,
            ProgrammeName::try_from("Squat 2x Int".to_owned())?,
        ),
        vec![1, 2, 3, 4],
    )?)
}

/// The test week in front of the cycle, asserting what it measured.
///
/// **The cycle opens from this and from nothing else.** A provided cycle states
/// no anchor of its own, so what week 1 is a share of is whatever measured the
/// lift before it — here 92.5kg completed with 95kg failed above it, which is
/// the operator's test of 25 September 2026.
fn entry_test() -> Result<GymMesocycle, Failure> {
    let week = Test::week(
        Date::new(2026, 9, 7)?,
        &[] as &[Skip],
        programme::weekdays()?,
        programme::zone()?,
    )?;
    let measured = Anchor::new(
        Kg::from_grams(92_500),
        Some(Kg::from_grams(95_000)),
        AnchorProvenance::Tested,
        Date::new(2026, 9, 11)?,
    )?;
    Ok(GymMesocycle::Test(Test::new(
        Tested::new(
            PrimaryPattern::KneeDominant,
            Exercise::Reps(RepsExercise::FrontSquatBarbell),
            RepCount::new(1)?,
        ),
        programme::fills()?,
        week,
        None,
        Some(measured),
    )?))
}

fn cycle() -> Result<GymMesocycle, Failure> {
    let answers = programme::authored(opens(), Shape::Provided { from: provided()? })?;
    Ok(programme::authoring(answers, &[])??)
}

/// One front squat set, every field of it one normalised session's.
fn set(grams: u64, reps: u32, on: NormalisedSessionId) -> Result<CanonicalSet<RepCount>, Failure> {
    Ok(CanonicalSet {
        outcome: Attributed::new(Performed::Completed(Some(RepCount::new(reps)?)), on),
        load: Some(Attributed::new(Load::absolute(Kg::from_grams(grams)), on)),
        began: None,
        intensity: None,
        kind: Some(Attributed::new(SetKind::Working, on)),
        rest_after: None,
    })
}

/// A front squat session on a day, from `(grams, repetitions)` in order.
fn session(
    day: Date,
    recorded: &[(u64, u32)],
    stands_on: NormalisedSessionId,
) -> Result<CanonicalGymSession, Failure> {
    let mut sets = Vec::new();
    for &(grams, reps) in recorded {
        sets.push(set(grams, reps, stands_on)?);
    }
    let Some((first, rest)) = sets.split_first() else {
        return Err("a session records at least one set".into());
    };
    let squat = CanonicalExercise::ForReps {
        identified: Attributed::new(
            Identified::Recorded(RepsExercise::FrontSquatBarbell),
            stands_on,
        ),
        sets: NonEmpty::of(first.clone(), rest.to_vec()),
    };
    Ok(CanonicalGymSession::new(
        Occurred::At(StartedAt::new(
            format!("{day}T18:30:00Z").parse()?,
            OperatorZone::try_from("Europe/London".to_owned())?,
        )),
        NonEmpty::of(CanonicalItem::Exercise(squat), vec![]),
        None,
        None,
    ))
}

type Prescriber = Prescribing<
    SqliteExerciseHistory,
    SqliteGymMesocycleStore,
    SqliteGenerationParameterStore,
    SqlitePrescribedWorkoutStore,
    SqlitePrescriptionDeliveryStore,
>;

/// A store holding the cycle, its entry test, and the sessions handed in.
///
/// The corpus is landed and derived first because a canonical set names the
/// normalised session each of its fields came from, and that is a foreign key:
/// a canonical layer standing on nothing cannot be written.
async fn ready(
    performed: &[(Date, &[(u64, u32)])],
) -> Result<(Prescriber, tempfile::TempDir, SqlitePool), Failure> {
    let (directory, pool) = support::store::derived_and_authored().await?;
    let zone = corpus::zone()?;

    // A second plan, beside the fixture's own. Its weeks are September's and
    // the fixture block's are July's, so neither overlaps the other.
    //
    // **Built before the await, not inside it.** A `?` in an argument holds its
    // `Result` across the await, and a boxed error is not `Send` — which
    // `clippy::future_not_send` refuses rather than this reading better.
    let plan = programme::named_plan("2026-autumn", vec![entry_test()?, cycle()?])?;
    let parameters = programme::parameters()?;
    Authoring::new(
        SqlitePlanStore::new(pool.clone(), zone.clone()),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(&plan, &parameters)
    .await?;

    let stands_on = sqlx::query_scalar::<_, i64>("SELECT MIN(id) FROM gym_session")
        .fetch_one(&pool)
        .await?;
    let stands_on = NormalisedSessionId::try_from(stands_on)?;

    let canonical = SqliteCanonicalGymSessionStore::new(pool.clone());
    let mut sessions = canonical.all().await?;
    for &(day, recorded) in performed {
        sessions.push(session(day, recorded, stands_on)?);
    }
    canonical.replace(sessions).await?;

    Ok((
        Prescribing::new(PrescriptionPorts {
            history: SqliteExerciseHistory::new(pool.clone()),
            programmes: SqliteGymMesocycleStore::new(pool.clone(), zone),
            parameters: SqliteGenerationParameterStore::new(pool.clone()),
            prescriptions: SqlitePrescribedWorkoutStore::new(
                pool.clone(),
                "Europe/London".to_owned(),
            ),
            lifecycle: SqlitePrescriptionDeliveryStore::new(pool.clone()),
        }),
        directory,
        pool,
    ))
}

/// The primary exercise of the strength block.
///
/// Fallible and unwrapped at the call site: the `clippy.toml` exemptions cover
/// a `#[test]` body and not a function beside one.
fn primary_of(prescription: &application::Prescription) -> Option<&PrescribedExercise> {
    prescription
        .workout
        .shape()
        .items()
        .iter()
        .find(|item| {
            item.slots()
                .next()
                .is_some_and(|slot| slot.block() == Block::Strength)
        })
        .and_then(|item| item.exercises().next())
}

/// Its working sets, as `(load in grams, repetitions prescribed)`.
///
/// Warm-ups are left out: the ramp is derived from the top set and says nothing
/// about what the chart decided.
/// `None` for an exercise counted in anything but repetitions, a set with no
/// absolute load, or a set the chart left open — none of which a percentage day
/// of this cycle produces.
fn working_sets(exercise: &PrescribedExercise) -> Option<Vec<(u64, String)>> {
    let PrescribedExercise::ForReps { sets, .. } = exercise else {
        return None;
    };
    sets.iter()
        .filter(|set| !set.warmup)
        .map(|set| {
            let Some(Load::Absolute(load)) = set.prescription.load() else {
                return None;
            };
            let Prescribed::Fixed { measure, .. } = &set.prescription else {
                return None;
            };
            Some((load.as_grams(), measure.to_string()))
        })
        .collect()
}

/// The whole of #360, end to end: his sets in, his Monday out.
#[test]
fn the_monday_is_a_share_of_the_eight_rep_maximum() {
    corpus::block_on(async {
        let (prescriber, _directory, _pool) = ready(&[(
            rep_max_day(),
            &[
                (72_500, 8),
                (75_000, 8),
                // Went up for the ninth eight and got five. Not an 8RM.
                (77_500, 5),
                (75_000, 6),
                (75_000, 5),
                (75_000, 5),
            ],
        )])
        .await
        .expect("a store holding the cycle and the session");

        let prescription = prescriber
            .prescribe(week_two())
            .await
            .expect("week 2 derives");

        let primary = primary_of(&prescription).expect("a primary exercise");
        let sets = working_sets(primary).expect("fixed sets at an absolute load");
        assert_eq!(
            sets,
            vec![
                (80_000, "3".to_owned()),
                (80_000, "3".to_owned()),
                (80_000, "3".to_owned()),
                (80_000, "3".to_owned()),
            ],
            "week 2 is 4 × 3 at 85% of 95, floored to the grid at 80 — not at \
             82.5, which is 85% of the 97.5 that reading 77.5kg as the 8RM gives",
        );
    })
    .expect("a runtime");
}

/// The slot the assertion above reads is the one the chart drives.
#[test]
fn the_primary_slot_is_the_front_squat() {
    corpus::block_on(async {
        let (prescriber, _directory, pool) = ready(&[(rep_max_day(), &[(75_000, 8)])])
            .await
            .expect("a store holding the cycle and the session");

        let prescription = prescriber
            .prescribe(week_two())
            .await
            .expect("week 2 derives");

        let store = SqliteGymMesocycleStore::new(pool.clone(), corpus::zone().expect("a zone"));
        let found = store
            .on(week_two())
            .await
            .expect("a read")
            .expect("the cycle answers for its own week");
        assert_eq!(found.mesocycle.template(), "sbs");

        assert_eq!(
            primary_of(&prescription)
                .expect("a primary exercise")
                .exercise_key(),
            "front-squat-barbell",
            "the cycle's primary fills the strength block's first slot",
        );
        assert_eq!(
            prescription
                .workout
                .shape()
                .items()
                .iter()
                .filter_map(|item| item.slots().next())
                .find(|slot| slot.block() == Block::Strength)
                .map(|slot| slot.to_string())
                .as_deref(),
            Some("knee_dominant"),
        );
    })
    .expect("a runtime");
}
