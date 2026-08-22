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
    gym::{
        Kg, RepCount,
        exercise::{DurationExercise, Exercise, RepsExercise},
    },
    prescription::{
        Anchor, AnchorProvenance, BlockWeek, Entry, EntryTest, Fill, InconsistentProgramme,
        PerRole, Periodisation, Periodised, Primary, PrimaryPattern, Programme, ProgrammeName,
        SessionRole, Skip, SlotFills, StaticFill, Test, TestTarget, Tested, WeekIndex, Weekdays,
    },
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

fn name(value: &str) -> Result<ProgrammeName, Invalid> {
    ProgrammeName::try_from(value.to_owned()).map_err(invalid)
}

fn date(year: i16, month: i8, day: i8) -> Result<Date, Invalid> {
    Date::new(year, month, day).map_err(invalid)
}

/// Monday light, Friday heavy — the operator's own week.
fn weekdays() -> Result<Weekdays, Invalid> {
    Weekdays::new(vec![
        (jiff::civil::Weekday::Monday, SessionRole::Light),
        (jiff::civil::Weekday::Friday, SessionRole::Heavy),
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

/// An anchor dated before any programme these tests build.
fn anchor(provenance: AnchorProvenance) -> Result<Anchor, Invalid> {
    let load = Kg::try_from("90".to_owned()).map_err(invalid)?;
    Anchor::new(load, None, provenance, date(2026, 9, 18)?).map_err(invalid)
}

/// A ten-week block opening on 21 September, three days after its entry test.
///
/// **Two shapes, and the provenance rule is the whole difference.** A block
/// handed a test taken before it opens from a measured maximum; a block that
/// runs its own opens from what the operator expects, and its first week finds
/// out.
fn block(
    provenance: AnchorProvenance,
    entry_test: Option<EntryTest>,
) -> Result<Result<Periodised, InconsistentProgramme>, Invalid> {
    let calendar = Periodised::weeks(
        date(2026, 9, 21)?,
        10,
        entry_test.is_some(),
        &[] as &[Skip],
        weekdays()?,
        TimeZone::UTC,
    )
    .map_err(invalid)?;
    Ok(Periodised::new(
        name("autumn")?,
        Primary::new(
            PrimaryPattern::KneeDominant,
            Exercise::Reps(RepsExercise::FrontSquat),
            SessionRole::Heavy,
        ),
        fills(Fill::Same(Exercise::Reps(RepsExercise::FrontSquat)))?,
        Entry::derived(anchor(provenance)?),
        entry_test,
        calendar,
    ))
}

/// A three-repetition entry test, with no light session.
fn entry_test() -> Result<EntryTest, Invalid> {
    EntryTest::new(reps(3)?, None).map_err(invalid)
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
    weekdays: Weekdays,
) -> Result<Result<Test, InconsistentProgramme>, Invalid> {
    let week =
        Test::week(date(2026, 9, 14)?, &[] as &[Skip], weekdays, TimeZone::UTC).map_err(invalid)?;
    Ok(Test::new(
        name("autumn-entry-test")?,
        Tested::new(
            PrimaryPattern::KneeDominant,
            Exercise::Reps(RepsExercise::FrontSquat),
            reps(reps_at)?,
        ),
        fills(knee_dominant)?,
        week,
        TestTarget::Inherited,
    ))
}

// ---------------------------------------------------------------------------

/// A test occupies exactly one week, and says so to the overlap rule.
///
/// The rule that two programmes may not compete for a day reads the window, so a
/// test claiming more than its week would refuse the block that follows it three
/// days later.
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
    let window = test.window();
    let Ok(monday) = Date::new(2026, 9, 14) else {
        panic!("14 September is a date")
    };
    let Ok(next_monday) = Date::new(2026, 9, 21) else {
        panic!("21 September is a date")
    };
    assert!(window.covers(monday), "the test covers its own Monday");
    assert!(
        !window.covers(next_monday),
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
    let inherited = Fill::Alternating(PerRole {
        light: Exercise::Reps(RepsExercise::SquatBarbell),
        heavy: Exercise::Reps(RepsExercise::FrontSquat),
    });
    let Ok(Ok(test)) = test(inherited, 1, weekdays) else {
        panic!("a week that back squats light and tests the front squat is a test")
    };
    assert_eq!(
        test.fills()
            .primary(PrimaryPattern::KneeDominant, SessionRole::Light),
        &Exercise::Reps(RepsExercise::SquatBarbell)
    );
    assert!(test.is_tested(PrimaryPattern::KneeDominant.slot(), SessionRole::Heavy));
    assert!(
        !test.is_tested(PrimaryPattern::KneeDominant.slot(), SessionRole::Light),
        "the light session is the predecessor's, not a second attempt at the maximum"
    );
}

/// A week that never runs the heavy session never takes the test.
#[test]
fn a_test_that_never_runs_its_session_is_refused() {
    let Ok(mondays) = Weekdays::new(vec![(jiff::civil::Weekday::Monday, SessionRole::Light)])
    else {
        panic!("one day is a weekday map")
    };
    let Ok(refused) = test(
        Fill::Same(Exercise::Reps(RepsExercise::FrontSquat)),
        1,
        mondays,
    ) else {
        panic!("the fixture builds")
    };
    assert!(matches!(
        refused,
        Err(InconsistentProgramme::TestNeverRunsItsSession {
            role: SessionRole::Heavy
        })
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
        Err(InconsistentProgramme::TestRepsTooMany { reps: 41 })
    ));
}

/// A block does not decide for itself whether its anchor had to be measured.
///
/// **The rule left this type on 2026-08-22.** Whether a block may state a number
/// outright depends on what precedes it — nothing, an unusable test, or a
/// measurement it should have opened from — and that is a fact about the store.
/// `Periodised` sees one programme, so it accepts all three provenances and
/// `Authoring` refuses the one that is wrong; `tests/composition.rs` at the
/// adapter's ring is where that rule is asserted.
#[test]
fn a_block_accepts_any_provenance_on_its_own() {
    for provenance in [
        AnchorProvenance::Asserted,
        AnchorProvenance::Estimated,
        AnchorProvenance::Tested,
    ] {
        let Ok(built) = block(provenance, None) else {
            panic!("the fixture builds")
        };
        assert!(
            built.is_ok(),
            "a {provenance} anchor is not a programme this type can refuse"
        );
    }
}

/// The entry test takes a week in front of the phases, and counts for none.
#[test]
fn an_entry_test_adds_a_week_and_shifts_the_phases() {
    let (Ok(test), Ok(Ok(without))) = (entry_test(), block(AnchorProvenance::Tested, None)) else {
        panic!("the fixture builds")
    };
    let Ok(Ok(with)) = block(AnchorProvenance::Tested, Some(test)) else {
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
    let Ok(Ok(block)) = block(AnchorProvenance::Tested, None) else {
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
fn a_test_has_no_anchor_and_no_gating_role() {
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
    let programme = Programme::Test(test);
    assert_eq!(programme.template(), "test");
    assert_eq!(
        programme.anchor(),
        None,
        "a test produces one, never reads one"
    );
    assert_eq!(programme.gating_role(), None, "and gates nothing");

    let Ok(Ok(block)) = block(AnchorProvenance::Tested, None) else {
        panic!("a tested anchor makes a block")
    };
    let programme = Programme::Periodisation(Periodisation::Block(block));
    assert_eq!(programme.template(), "block");
    assert!(programme.anchor().is_some());
    assert_eq!(programme.gating_role(), Some(SessionRole::Heavy));
}
