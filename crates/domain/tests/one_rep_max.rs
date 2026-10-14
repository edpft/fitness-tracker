//! Estimated 1RM as a trend, one planned macrocycle at a time (#420).

use domain::{
    analytical::Rts,
    analytical::{
        Estimate,
        one_rep_max::{Line, Point, Reading, by_macrocycle, points as read, tested},
    },
    canonical::{Attributed, NormalisedSessionId, Occurred},
    gym::{
        CanonicalExercise, CanonicalGymSession, CanonicalItem, CanonicalSet, Identified, Load,
        Performed, Rir, exercise::RepsExercise,
    },
    measure::{Kg, RepCount},
    plan::{PlanName, PlanWindow, Span},
    sequence::NonEmpty,
};
use jiff::civil::{Date, date};

/// A session's estimate on each day, read off 80 kg × 3 @ 2.
fn points(days: &[(Date, u64)]) -> Option<Vec<Point>> {
    days.iter()
        .map(|(on, grams)| {
            Some(Point {
                on: *on,
                estimate: Estimate {
                    one_rep_max: Kg::from_grams(*grams),
                    load: Kg::from_grams(80_000),
                    reps: RepCount::new(3).ok()?,
                    rir: Rir::Two,
                },
                reading: Reading::Estimated,
            })
        })
        .collect()
}

fn plan(name: &str, start: Date, weeks: u32) -> Option<PlanWindow> {
    Some(PlanWindow::new(
        PlanName::try_from(name.to_owned()).ok()?,
        Span::new(start, weeks),
    ))
}

/// Front squat on a copy of the store, 2026-10-14: one session before the
/// autumn plan starts and five in it.
fn autumn() -> Option<Vec<Point>> {
    points(&[
        (date(2026, 9, 7), 90_000),
        (date(2026, 9, 29), 90_625),
        (date(2026, 10, 2), 96_774),
        (date(2026, 10, 5), 94_117),
        (date(2026, 10, 9), 94_117),
        (date(2026, 10, 12), 94_594),
    ])
}

fn autumn_plan() -> Option<PlanWindow> {
    plan("2026-autumn", date(2026, 9, 14), 6)
}

fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1.0,
        "{actual} is not {expected} to the gram"
    );
}

#[test]
fn a_macrocycle_is_its_plans_days_only() {
    let planned = by_macrocycle(
        &autumn().expect("points"),
        &[autumn_plan().expect("a plan")],
    );
    let [autumn] = planned.as_slice() else {
        panic!("one macrocycle: {planned:?}");
    };
    assert_eq!(autumn.points.len(), 5, "the 7th is before the plan starts");
    assert_eq!(
        autumn.points.first().map(|point| point.on),
        Some(date(2026, 9, 29))
    );
}

#[test]
fn the_line_is_least_squares_with_a_95_percent_band() {
    let planned = by_macrocycle(
        &autumn().expect("points"),
        &[autumn_plan().expect("a plan")],
    );
    let line = planned
        .first()
        .and_then(|autumn| autumn.line)
        .expect("five sessions give a line");
    near(line.per_week(), 1_055.333);
    let (low, high) = line.per_week_interval();
    near(low, -4_014.726);
    near(high, 6_125.393);
    assert_eq!(
        (line.first(), line.last()),
        (15, 28),
        "first session to last"
    );
    near(line.at(15), 93_080.524);
    near(line.at(28), 95_040.429);
    near(line.band(15), 5_739.775);
    assert!(
        line.band(21) < line.band(15),
        "narrowest near the middle of the sessions"
    );
}

#[test]
fn two_sessions_are_points_and_no_line() {
    let points =
        points(&[(date(2026, 9, 29), 90_625), (date(2026, 10, 2), 96_774)]).expect("points");
    let planned = by_macrocycle(&points, &[autumn_plan().expect("a plan")]);
    assert_eq!(planned.first().map(|autumn| autumn.points.len()), Some(2));
    assert_eq!(planned.first().and_then(|autumn| autumn.line), None);
}

#[test]
fn no_plan_is_no_macrocycle() {
    assert!(by_macrocycle(&autumn().expect("points"), &[]).is_empty());
}

#[test]
fn a_plan_reauthored_is_one_macrocycle_at_its_latest_days() {
    let plans = [
        plan("2026-autumn", date(2026, 9, 7), 2).expect("a plan"),
        autumn_plan().expect("a plan"),
    ];
    let planned = by_macrocycle(&autumn().expect("points"), &plans);
    assert_eq!(planned.len(), 1);
    assert_eq!(
        planned.first().map(|autumn| autumn.span.start()),
        Some(date(2026, 9, 14))
    );
}

