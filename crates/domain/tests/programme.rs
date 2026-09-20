//! What each of the three templates refuses (decision 0013).
//!
//! **The rules that differ are the interesting ones.** All three share a
//! template, so all three ask whether the primary fills the slot it named; what
//! separates them is what each one is *for*. A linear programme may open from a
//! number the operator asserted, because it declares its own opening. A block
//! may not, because 0013 makes a block's entry requirement the thing that stops
//! a lift change skipping its test. And a test is neither: it has no anchor at
//! all, because producing one is the whole of what it does.
//!
//! Helpers are free functions returning `Result`, so a test unwraps at the call
//! site: the `clippy.toml` exemptions cover `#[test]` bodies and not helpers
//! defined beside them.

use domain::{
    gym::exercise::{DurationExercise, Exercise, RepsExercise},
    measure::RepCount,
    plan::Occupies,
    prescription::{
        Authored, AuthoringError, BlockPeriodisation, BlockWeek, ByIntensity, EntryTest, Fill,
        InconsistentMesocycle, Mesocycle, Primary, PrimaryPattern, Progression, Skip, SlotFills,
        StaticFill, Test, Tested, WeekIndex, authored::Shape as AuthoredShape, seed::seed,
    },
    schedule::{Relative, SessionRole, TrainingWeek},
};
use jiff::{civil::Date, tz::TimeZone};

#[derive(Debug, thiserror::Error)]
#[error("the fixture holds an invalid value: {0}")]
struct Invalid(String);

fn invalid(detail: impl std::fmt::Display) -> Invalid {
    Invalid(detail.to_string())
}

fn reps(count: u32) -> Result<RepCount, Invalid> {
    RepCount::new(count).map_err(invalid)
}

fn date(year: i16, month: i8, day: i8) -> Result<Date, Invalid> {
    Date::new(year, month, day).map_err(invalid)
}

/// Monday light, Friday heavy — the operator's own week.
fn weekdays() -> Result<TrainingWeek, Invalid> {
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

/// Every slot filled, with the knee-dominant one taking whatever is handed in.
///
/// The lower slot is the parameter because it is the only one any test here
/// varies: everything else exists so that `SlotFills` is total.
fn fills(knee_dominant: Fill<Exercise>) -> Result<SlotFills, Invalid> {
    let (three, five, twenty) = (reps(3)?, reps(5)?, reps(20)?);
    let hold = |exercise| Fill::Same(Exercise::Duration(exercise));
    let lift = |exercise| Fill::Same(Exercise::Reps(exercise));
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
        knee_dominant,
        upper_push: lift(RepsExercise::ChestDip),
        upper_pull: lift(RepsExercise::NeutralGripPullUp),
        hip_dominant: lift(RepsExercise::NordicHamstringsCurls),
        biceps: lift(RepsExercise::PreacherCurlBarbell),
        triceps: lift(RepsExercise::OverheadTricepsExtensionCable),
        wrist_flexion: lift(RepsExercise::WristFlexionDumbbell),
        wrist_extension: lift(RepsExercise::WristExtensionDumbbell),
        core: lift(RepsExercise::BentOverCableChop),
        handstand_hold: hold(DurationExercise::HandstandHold),
        dead_hang: hold(DurationExercise::DeadHang),
        hip_flexor_stretch: hold(DurationExercise::CouchStretch),
        hip_external_rotator_stretch: hold(DurationExercise::NinetyNinety),
        hamstring_stretch: hold(DurationExercise::StandingStraddleFold),
        groin_stretch: hold(DurationExercise::SquattingGroinStretch),
    })
}

/// A ten-week block opening on 21 September, three days after its entry test.
///
/// **Two shapes: one that measures its own entry and one that does not.** What
/// either opens from is the maximum in force on the day it starts, which is not
/// something the programme carries.
fn block(
    entry_test: Option<EntryTest>,
) -> Result<Result<BlockPeriodisation, InconsistentMesocycle>, Invalid> {
    let calendar = BlockPeriodisation::weeks(
        date(2026, 9, 21)?,
        10,
        entry_test.is_some(),
        &[] as &[Skip],
        weekdays()?,
        TimeZone::UTC,
    )
    .map_err(invalid)?;
    Ok(BlockPeriodisation::new(
        Primary::new(
            PrimaryPattern::KneeDominant,
            Exercise::Reps(RepsExercise::FrontSquat),
            SessionRole::new(Relative::Higher, Relative::Lower),
        ),
        fills(Fill::Same(Exercise::Reps(RepsExercise::FrontSquat)))?,
        entry_test,
        calendar,
    ))
}

