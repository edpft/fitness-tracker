//! The SBS chart, through its public surface.
//!
//! **The chart test is against the workbook, not against the code.** Every
//! prescription below was read off `Squat 2x Int` in the operator's own copy of
//! Stronger By Science's `28 Training Programs`, and is what decision 0024
//! records. If this file and the module disagree, the module is wrong.
//!
//! The second thing worth testing is the mechanism rather than the table: the
//! maximum moves *inside* the cycle, so a percentage day's load depends on what
//! was lifted the week before. `a_cycle_climbs_off_its_own_repetition_maxima`
//! walks a whole four weeks and is the one that would catch a wrong table.

use domain::{
    gym::{Kg, RepCount},
    prescription::{
        SbsDay, SbsSession, advance, day, maximum_after,
        parameters::Percentage,
        sbs::{InvalidSbs, WEEKS, training_max_share, working_load},
        target::Target,
    },
};

/// 2.5 kg, the workbook's own rounding and the operator's plate grid.
const fn increment() -> Kg {
    Kg::from_grams(2_500)
}

fn reps(count: u32) -> Option<RepCount> {
    RepCount::new(count).ok()
}

/// The whole chart, as the workbook states it.
///
/// `(week, session, description)` where a percentage day is `sets × reps @
/// share` and a repetition-maximum day is `reps, back-off sets, low–high`.
#[test]
fn the_chart_is_what_the_workbook_states() {
    let percentage = |week, session, sets: u32, count: u32, points: i32| {
        let Ok(SbsDay::Percentage {
            sets: s,
            reps: r,
            share,
        }) = day(week, session)
        else {
            panic!("week {week} should be a percentage day");
        };
        assert_eq!(s.as_u32(), sets, "week {week} sets");
        assert_eq!(r.as_u32(), count, "week {week} reps");
        assert_eq!(share.as_basis_points(), points, "week {week} share");
    };

    percentage(1, SbsSession::First, 5, 5, 8_000);
    percentage(2, SbsSession::First, 4, 3, 8_500);
    percentage(3, SbsSession::First, 3, 1, 9_000);
    // The operator's transposition of the beginner sheet's taper: five points
    // below the intermediate's own week 1, as the beginner's is below its.
    percentage(4, SbsSession::First, 3, 3, 7_500);

    let rep_max = |week, top: u32, sets: u32, low: u32, high: u32| {
        let Ok(SbsDay::RepMax {
            reps: r,
            back_off_sets,
            back_off_reps,
        }) = day(week, SbsSession::Second)
        else {
            panic!("week {week} day 2 should be a repetition maximum");
        };
        assert_eq!(r.as_u32(), top, "week {week} top set");
        assert_eq!(back_off_sets.as_u32(), sets, "week {week} back-off sets");
        assert_eq!(
            back_off_reps.minimum().as_u32(),
            low,
            "week {week} back-off floor",
        );
        assert_eq!(
            back_off_reps.maximum().as_u32(),
            high,
            "week {week} back-off ceiling",
        );
    };

    rep_max(1, 8, 3, 5, 6);
    rep_max(2, 5, 3, 3, 4);
    rep_max(3, 3, 3, 1, 2);

    let Ok(SbsDay::Test { reps: r }) = day(4, SbsSession::Second) else {
        panic!("week 4 day 2 is the test");
    };
    assert_eq!(r.as_u32(), 1, "and it is a single");
}

#[test]
fn the_cycle_runs_four_weeks_and_no_more() {
    assert_eq!(WEEKS, 4);
    assert_eq!(
        day(5, SbsSession::First),
        Err(InvalidSbs::NoSuchWeek { week: 5 }),
    );
    assert_eq!(
        day(0, SbsSession::Second),
        Err(InvalidSbs::NoSuchWeek { week: 0 }),
    );
}

#[test]
fn only_the_second_session_sets_the_maximum() {
    for week in 1..=WEEKS {
        let first = day(week, SbsSession::First).expect("the week is in the chart");
        assert_eq!(
            first.sets_the_maximum(),
            None,
            "week {week} day 1 programmes from the maximum, it does not set one",
        );

        let second = day(week, SbsSession::Second).expect("the week is in the chart");
        assert!(
            second.sets_the_maximum().is_some(),
            "week {week} day 2 establishes the number week {} programmes from",
            week + 1,
        );
    }
}