#[test]
fn every_planned_macrocycle_oldest_first() {
    let plans = [
        plan("2027-spring", date(2027, 1, 4), 12).expect("a plan"),
        autumn_plan().expect("a plan"),
    ];
    let names: Vec<String> = by_macrocycle(&autumn().expect("points"), &plans)
        .iter()
        .map(|planned| planned.name.to_string())
        .collect();
    assert_eq!(names, ["2026-autumn", "2027-spring"]);
}

#[test]
fn sessions_on_one_day_give_no_line() {
    let day = date(2026, 10, 2);
    assert_eq!(
        Line::through(&[(18, 90_000.0), (18, 92_000.0), (18, 94_000.0)]),
        None
    );
    let points = points(&[(day, 90_000), (day, 92_000), (day, 94_000)]).expect("points");
    let planned = by_macrocycle(&points, &[autumn_plan().expect("a plan")]);
    assert_eq!(planned.first().and_then(|autumn| autumn.line), None);
}

const ACCOUNT: NormalisedSessionId = NormalisedSessionId::FIRST;

/// A front squat set: kilograms, repetitions (`None` for a failure) and the
/// reserve where one was recorded.
fn set(kg: u64, reps: Option<u32>, rir: Option<Rir>) -> Option<CanonicalSet<RepCount>> {
    let outcome = match reps {
        Some(reps) => Performed::Completed(Some(RepCount::new(reps).ok()?)),
        None => Performed::Failed,
    };
    Some(CanonicalSet {
        outcome: Attributed::new(outcome, ACCOUNT),
        load: Some(Attributed::new(
            Load::Absolute(Kg::from_grams(kg * 1_000 / 10)),
            ACCOUNT,
        )),
        began: None,
        intensity: rir.map(|rir| Attributed::new(rir, ACCOUNT)),
        kind: None,
        rest_after: None,
    })
}

/// A session of front squat on `on`, its sets as [`set`] states them.
fn front_squat(on: Date, sets: Vec<Option<CanonicalSet<RepCount>>>) -> Option<CanonicalGymSession> {
    let sets: Vec<CanonicalSet<RepCount>> = sets.into_iter().collect::<Option<_>>()?;
    let exercise = CanonicalExercise::ForReps {
        identified: Attributed::new(
            Identified::Recorded(RepsExercise::FrontSquatBarbell),
            ACCOUNT,
        ),
        sets: NonEmpty::new(sets).ok()?,
    };
    Some(CanonicalGymSession::new(
        Occurred::On(on),
        NonEmpty::of(CanonicalItem::Exercise(exercise), Vec::new()),
        None,
        None,
    ))
}

/// The entry test as the record holds it, 2026-09-25, in tenths of a kilogram.
fn entry_test() -> Option<CanonicalGymSession> {
    front_squat(
        date(2026, 9, 25),
        vec![
            set(350, Some(4), None),
            set(800, Some(1), None),
            set(900, Some(1), None),
            set(925, Some(1), None),
            set(950, None, None),
            set(950, None, None),
        ],
    )
}

#[test]
fn a_set_beneath_a_failed_attempt_is_tested_at_none_in_reserve() {
    let session = entry_test().expect("a session");
    let (estimate, failed) =
        tested(&Rts, &session, RepsExercise::FrontSquatBarbell).expect("a maximal attempt");
    assert_eq!(estimate.one_rep_max, Kg::from_grams(92_500));
    assert_eq!(failed, Kg::from_grams(95_000));
    let points = read(&Rts, &[session], RepsExercise::FrontSquatBarbell);
    assert_eq!(
        points.first().map(|point| point.reading),
        Some(Reading::Tested {
            failed: Kg::from_grams(95_000)
        })
    );
}

#[test]
fn no_failure_is_no_test() {
    let session = front_squat(
        date(2026, 9, 25),
        vec![set(900, Some(1), None), set(925, Some(1), None)],
    )
    .expect("a session");
    assert_eq!(
        tested(&Rts, &session, RepsExercise::FrontSquatBarbell),
        None
    );
    assert!(read(&Rts, &[session], RepsExercise::FrontSquatBarbell).is_empty());
}

#[test]
fn a_higher_estimate_in_the_same_session_is_the_point() {
    // 80 kg × 5 @ 1 reads 94.1 kg; the failure at 85 makes 82.5 × 1 a 82.5 kg test.
    let session = front_squat(
        date(2026, 10, 9),
        vec![
            set(800, Some(5), Some(Rir::One)),
            set(825, Some(1), None),
            set(850, None, None),
        ],
    )
    .expect("a session");
    let points = read(&Rts, &[session], RepsExercise::FrontSquatBarbell);
    assert_eq!(
        points.first().map(|point| point.reading),
        Some(Reading::Estimated)
    );
    assert_eq!(
        points.first().map(|point| point.estimate.one_rep_max),
        Some(Kg::from_grams(94_117))
    );
}
