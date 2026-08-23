//! An authored programme, as a fixture.
//!
//! Free functions returning `Result`, so a test unwraps at the call site: the
//! `clippy.toml` exemptions cover `#[test]` bodies and not helpers defined
//! beside them.
//!
//! **The ladder span here is a test value and not the authored one.** Research
//! D8 records that the real span is undecided, and the authored document at
//! `tests/fixtures/programme.toml` still carries `TODO` for it. These numbers
//! exist so the machinery can be exercised, and a test asserting a real
//! prescribed load must not read them as the programme's intent.
//!
//! Three others are inferred from the performed record rather than stated by the
//! operator, and the document marks them `INFERRED`: the light-of-heavy
//! percentage, the accessory range, and the per-role top-set repetitions. The
//! back-off percentage, the warm-up ramp and the anchor are the operator's own.
//! The duration is neither — it is an input the operator supplies per block.

use application::StoreError;
use std::collections::BTreeMap;

use domain::{
    gym::{
        Kg, RepCount,
        exercise::{DistanceExercise, DurationExercise, Exercise, Implement, RepsExercise},
        sequence::{AtLeastTwo, NonEmpty},
    },
    prescription::{
        Anchor, AnchorProvenance, BackOff, Calendar, Entry, GenerationParameters, Linear,
        LoadSteps, PerRole, Percentage, Periodisation, Programme, ProgrammeName, ResetProtocol,
        Scales, SessionRole, Skip, Step, TopSetReps, WarmupStep, Weekdays,
        linear::{Fill, Primary, SlotFills, StaticFill},
    },
};
use jiff::{civil::Date, tz::TimeZone};