/// A three-repetition entry test, with no light session.
fn entry_test() -> Result<EntryTest, Invalid> {
    EntryTest::new(reps(3)?, None, None).map_err(invalid)
}

/// A test on the week of 14 September: Monday light, Friday the test itself.
///
/// **Two layers of `Result`, deliberately.** The outer one is the fixture
/// failing to build, which is a broken test; the inner one is [`Test::new`]
/// refusing, which is what several of these are asserting. Collapsing them would
/// make a fixture typo look like the rule under test.
fn test(
    knee_dominant: Fill<Exercise>,
    reps_at: u32,
    weekdays: TrainingWeek,
) -> Result<Result<Test, InconsistentMesocycle>, Invalid> {
    let week =
        Test::week(date(2026, 9, 14)?, &[] as &[Skip], weekdays, TimeZone::UTC).map_err(invalid)?;
    Ok(Test::new(
        Tested::new(
            PrimaryPattern::KneeDominant,
            Exercise::Reps(RepsExercise::FrontSquat),
            reps(reps_at)?,
        ),
        fills(knee_dominant)?,
        week,
        None,
        None,
    ))
}

// ---------------------------------------------------------------------------

/// A test occupies exactly one week, and says so to the plan holding it.
///
/// The rule that two mesocycles of one programme may not compete for a day reads
/// the span, so a test claiming more than its week would refuse the block that
/// follows it three days later.
#[test]
fn a_test_occupies_one_week() {
    let Ok(weekdays) = weekdays() else {
        panic!("the operator's week is a weekday map")
    };
    let Ok(Ok(test)) = test(
        Fill::Same(Exercise::Reps(RepsExercise::FrontSquat)),
        1,
        weekdays,
    ) else {
        panic!("a front squat single on the heavy day is a test")
    };
    assert_eq!(test.calendar().duration_weeks(), 1);
    let span = Mesocycle::Test(test).span();
    let Ok(monday) = Date::new(2026, 9, 14) else {
        panic!("14 September is a date")
    };
    let Ok(next_monday) = Date::new(2026, 9, 21) else {
        panic!("21 September is a date")
    };
    assert!(span.covers(monday), "the test covers its own Monday");
    assert!(
        !span.covers(next_monday),
        "and stops before the block that inherits it opens"
    );
}

/// The light session may fill the tested slot with the predecessor's lift.
///
/// This is the shape decision 0013's inheritance produces: the week runs the
/// programme before it on the light day and the test on the heavy one, so where
/// both lifts are knee-dominant they share one slot as the two halves of an
/// alternating fill. The check has to read the *test's own* session, or this is
/// refused as a primary that does not fill its slot.
#[test]
fn the_light_session_may_run_the_predecessors_lift() {
    let Ok(weekdays) = weekdays() else {
        panic!("the operator's week is a weekday map")
    };
    let inherited = Fill::Alternating(ByIntensity {
        lower: Exercise::Reps(RepsExercise::SquatBarbell),
        higher: Exercise::Reps(RepsExercise::FrontSquat),
    });
    let Ok(Ok(test)) = test(inherited, 1, weekdays) else {
        panic!("a week that back squats light and tests the front squat is a test")
    };
    assert_eq!(
        test.fills().primary(
            PrimaryPattern::KneeDominant,
            SessionRole::new(Relative::Lower, Relative::Higher)
        ),
        &Exercise::Reps(RepsExercise::SquatBarbell)
    );
    assert!(test.is_tested(
        PrimaryPattern::KneeDominant.slot(),
        SessionRole::new(Relative::Higher, Relative::Lower)
    ));
    assert!(
        !test.is_tested(
            PrimaryPattern::KneeDominant.slot(),
            SessionRole::new(Relative::Lower, Relative::Higher)
        ),
        "the light session is the predecessor's, not a second attempt at the maximum"
    );
}

