//! An authored mesocycle, as a fixture.
//!
//! Free functions returning `Result`, so a test unwraps at the call site: the
//! `clippy.toml` exemptions cover `#[test]` bodies and not helpers defined
//! beside them.
//!
//! **The ladder span here is a test value and not the authored one.** These
//! numbers exist so the machinery can be exercised, and a test asserting a real
//! prescribed load must not read them as the programme's intent.
//!
//! Three are inferred from the performed record rather than stated by the
//! operator: the light-of-heavy percentage, the accessory range, and the
//! per-role top-set repetitions. The back-off percentage, the warm-up ramp and
//! the anchor are the operator's own. The duration is neither — it is an input
//! the operator supplies per block.
//!
//! Until 2026-09-06 these values had a second home in
//! `tests/fixtures/programme.toml`, and a test asserted the two agreed. The
//! document is gone and so is that test; this file is now the only fixture.

use application::{PlannedWith, StoreError};
use std::collections::BTreeMap;

use domain::{
    gym::exercise::{DistanceExercise, DurationExercise, Exercise, Implement, RepsExercise},
    measure::{Kg, RepCount},
    plan::{Mesocycles, Plan, PlanName},
    prescription::{
        Anchor, AnchorProvenance, Authored, AuthoringError, ByIntensity, Calendar, GymMesocycle,
        Linear, LoadSteps, PrescribedWorkout, Progression, Scales, Skip, Step,
        authored::Shape,
        linear::{Fill, Primary, PrimaryPattern, SlotFills, StaticFill},
    },
    schedule::{Relative, SessionRole, TrainingWeek},
    sequence::{AtLeastTwo, NonEmpty},
};
use jiff::{civil::Date, tz::TimeZone};