#[derive(Debug, thiserror::Error)]
pub enum ProgrammeFixtureError {
    #[error("the programme fixture holds an invalid value: {0}")]
    Invalid(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

fn invalid(detail: impl std::fmt::Display) -> ProgrammeFixtureError {
    ProgrammeFixtureError::Invalid(detail.to_string())
}

fn kg(value: &str) -> Result<Kg, ProgrammeFixtureError> {
    Kg::try_from(value.to_owned()).map_err(invalid)
}

fn pct(value: &str) -> Result<Percentage, ProgrammeFixtureError> {
    Percentage::try_from(value.to_owned()).map_err(invalid)
}

fn reps(count: u32) -> Result<RepCount, ProgrammeFixtureError> {
    RepCount::new(count).map_err(invalid)
}

/// The zone the corpus was trained in.
///
/// # Errors
///
/// [`ProgrammeFixtureError`] if the identifier is not one the database knows.
pub fn zone() -> Result<TimeZone, ProgrammeFixtureError> {
    TimeZone::get("Europe/London").map_err(invalid)
}

/// The parameters, with a **test** ladder climb.
///
/// The back-off percentage and the warm-up ramp are the operator's own. The
/// light-of-heavy percentage, the accessory range and the per-role repetitions are
/// inferred from the record; see the module note.
///
/// # Errors
///
/// [`ProgrammeFixtureError`] if any literal here is not a valid value, which
/// would be a typo in this file.
pub fn parameters() -> Result<GenerationParameters, ProgrammeFixtureError> {
    let warmup = NonEmpty::new(vec![
        WarmupStep {
            of_top_set: pct("40%")?,
            reps: reps(4)?,
        },
        WarmupStep {
            of_top_set: pct("60%")?,
            reps: reps(3)?,
        },
        WarmupStep {
            of_top_set: pct("80%")?,
            reps: reps(2)?,
        },
        WarmupStep {
            of_top_set: pct("90%")?,
            reps: reps(1)?,
        },
    ])
    .map_err(invalid)?;

    Ok(GenerationParameters {
        warmup,
        // The primary's back-off, per role: heavy is 2 x 4 and light is 3 x 6.
        // These used to be read off `strength` below, which issued the light
        // session's pattern on the heavy day.
        back_off: PerRole {
            light: BackOff {
                sets: reps(3)?,
                reps: reps(6)?,
                of_top_set: pct("85%")?,
            },
            heavy: BackOff {
                sets: reps(2)?,
                reps: reps(4)?,
                of_top_set: pct("85%")?,
            },
        },
        light_of_heavy: pct("85%")?,
        // A test rate. See the module note.
        ladder_climb_per_week: kg("2.5")?,
        entry_drop: pct("-10%")?,
        top_set_reps: PerRole {
            light: TopSetReps::new(reps(3)?),
            heavy: TopSetReps::new(reps(1)?),
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
        strength: domain::prescription::AccessoryScheme {
            reps: domain::prescription::Target::spanning(reps(4)?, reps(2)?),
            sets: reps(3)?,
        },
        hypertrophy: domain::prescription::AccessoryScheme {
            reps: domain::prescription::Target::spanning(reps(4)?, reps(2)?),
            sets: reps(3)?,
        },
        static_hold: domain::gym::Duration::from_seconds(60),
        first_reset: ResetProtocol {
            drop: pct("-10%")?,
            reclimb_per_week: kg("5")?,
        },
        second_reset: ResetProtocol {
            drop: pct("-5%")?,
            reclimb_per_week: kg("2.5")?,
        },
    })
}

/// The eleven slot fills the record shows.
///
/// Fallible again since the static slots carry repetition counts, which are
/// constructed rather than taken by value.
///
/// # Errors
///
/// [`ProgrammeFixtureError`] if a literal here is invalid.
pub fn fills() -> Result<SlotFills, ProgrammeFixtureError> {
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
        knee_dominant: Fill::Same(Exercise::Reps(RepsExercise::FrontSquat)),
        upper_push: Fill::Same(Exercise::Reps(RepsExercise::ChestDip)),
        upper_pull: Fill::Same(Exercise::Reps(RepsExercise::NeutralGripPullUp)),
        // Alternating: the reason the history projection is unbounded.
        hip_dominant: Fill::Alternating(PerRole {
            light: Exercise::Reps(RepsExercise::BackExtensionMachine),
            heavy: Exercise::Reps(RepsExercise::NordicHamstringsCurls),
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
/// [`ProgrammeFixtureError`] if the date or load is invalid.
pub fn anchor() -> Result<Anchor, ProgrammeFixtureError> {
    let from = Date::new(2026, 7, 3).map_err(invalid)?;
    // The 3 July test: a completed single at 90, then a failed 95. The failed
    // load is what the block opens at.
    Anchor::new(kg("90")?, Some(kg("95")?), AnchorProvenance::Tested, from).map_err(invalid)
}

/// Monday light, Friday heavy — what the record has run since June.
///
/// # Errors
///
/// [`ProgrammeFixtureError`] if the list is empty.
pub fn weekdays() -> Result<Weekdays, ProgrammeFixtureError> {
    Weekdays::new(vec![
        (jiff::civil::Weekday::Monday, SessionRole::Light),
        (jiff::civil::Weekday::Friday, SessionRole::Heavy),
    ])
    .map_err(invalid)
}

/// The block's calendar: eight training weeks from 2026-07-06, uninterrupted.
///
/// # Errors
///
/// [`ProgrammeFixtureError`] if the date or the weekday list is invalid.
pub fn calendar() -> Result<Calendar, ProgrammeFixtureError> {
    calendar_running(weekdays()?, &[])
}

/// The same block, run on given weekdays and skipping given weeks.
///
/// # Errors
///
/// [`ProgrammeFixtureError`] if the date is invalid, or if a named week falls
/// outside the block.
pub fn calendar_running(
    weekdays: Weekdays,
    skipping: &[Skip],
) -> Result<Calendar, ProgrammeFixtureError> {
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
/// [`ProgrammeFixtureError`] if the date or the weekday list is invalid.
pub fn calendar_from(start: Date, weekdays: Weekdays) -> Result<Calendar, ProgrammeFixtureError> {
    Calendar::new(start, 8, &[], weekdays, zone()?).map_err(invalid)
}

/// A whole programme, ready to prescribe from.
///
/// Eight weeks from 2026-07-06, gating on the heavy session — the block the
/// record was trained under, with the test span from [`parameters`].
///
/// # Errors
///
/// [`ProgrammeFixtureError`] if the programme is inconsistent, which would be a
/// mistake in this file rather than in the code under test.
/// What the fixtures call themselves.
///
/// One name across the fixtures, so two of them authored into one store are
/// versions of one programme rather than rivals for the same days — which is
/// what most of these tests want. A test about succession names its own.
pub const FIXTURE_NAME: &str = "fixture";

/// # Errors
///
/// [`ProgrammeFixtureError`] if the text is not a usable programme name.
pub fn name(text: &str) -> Result<ProgrammeName, ProgrammeFixtureError> {
    ProgrammeName::try_from(text.to_owned()).map_err(invalid)
}

/// A linear programme, as one of the three things a programme can be.
///
/// The fixtures below build a `Linear` because that is what they are about;
/// every port takes a `Programme`, so this is the one line between them.
#[must_use]
pub const fn as_programme(linear: Linear) -> Programme {
    Programme::Periodisation(Periodisation::Linear(linear))
}

pub fn programme() -> Result<Linear, ProgrammeFixtureError> {
    programme_skipping(&[])
}

/// The same programme, with sessions it does not run.
///
/// # Errors
///
/// [`ProgrammeFixtureError`] if the programme is inconsistent, or if a skip
/// falls outside the block.
pub fn programme_skipping(skips: &[Skip]) -> Result<Linear, ProgrammeFixtureError> {
    let parameters = parameters()?;
    Linear::new(
        name(FIXTURE_NAME)?,
        Primary::new(
            domain::prescription::PrimaryPattern::KneeDominant,
            Exercise::Reps(RepsExercise::FrontSquat),
            SessionRole::Heavy,
        ),
        fills()?,
        // These fixtures derive their opening from the anchor's entry test.
        Entry::derived(anchor()?),
        calendar_running(weekdays()?, skips)?,
        &parameters,
    )
    .map_err(invalid)
}

/// The same programme, started on a given date.
///
/// # Errors
///
/// [`ProgrammeFixtureError`] if the programme is inconsistent, which would be a
/// mistake in this file rather than in the code under test.
pub fn programme_from(start: Date) -> Result<Linear, ProgrammeFixtureError> {
    programme_named_from(FIXTURE_NAME, start)
}

/// A programme with its own name and start, for succeeding another one.
///
/// # Errors
///
/// [`ProgrammeFixtureError`] if the name is unusable or the programme is
/// inconsistent.
pub fn programme_named_from(called: &str, start: Date) -> Result<Linear, ProgrammeFixtureError> {
    let parameters = parameters()?;
    Linear::new(
        name(called)?,
        Primary::new(
            domain::prescription::PrimaryPattern::KneeDominant,
            Exercise::Reps(RepsExercise::FrontSquat),
            SessionRole::Heavy,
        ),
        fills()?,
        // These fixtures derive their opening from the anchor's entry test.
        Entry::derived(anchor()?),
        calendar_from(start, weekdays()?)?,
        &parameters,
    )
    .map_err(invalid)
}

/// A programme whose gating role it never runs.
///
/// One of the three inconsistencies the types cannot catch, built here so the
/// test asserting it is refused does not have to construct it inline.
///
/// # Errors
///
/// [`ProgrammeFixtureError`] only if a literal here is invalid; the programme
/// itself is expected to be refused, which the caller asserts.
pub fn gating_on_a_role_it_never_runs()
-> Result<Result<Linear, domain::prescription::InconsistentProgramme>, ProgrammeFixtureError> {
    let parameters = parameters()?;
    // Monday only, and Monday is light — so a heavy gate never fires.
    let monday_only =
        Weekdays::new(vec![(jiff::civil::Weekday::Monday, SessionRole::Light)]).map_err(invalid)?;
    Ok(Linear::new(
        name(FIXTURE_NAME)?,
        Primary::new(
            domain::prescription::PrimaryPattern::KneeDominant,
            Exercise::Reps(RepsExercise::FrontSquat),
            SessionRole::Heavy,
        ),
        fills()?,
        // These fixtures derive their opening from the anchor's entry test.
        Entry::derived(anchor()?),
        calendar_running(monday_only, &[])?,
        &parameters,
    ))
}

/// A programme whose primary is counted in something other than repetitions.
///
/// # Errors
///
/// [`ProgrammeFixtureError`] only if a literal here is invalid.
pub fn primary_not_counted_in_reps()
-> Result<Result<Linear, domain::prescription::InconsistentProgramme>, ProgrammeFixtureError> {
    let parameters = parameters()?;
    Ok(Linear::new(
        name(FIXTURE_NAME)?,
        Primary::new(
            domain::prescription::PrimaryPattern::KneeDominant,
            Exercise::Distance(DistanceExercise::Running),
            SessionRole::Heavy,
        ),
        fills()?,
        // These fixtures derive their opening from the anchor's entry test.
        Entry::derived(anchor()?),
        calendar()?,
        &parameters,
    ))
}

/// A programme naming one exercise as primary and filling the slot with another.
///
/// # Errors
///
/// [`ProgrammeFixtureError`] only if a literal here is invalid.
pub fn primary_does_not_fill_its_slot()
-> Result<Result<Linear, domain::prescription::InconsistentProgramme>, ProgrammeFixtureError> {
    let parameters = parameters()?;
    Ok(Linear::new(
        name(FIXTURE_NAME)?,
        // Names the knee-dominant slot as primary, but the primary exercise is a
        // deadlift, and the knee-dominant fill is a front squat.
        Primary::new(
            domain::prescription::PrimaryPattern::KneeDominant,
            Exercise::Reps(RepsExercise::DeadliftBarbell),
            SessionRole::Heavy,
        ),
        fills()?,
        // These fixtures derive their opening from the anchor's entry test.
        Entry::derived(anchor()?),
        calendar()?,
        &parameters,
    ))
}