#[test]
fn the_sbs_table_is_not_the_domain_s() {
    let share = |count: u32| {
        reps(count)
            .and_then(training_max_share)
            .map(Percentage::as_basis_points)
    };
    assert_eq!(share(8), Some(8_000), "SBS counts an 8RM as 80%");
    assert_eq!(share(5), Some(8_500));
    assert_eq!(share(3), Some(9_000));
    assert_eq!(share(1), Some(10_000), "a single is the whole");

    // The RTS grid in `repmax.rs` says 82.5, 90 and 95 for the same counts.
    // They answer different questions and are kept apart deliberately.
    let rts = |count: u32| {
        reps(count)
            .and_then(domain::prescription::repmax::rep_max)
            .map(Percentage::as_basis_points)
    };
    assert_eq!(rts(8), Some(8_250));
    assert_eq!(rts(5), Some(9_000));
    assert_eq!(rts(3), Some(9_500));

    assert_eq!(
        share(4),
        None,
        "the published table names three counts and this does not extrapolate",
    );
}

#[test]
fn a_repetition_maximum_advances_the_training_maximum() {
    // A 100 kg triple. SBS counts it as 90% of next week's maximum.
    let achieved = Kg::from_grams(100_000);
    let three = reps(3).expect("three is a count");
    let advanced = advance(achieved, three, increment()).expect("the table names a triple");
    assert_eq!(
        advanced.as_grams(),
        110_000,
        "100 / 0.90 is 111.11, and the grid below it is 110.0",
    );
}

#[test]
fn a_load_is_floored_and_never_rounded_up() {
    // 80% of 111 kg is 88.8, which is not on the grid. Flooring gives 87.5;
    // rounding to nearest would give 90 and prescribe a load never demonstrated.
    let maximum = Kg::from_grams(111_000);
    let share = Percentage::from_basis_points(8_000).expect("80% is a percentage");
    let load = working_load(maximum, share, increment()).expect("the load computes");
    assert_eq!(load.as_grams(), 87_500);
    assert!(
        load.as_grams() < 88_800,
        "flooring never prescribes above the true share",
    );
}

#[test]
fn a_count_the_table_does_not_name_advances_nothing() {
    let four = reps(4).expect("four is a count");
    assert_eq!(
        advance(Kg::from_grams(100_000), four, increment()),
        None,
        "a fourth row would be ours rather than SBS's",
    );
}

/// A whole cycle, with the maximum moving off each repetition-maximum day.
///
/// **This is the test that would catch a wrong table**, because every load after
/// week 1 depends on one. The rep maxima below are invented for the walk — they
/// are not the operator's numbers and nothing is fitted to them.
#[test]
fn a_cycle_climbs_off_its_own_repetition_maxima() {
    let step = increment();
    let load_on = |week: u32, maximum: Kg| -> Kg {
        let Ok(SbsDay::Percentage { share, .. }) = day(week, SbsSession::First) else {
            panic!("week {week} day 1 is a percentage day");
        };
        working_load(maximum, share, step).expect("the load computes")
    };

    // Opening maximum, from the standalone week-4 test that runs first.
    let mut maximum = Kg::from_grams(100_000);

    // Week 1: 5×5 at 80% of 100 is 80.
    assert_eq!(load_on(1, maximum).as_grams(), 80_000);
    // He gets 82.5 for eight. SBS counts that as 80% of the new maximum.
    maximum = advance(
        Kg::from_grams(82_500),
        reps(8).expect("eight is a count"),
        step,
    )
    .expect("an 8RM advances");
    assert_eq!(
        maximum.as_grams(),
        102_500,
        "82.5 / 0.80 is 103.125 → 102.5"
    );

    // Week 2: 4×3 at 85% of 102.5 is 87.125 → 85.0.
    assert_eq!(load_on(2, maximum).as_grams(), 85_000);
    maximum = advance(
        Kg::from_grams(90_000),
        reps(5).expect("five is a count"),
        step,
    )
    .expect("a 5RM advances");
    assert_eq!(maximum.as_grams(), 105_000, "90 / 0.85 is 105.88 → 105.0");

    // Week 3: 3×1 at 90% of 105 is 94.5 → 92.5.
    assert_eq!(load_on(3, maximum).as_grams(), 92_500);
    maximum = advance(
        Kg::from_grams(97_500),
        reps(3).expect("three is a count"),
        step,
    )
    .expect("a 3RM advances");
    assert_eq!(maximum.as_grams(), 107_500, "97.5 / 0.90 is 108.3 → 107.5");

    // Week 4 day 1: 3×3 at 75% of 107.5 is 80.625 → 80.
    let taper = load_on(4, maximum);
    assert_eq!(taper.as_grams(), 80_000);

    // **The taper is a taper in percentage and not in kilograms.** Week 1 opened
    // at 80 kg from 80% of 100; week 4 asks 80 kg from 75% of 107.5. The
    // operator was told this and confirmed it (decision 0024).
    assert_eq!(
        taper.as_grams(),
        load_on(1, Kg::from_grams(100_000)).as_grams(),
        "five points lighter against a maximum seven and a half kilos heavier",
    );
}