#[derive(Debug, thiserror::Error)]
pub enum MesocycleFixtureError {
    #[error("the programme fixture holds an invalid value: {0}")]
    Invalid(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

fn invalid(detail: impl std::fmt::Display) -> MesocycleFixtureError {
    MesocycleFixtureError::Invalid(detail.to_string())
}

fn kg(value: &str) -> Result<Kg, MesocycleFixtureError> {
    Kg::try_from(value.to_owned()).map_err(invalid)
}

fn reps(count: u32) -> Result<RepCount, MesocycleFixtureError> {
    RepCount::new(count).map_err(invalid)
}

/// The zone the corpus was trained in.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if the identifier is not one the database knows.
pub fn zone() -> Result<TimeZone, MesocycleFixtureError> {
    TimeZone::get("Europe/London").map_err(invalid)
}

/// What a fixture plan is derived from: the versions of the programme and each
/// primary progression (#192), and the load scales.
pub struct Parameters {
    pub planned_with: PlannedWith,
    pub scales: Scales,
}

/// The versions this build ships, and the fixture's load scales.
///
/// The ladder climb is the shipped 2.5kg, which is also the smallest plate, so
/// it reads as a test rate as well as the authored one.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if any literal here is not a valid value, which
/// would be a typo in this file.
pub fn parameters() -> Result<Parameters, MesocycleFixtureError> {
    use domain::prescription::seed;
    Ok(Parameters {
        planned_with: PlannedWith {
            workouts: domain::prescription::ByIntensity {
                higher: seed::workout().map_err(invalid)?,
                lower: seed::workout().map_err(invalid)?,
            },
            linear: seed::linear().map_err(invalid)?,
            sbs: seed::sbs().map_err(invalid)?,
        },
        // The bar, and the rack the wrist work is done on. A banded scale is
        // what the single increment could not express: 10kg leaves on a 2kg
        // step and 7kg on a 1kg one.
        scales: Scales::new(BTreeMap::from([
            (
                Implement::Barbell,
                LoadSteps::uniform(kg("2.5")?).map_err(invalid)?,
            ),
            (
                Implement::Cable,
                LoadSteps::uniform(kg("2.5")?).map_err(invalid)?,
            ),
            (
                Implement::Machine,
                LoadSteps::uniform(kg("2.5")?).map_err(invalid)?,
            ),
            (
                Implement::Dumbbell,
                LoadSteps::new(vec![
                    Step {
                        from: Kg::NONE,
                        size: kg("1")?,
                    },
                    Step {
                        from: kg("10")?,
                        size: kg("2")?,
                    },
                ])
                .map_err(invalid)?,
            ),
        ])),
    })
}

/// Store the fixture's versions, dated at the epoch so every plan a test makes
/// takes them (#192), and its scales as the equipment's kinds where none is
/// known (#450).
///
/// **What authoring a plan used to do as a side effect**, which it no longer
/// does: a plan takes the versions in force when it was made, and one made in
/// a store with none has nothing to be derived from.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if a fixture value is invalid or the store refuses.
pub async fn design(pool: &sqlx::SqlitePool) -> Result<(), MesocycleFixtureError> {
    use application::{EquipmentStore as _, ProgressionStore as _, WorkoutStore as _};
    use domain::schedule::Relative;
    use infrastructure::store::{SqliteEquipmentStore, SqliteGenerationParameterStore};

    let fixture = parameters()?;
    let versions = SqliteGenerationParameterStore::new(pool.clone());
    let at = jiff::Timestamp::UNIX_EPOCH;
    // Once: a test that builds on another's store designs it again.
    if versions.workout_at(Relative::Higher, at).await?.is_none() {
        versions
            .author_workout(Relative::Higher, at, &fixture.planned_with.workouts.higher)
            .await?;
        versions
            .author_workout(Relative::Lower, at, &fixture.planned_with.workouts.lower)
            .await?;
        versions
            .author_linear(at, &fixture.planned_with.linear)
            .await?;
        versions.author_sbs(at, &fixture.planned_with.sbs).await?;
    }

    let equipment = SqliteEquipmentStore::new(pool.clone());
    if equipment.facts().await?.kinds.is_empty() {
        for (implement, steps) in fixture.scales.iter() {
            equipment
                .record(
                    &domain::gym::equipment::Statement::Kind(*implement, steps.clone()),
                    None,
                    at,
                    None,
                )
                .await?;
        }
    }
    Ok(())
}

/// The eleven slot fills the record shows.
///
/// Fallible again since the static slots carry repetition counts, which are
/// constructed rather than taken by value.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if a literal here is invalid.
pub fn fills() -> Result<SlotFills, MesocycleFixtureError> {
    // Static prescriptions, authored rather than derived. Unwrapped here because
    // these literals are non-zero by inspection and a fallible fixture builder
    // would push the panic to every call site.
    let (three, five, twenty) = (reps(3)?, reps(5)?, reps(20)?);
    Ok(SlotFills {
        plyometric: Fill::Same(StaticFill {
            exercise: Exercise::Reps(RepsExercise::Pogo),
            sets: three,
            reps: twenty,
        }),
        power: Fill::Same(StaticFill {
            exercise: Exercise::Reps(RepsExercise::BoxJump),
            sets: three,
            reps: five,
        }),
        knee_dominant: Fill::Same(Exercise::Reps(RepsExercise::FrontSquatBarbell)),
        upper_push: Fill::Same(Exercise::Reps(RepsExercise::ChestDip)),
        upper_pull: Fill::Same(Exercise::Reps(RepsExercise::NeutralGripPullUp)),
        // Alternating: the reason the history projection is unbounded.
        hip_dominant: Fill::Alternating(ByIntensity {
            lower: Exercise::Reps(RepsExercise::BackExtensionMachine),
            higher: Exercise::Reps(RepsExercise::NordicHamstringsCurls),
        }),
        biceps: Fill::Same(Exercise::Reps(RepsExercise::PreacherCurlBarbell)),
        triceps: Fill::Same(Exercise::Reps(RepsExercise::OverheadTricepsExtensionCable)),
        wrist_flexion: Fill::Same(Exercise::Reps(RepsExercise::WristFlexionDumbbell)),
        wrist_extension: Fill::Same(Exercise::Reps(RepsExercise::WristExtensionDumbbell)),
        core: Fill::Same(Exercise::Reps(RepsExercise::BentOverCableChop)),
        handstand_hold: Fill::Same(Exercise::Duration(DurationExercise::HandstandHold)),
        dead_hang: Fill::Same(Exercise::Duration(DurationExercise::DeadHang)),
        hip_flexor_stretch: Fill::Same(Exercise::Duration(DurationExercise::CouchStretch)),
        hip_external_rotator_stretch: Fill::Same(Exercise::Duration(
            DurationExercise::NinetyNinety,
        )),
        // Both created in Hevy on 2026-08-20 and not yet performed. A hold needs
        // no history, so unlike the reps slots these still derive.
        hamstring_stretch: Fill::Same(Exercise::Duration(DurationExercise::StandingStraddleFold)),
        groin_stretch: Fill::Same(Exercise::Duration(DurationExercise::SquattingGroinStretch)),
    })
}

/// The anchor the July test established: 90kg, measured.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if the date or load is invalid.
pub fn anchor() -> Result<Anchor, MesocycleFixtureError> {
    let from = Date::new(2026, 7, 3).map_err(invalid)?;
    // The 3 July test: a completed single at 90, then a failed 95. The failed
    // load is what the block opens at.
    Anchor::new(kg("90")?, Some(kg("95")?), AnchorProvenance::Tested, from).map_err(invalid)
}

/// The operator's week, recorded so a mesocycle can be read back against it.
///
/// **A store needs a schedule before it can hold a plan**, and that is not a
/// fixture convenience. Since issue #63 a gym mesocycle carries no weekdays of
/// its own: its calendar is rebuilt from `training_slot` on every read, so a
/// programme authored into a store with no week in it cannot be read out
/// again. `fitness schedule add` is what the operator runs first, and the
/// wizard refuses to author a block without it.
///
/// From 2026-01-01, which is before every fixture block.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if the store is unavailable.
pub async fn record_the_week(pool: &sqlx::SqlitePool) -> Result<(), MesocycleFixtureError> {
    use application::DiaryAuthor as _;
    use domain::schedule::{Allocation, Discipline, PartOfDay, TrainingPattern, TrainingSlot};

    let slots = [
        (
            TrainingSlot::new(jiff::civil::Weekday::Monday, PartOfDay::Evening),
            Allocation::new(
                Discipline::Gym,
                SessionRole::new(Relative::Lower, Relative::Higher),
            ),
        ),
        (
            TrainingSlot::new(jiff::civil::Weekday::Wednesday, PartOfDay::Evening),
            Allocation::new(
                Discipline::Cycling,
                SessionRole::new(Relative::Higher, Relative::Lower),
            ),
        ),
        (
            TrainingSlot::new(jiff::civil::Weekday::Friday, PartOfDay::Evening),
            Allocation::new(
                Discipline::Gym,
                SessionRole::new(Relative::Higher, Relative::Lower),
            ),
        ),
        (
            TrainingSlot::new(jiff::civil::Weekday::Sunday, PartOfDay::Morning),
            Allocation::new(
                Discipline::Cycling,
                SessionRole::new(Relative::Lower, Relative::Higher),
            ),
        ),
    ]
    .into_iter()
    .collect();

    infrastructure::SqliteDiaryStore::new(pool.clone())
        .record_pattern(&TrainingPattern::new(
            Date::new(2026, 1, 1).map_err(invalid)?,
            super::corpus::zone().map_err(invalid)?,
            slots,
        ))
        .await
        .map_err(invalid)
}

/// Monday light, Friday heavy — what the record has run since June.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if the list is empty.
pub fn weekdays() -> Result<TrainingWeek, MesocycleFixtureError> {
    TrainingWeek::new(vec![
        (
            jiff::civil::Weekday::Monday,
            SessionRole::new(Relative::Lower, Relative::Higher),
        ),
        (
            jiff::civil::Weekday::Friday,
            SessionRole::new(Relative::Higher, Relative::Lower),
        ),
    ])
    .map_err(invalid)
}

/// The block's calendar: eight training weeks from 2026-07-06, uninterrupted.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if the date or the weekday list is invalid.
pub fn calendar() -> Result<Calendar, MesocycleFixtureError> {
    calendar_running(weekdays()?, &[])
}

/// The same block, run on given weekdays and skipping given weeks.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if the date is invalid, or if a named week falls
/// outside the block.
pub fn calendar_running(
    weekdays: TrainingWeek,
    skipping: &[Skip],
) -> Result<Calendar, MesocycleFixtureError> {
    let start = Date::new(2026, 7, 6).map_err(invalid)?;
    Calendar::new(start, 8, skipping, weekdays, zone()?).map_err(invalid)
}

/// The same block, started on a given date.
///
/// **Which weeks are inside the block decides which performed sessions gate the
/// ladder.** The corpus's one failed attempt is on Friday 2026-07-03, the week
/// before the block above opens, so a test about the failure mechanism has to
/// start the block early enough to contain it.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if the date or the weekday list is invalid.
pub fn calendar_from(
    start: Date,
    weekdays: TrainingWeek,
) -> Result<Calendar, MesocycleFixtureError> {
    Calendar::new(start, 8, &[], weekdays, zone()?).map_err(invalid)
}

/// A whole programme, ready to prescribe from.
///
/// Eight weeks from 2026-07-06, gating on the heavy session — the block the
/// record was trained under, with the test span from [`parameters`].
///
/// # Errors
///
/// [`MesocycleFixtureError`] if the programme is inconsistent, which would be a
/// mistake in this file rather than in the code under test.
/// What the fixtures call themselves.
///
/// One name across the fixtures, so two of them authored into one store are
/// versions of one programme rather than rivals for the same days — which is
/// what most of these tests want. A test about succession names its own.
pub const FIXTURE_NAME: &str = "fixture";

/// # Errors
///
/// [`MesocycleFixtureError`] if the text is not a usable plan name.
pub fn name(text: &str) -> Result<PlanName, MesocycleFixtureError> {
    PlanName::try_from(text.to_owned()).map_err(invalid)
}

/// One plan holding one gym mesocycle, under the fixture's name.
///
/// **What every suite that used to author a programme now authors.** A
/// mesocycle is not written on its own since #86: the plan is, and a test that
/// wants one mesocycle in the store wants a plan holding it.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if the name is unusable or the mesocycles do not
/// make a programme.
pub fn plan(mesocycles: Vec<GymMesocycle>) -> Result<Plan, MesocycleFixtureError> {
    named_plan(FIXTURE_NAME, mesocycles)
}

/// The same, under a name the caller chose. What a test about succession wants.
///
/// # Errors
///
/// As [`plan`].
pub fn named_plan(
    called: &str,
    mesocycles: Vec<GymMesocycle>,
) -> Result<Plan, MesocycleFixtureError> {
    Plan::new(
        name(called)?,
        jiff::Timestamp::now(),
        Some(Mesocycles::new(mesocycles).map_err(invalid)?),
        None,
    )
    .map_err(invalid)
}

/// A plan holding the one linear programme handed in.
///
/// # Errors
///
/// As [`plan`].
pub fn as_plan(linear: Linear) -> Result<Plan, MesocycleFixtureError> {
    plan(vec![entry_test()?, as_mesocycle(linear)])
}

/// The 3 July entry test, as the week in front of the block.
///
/// **Here because a programme no longer carries what it opens from.** The block
/// used to hold [`anchor`] as a field; it now reads the anchor off whatever
/// measured the lift before it, so the fixture has to put that measurement in
/// the plan rather than on the programme. The week is the one the record shows
/// — Monday 29 June to Sunday 5 July, with the test taken on the Friday — and
/// it asserts [`anchor`] so a store with no performances still resolves.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if the week or the test is invalid.
pub fn entry_test() -> Result<GymMesocycle, MesocycleFixtureError> {
    let start = Date::new(2026, 6, 29).map_err(invalid)?;
    let week = domain::prescription::Test::week(
        start,
        &[] as &[Skip],
        weekdays()?,
        jiff::tz::TimeZone::UTC,
    )
    .map_err(invalid)?;
    Ok(GymMesocycle::Test(
        domain::prescription::Test::new(
            domain::prescription::Tested::new(
                PrimaryPattern::KneeDominant,
                Exercise::Reps(RepsExercise::FrontSquatBarbell),
                domain::measure::RepCount::new(1).map_err(invalid)?,
            ),
            fills()?,
            week,
            None,
            Some(anchor()?),
        )
        .map_err(invalid)?,
    ))
}

/// The same entry test, in a week of the operator's choosing and his zone.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if the week or the test is invalid.
pub fn entry_test_from(start: Date) -> Result<GymMesocycle, MesocycleFixtureError> {
    let week = domain::prescription::Test::week(start, &[] as &[Skip], weekdays()?, zone()?)
        .map_err(invalid)?;
    Ok(GymMesocycle::Test(
        domain::prescription::Test::new(
            domain::prescription::Tested::new(
                PrimaryPattern::KneeDominant,
                Exercise::Reps(RepsExercise::FrontSquatBarbell),
                domain::measure::RepCount::new(1).map_err(invalid)?,
            ),
            fills()?,
            week,
            None,
            Some(anchor()?),
        )
        .map_err(invalid)?,
    ))
}

/// A set of answers, as the wizard would hand them over.
///
/// **The one place these tests name a template.** What used to be a TOML
/// document in every suite is now a [`Shape`] and the five facts every
/// programme states, so a test says what it is about — a cycle, a block, a
/// test — and nothing about how it was typed in.
///
/// The pattern and the primary are the fixture's own, so they agree with
/// [`fills`] by construction.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if the name or the weekday list is invalid.
pub fn authored(start: Date, shape: Shape) -> Result<Authored, MesocycleFixtureError> {
    Ok(Authored {
        start,
        pattern: PrimaryPattern::KneeDominant,
        primary_exercise: Exercise::Reps(RepsExercise::FrontSquatBarbell),
        week: weekdays()?,
        shape,
    })
}

/// What a set of answers authors, over the fixture's own fills and parameters.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if a fixture value is invalid. What the assembly
/// itself refuses is returned as an [`AuthoringError`] for the caller to assert
/// on.
pub fn authoring(
    answers: Authored,
    interruptions: &[Skip],
) -> Result<Result<GymMesocycle, AuthoringError>, MesocycleFixtureError> {
    Ok(domain::prescription::authored::mesocycle(
        answers,
        fills()?,
        interruptions,
        zone()?,
        &parameters()?.planned_with.linear,
        &parameters()?.scales,
    ))
}

/// A linear programme, as one of the three things a programme can be.
///
/// The fixtures below build a `Linear` because that is what they are about;
/// every port takes a `GymMesocycle`, so this is the one line between them.
#[must_use]
pub const fn as_mesocycle(linear: Linear) -> GymMesocycle {
    GymMesocycle::Progression(Progression::Linear(linear))
}

pub fn linear() -> Result<Linear, MesocycleFixtureError> {
    linear_skipping(&[])
}

/// The same programme, with sessions it does not run.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if the programme is inconsistent, or if a skip
/// falls outside the block.
pub fn linear_skipping(skips: &[Skip]) -> Result<Linear, MesocycleFixtureError> {
    let parameters = parameters()?;
    Linear::new(
        Primary::new(
            domain::prescription::PrimaryPattern::KneeDominant,
            Exercise::Reps(RepsExercise::FrontSquatBarbell),
            SessionRole::new(Relative::Higher, Relative::Lower),
        ),
        fills()?,
        calendar_running(weekdays()?, skips)?,
        &parameters.planned_with.linear,
        &parameters.scales,
    )
    .map_err(invalid)
}

/// The same programme, started on a given date.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if the programme is inconsistent, which would be a
/// mistake in this file rather than in the code under test.
pub fn linear_from(start: Date) -> Result<Linear, MesocycleFixtureError> {
    let parameters = parameters()?;
    Linear::new(
        Primary::new(
            domain::prescription::PrimaryPattern::KneeDominant,
            Exercise::Reps(RepsExercise::FrontSquatBarbell),
            SessionRole::new(Relative::Higher, Relative::Lower),
        ),
        fills()?,
        calendar_from(start, weekdays()?)?,
        &parameters.planned_with.linear,
        &parameters.scales,
    )
    .map_err(invalid)
}

/// A programme gating on a role the operator's week never runs.
///
/// One of the three inconsistencies the types cannot catch, built here so the
/// test asserting it is refused does not have to construct it inline.
///
/// **Refused at authoring rather than by `Linear::new`**, and it moved there on
/// 2026-09-20 (issue #63). Whether the week offers the gating session is a
/// question about a programme and a week together, and the week is superseded
/// whenever the operator's life changes while the programme stays in the
/// store — so as a type invariant it would make an old mesocycle fail to load.
///
/// # Errors
///
/// [`MesocycleFixtureError`] only if a literal here is invalid; the programme
/// itself is expected to be refused, which the caller asserts.
pub fn gating_on_a_role_it_never_runs()
-> Result<Result<GymMesocycle, AuthoringError>, MesocycleFixtureError> {
    // Monday only, and Monday is the easier session — so a gate on the harder
    // one never fires.
    let monday_only = TrainingWeek::new(vec![(
        jiff::civil::Weekday::Monday,
        SessionRole::new(Relative::Lower, Relative::Higher),
    )])
    .map_err(invalid)?;

    authoring(
        Authored {
            start: Date::new(2026, 7, 6).map_err(invalid)?,
            pattern: PrimaryPattern::KneeDominant,
            primary_exercise: Exercise::Reps(RepsExercise::FrontSquatBarbell),
            week: monday_only,
            shape: Shape::Linear {
                gating: SessionRole::new(Relative::Higher, Relative::Lower),
                weeks: 8,
            },
        },
        &[],
    )
}

/// A programme whose primary is counted in something other than repetitions.
///
/// # Errors
///
/// [`MesocycleFixtureError`] only if a literal here is invalid.
pub fn primary_not_counted_in_reps()
-> Result<Result<Linear, domain::prescription::InconsistentMesocycle>, MesocycleFixtureError> {
    let parameters = parameters()?;
    Ok(Linear::new(
        Primary::new(
            domain::prescription::PrimaryPattern::KneeDominant,
            Exercise::Distance(DistanceExercise::Running),
            SessionRole::new(Relative::Higher, Relative::Lower),
        ),
        fills()?,
        calendar()?,
        &parameters.planned_with.linear,
        &parameters.scales,
    ))
}

/// A programme naming one exercise as primary and filling the slot with another.
///
/// # Errors
///
/// [`MesocycleFixtureError`] only if a literal here is invalid.
pub fn primary_does_not_fill_its_slot()
-> Result<Result<Linear, domain::prescription::InconsistentMesocycle>, MesocycleFixtureError> {
    let parameters = parameters()?;
    Ok(Linear::new(
        // Names the knee-dominant slot as primary, but the primary exercise is a
        // deadlift, and the knee-dominant fill is a front squat.
        Primary::new(
            domain::prescription::PrimaryPattern::KneeDominant,
            Exercise::Reps(RepsExercise::DeadliftBarbell),
            SessionRole::new(Relative::Higher, Relative::Lower),
        ),
        fills()?,
        calendar()?,
        &parameters.planned_with.linear,
        &parameters.scales,
    ))
}

/// A minimal issued session for a date, for a test that only needs one to
/// exist.
///
/// **Its contents do not matter and its identity does.** What #185 asks of a
/// gym prescription is whether one holds the date's place at a destination,
/// which is a join between two tables and says nothing about what was
/// prescribed.
///
/// # Errors
///
/// [`MesocycleFixtureError`] if the date or the shape is invalid.
pub fn a_workout_for(date: Date) -> Result<PrescribedWorkout, MesocycleFixtureError> {
    use domain::{
        gym::{Load, exercise::RepsExercise},
        measure::RepCount,
        prescription::{
            DerivedFrom, MesocycleId, PrescribedExercise, PrescribedItem, PrescribedSet, SlotId,
            Target, WeekIndex, WeekKind, WorkoutShape,
        },
        sequence::NonEmpty,
    };

    let sets = NonEmpty::new(vec![PrescribedSet::fixed(
        Load::BODYWEIGHT,
        Target::spanning(
            RepCount::new(5).map_err(invalid)?,
            RepCount::new(3).map_err(invalid)?,
        ),
    )])
    .map_err(invalid)?;

    let shape = WorkoutShape::new(
        NonEmpty::new(vec![PrescribedItem::Exercise {
            slot: SlotId::UpperPull,
            exercise: PrescribedExercise::ForReps {
                exercise: RepsExercise::NeutralGripPullUp,
                sets,
                assisted: None,
            },
        }])
        .map_err(invalid)?,
    );

    Ok(PrescribedWorkout::new(
        shape,
        date,
        SessionRole::new(Relative::Higher, Relative::Lower),
        WeekKind::Climbing(WeekIndex::new(1).map_err(invalid)?),
        DerivedFrom::Anchor(anchor()?),
        MesocycleId::new(1),
        "2026-09-15T18:00:00Z".parse().map_err(invalid)?,
    ))
}