/// A week that never runs the higher-intensity session never takes the test.
///
/// **Asked of the authoring, not of `Test::new`.** The check moved there on
/// 2026-09-20 (issue #63): a role belongs to a training slot now, so whether
/// the week offers the one the test is taken in is a question about a
/// programme and a week together — and the week is superseded whenever the
/// operator's life changes, while the programme stays in the store.
#[test]
fn a_test_that_never_runs_its_session_is_refused() {
    let Ok(mondays) = TrainingWeek::new(vec![(
        jiff::civil::Weekday::Monday,
        SessionRole::new(Relative::Lower, Relative::Higher),
    )]) else {
        panic!("one day is a week")
    };
    let (Ok(start), Ok(count)) = (date(2026, 9, 14), reps(1)) else {
        panic!("the fixture builds")
    };
    let Ok(filled) = fills(Fill::Same(Exercise::Reps(RepsExercise::FrontSquat))) else {
        panic!("the fixture builds")
    };
    let Ok(parameters) = seed() else {
        panic!("the seed builds")
    };

    let refused = domain::prescription::authored::programme(
        Authored {
            start,
            pattern: PrimaryPattern::KneeDominant,
            primary_exercise: Exercise::Reps(RepsExercise::FrontSquat),
            week: mondays,
            shape: AuthoredShape::Test {
                reps: count,
                provided: None,
                asserted: None,
            },
        },
        filled,
        &[] as &[Skip],
        TimeZone::UTC,
        &parameters,
    );

    assert!(matches!(
        refused,
        Err(AuthoringError::Mesocycle(
            InconsistentMesocycle::TestNeverRunsItsSession { .. }
        ))
    ));
}

/// A test at a repetition count the table cannot convert is refused.
///
/// The check used to sit on `Block::new`, asking the same question of a number
/// the block held. The number belongs to the test — a block enters on whatever
/// the test measured, converted to a one-rep maximum — so the check followed it.
#[test]
fn a_test_off_the_repetition_maximum_table_is_refused() {
    let Ok(weekdays) = weekdays() else {
        panic!("the operator's week is a weekday map")
    };
    let Ok(refused) = test(
        Fill::Same(Exercise::Reps(RepsExercise::FrontSquat)),
        41,
        weekdays,
    ) else {
        panic!("the fixture builds")
    };
    assert!(matches!(
        refused,
        Err(InconsistentMesocycle::TestRepsTooMany { reps: 41 })
    ));
}

/// The entry test takes a week in front of the phases, and counts for none.
#[test]
fn an_entry_test_adds_a_week_and_shifts_the_phases() {
    let (Ok(test), Ok(Ok(without))) = (entry_test(), block(None)) else {
        panic!("the fixture builds")
    };
    let Ok(Ok(with)) = block(Some(test)) else {
        panic!("the fixture builds")
    };

    assert_eq!(without.phase_weeks(), 10);
    assert_eq!(
        with.phase_weeks(),
        10,
        "the phases are the same ten weeks either way"
    );
    assert_eq!(
        with.calendar().duration_weeks(),
        11,
        "and the week in front"
    );

    let Ok(first) = WeekIndex::new(1) else {
        panic!("one is a week index")
    };
    assert!(
        matches!(with.week(first), Some(BlockWeek::Entry(_))),
        "week one is the measurement"
    );
    assert_eq!(
        without
            .week(first)
            .map(|week| matches!(week, BlockWeek::Entry(_))),
        Some(false),
        "and without an entry test week one is already a phase"
    );
}

/// A block's weeks are its phase weeks, with no entry test among them.
#[test]
fn a_block_plans_exactly_the_weeks_its_calendar_holds() {
    let Ok(Ok(block)) = block(None) else {
        panic!("a tested anchor makes a block")
    };
    let Ok(plan) = block.plan() else {
        panic!("ten weeks is plannable")
    };
    assert_eq!(plan.duration_weeks(), 10);
    assert_eq!(
        plan.weeks().len(),
        10,
        "the entry test is the week before, not one of these"
    );
}

/// The two levels of the enum answer different questions.
#[test]
fn a_test_gates_nothing_and_a_block_gates_a_role() {
    let Ok(weekdays) = weekdays() else {
        panic!("the operator's week is a weekday map")
    };
    let Ok(Ok(test)) = test(
        Fill::Same(Exercise::Reps(RepsExercise::FrontSquat)),
        1,
        weekdays,
    ) else {
        panic!("a front squat single on the heavy day is a test")
    };
    let programme = Mesocycle::Test(test);
    assert_eq!(programme.template(), "test");
    assert_eq!(programme.gating_role(), None, "and gates nothing");

    let Ok(Ok(block)) = block(None) else {
        panic!("a tested anchor makes a block")
    };
    let programme = Mesocycle::Progression(Progression::BlockPeriodisation(block));
    assert_eq!(programme.template(), "block");
    assert_eq!(
        programme.gating_role(),
        Some(SessionRole::new(Relative::Higher, Relative::Lower))
    );
}