#[test]
fn a_back_off_range_is_a_range() {
    let Ok(SbsDay::RepMax { back_off_reps, .. }) = day(1, SbsSession::Second) else {
        panic!("week 1 day 2 is a repetition maximum");
    };
    assert!(
        matches!(back_off_reps, Target::Range { .. }),
        "5–6 is a range rather than an exact count",
    );
}

/// The whole progression, as the prescriber drives it: an opening maximum and
/// what was lifted on each repetition-maximum day.
///
/// **This is the mechanism, and it is the ordinary case.** A linear ladder reads
/// the record too; what differs is what it asks. A ladder asks whether the top
/// set was completed, and this asks what it weighed.
#[test]
fn the_maximum_is_what_the_performed_rep_max_days_make_it() {
    let step = increment();
    let opening = Kg::from_grams(100_000);

    // Nothing performed yet — week 1 programmes from the opening, unchanged.
    assert_eq!(
        maximum_after(opening, &[], step).as_grams(),
        100_000,
        "an untrained cycle stands where it was authored",
    );

    // Week 1's eight-rep day made 82.5. 82.5 / 0.80 is 103.125 → 102.5.
    assert_eq!(
        maximum_after(opening, &[(1, Kg::from_grams(82_500))], step).as_grams(),
        102_500,
    );

    // And week 2's five-rep day made 90. Applied in order, on top of the first.
    assert_eq!(
        maximum_after(
            opening,
            &[(1, Kg::from_grams(82_500)), (2, Kg::from_grams(90_000))],
            step,
        )
        .as_grams(),
        105_000,
        "90 / 0.85 is 105.88 → 105.0, from the maximum week 1 left behind",
    );
}

#[test]
fn a_week_nobody_trained_leaves_the_maximum_where_it_was() {
    let step = increment();
    let opening = Kg::from_grams(100_000);

    // Week 1 skipped, week 2 trained. The week 2 result still advances, off the
    // opening rather than off a week that never happened.
    let skipped = maximum_after(opening, &[(2, Kg::from_grams(90_000))], step);
    assert_eq!(skipped.as_grams(), 105_000);
}

#[test]
fn a_week_the_chart_does_not_name_advances_nothing() {
    let step = increment();
    let opening = Kg::from_grams(100_000);
    assert_eq!(
        maximum_after(opening, &[(9, Kg::from_grams(90_000))], step).as_grams(),
        100_000,
        "the chart runs four weeks, and a fifth is not one of them",
    );
}

/// Week 1's percentage day against the maximum a performed week 1 produces.
///
/// Ties the two halves together: the chart states 85% for week 2, and what that
/// is 85% *of* is what week 1's rep-max day made.
#[test]
fn week_two_is_a_share_of_what_week_one_produced() {
    let step = increment();
    let maximum = maximum_after(
        Kg::from_grams(100_000),
        &[(1, Kg::from_grams(82_500))],
        step,
    );

    let Ok(SbsDay::Percentage { share, .. }) = day(2, SbsSession::First) else {
        panic!("week 2 day 1 is a percentage day");
    };
    let load = working_load(maximum, share, step).expect("the load computes");

    assert_eq!(
        load.as_grams(),
        85_000,
        "85% of 102.5 is 87.125, floored to the grid at 85.0",
    );
}
