//! Estimated 1RM as a trend, one planned macrocycle at a time (#420).

use domain::{
    analytical::Rts,
    analytical::{
        Estimate,
        one_rep_max::{Line, NextTest, Point, Reading, points as read, stretches, tested},
    },
    canonical::{Attributed, NormalisedSessionId, Occurred},
    gym::{
        CanonicalExercise, CanonicalGymSession, CanonicalItem, CanonicalSet, Identified, Load,
        Performed, Rir, exercise::RepsExercise,
    },
    measure::{Kg, RepCount},
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

/// The entry test on 2026-09-25, and the five sessions since, as a copy of the
/// store had them on 2026-10-14.
fn autumn() -> Option<Vec<Point>> {
    let mut points = points(&[
        (date(2026, 9, 29), 90_625),
        (date(2026, 10, 2), 96_774),
        (date(2026, 10, 5), 94_117),
        (date(2026, 10, 9), 94_117),
        (date(2026, 10, 12), 94_594),
    ])?;
    points.insert(0, test_on(date(2026, 9, 25), 92_500)?);
    Some(points)
}

fn test_on(on: Date, grams: u64) -> Option<Point> {
    Some(Point {
        on,
        estimate: Estimate {
            one_rep_max: Kg::from_grams(grams),
            load: Kg::from_grams(grams),
            reps: RepCount::new(1).ok()?,
            rir: Rir::Zero,
        },
        reading: Reading::Tested {
            failed: Kg::from_grams(grams + 2_500),
        },
    })
}

fn days(points: &[Point]) -> Vec<Date> {
    points.iter().map(|point| point.on).collect()
}

fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1.0,
        "{actual} is not {expected} to the gram"
    );
}

/// The next test three planned sessions after the last one performed.
const NEXT: NextTest = NextTest {
    on: date(2026, 10, 23),
    sessions: 3,
};

#[test]
fn a_stretch_runs_from_a_test_through_the_estimates_after_it() {
    let points = autumn().expect("points");
    let stretches = stretches(&points, &days(&points), Some(NEXT));
    let [stretch] = stretches.as_slice() else {
        panic!("one stretch: {stretches:?}");
    };
    assert_eq!(stretch.test.on, date(2026, 9, 25));
    let sessions: Vec<u32> = stretch.estimates.iter().map(|(at, _)| *at).collect();
    assert_eq!(sessions, [1, 2, 3, 4, 5], "sessions since the test");
    assert_eq!(stretch.closed_by, None);
}

#[test]
fn the_line_is_the_estimates_alone_and_projects_to_the_next_test() {
    let points = autumn().expect("points");
    let stretches = stretches(&points, &days(&points), Some(NEXT));
    let stretch = stretches.first().expect("a stretch");
    let line = stretch.line.expect("five estimates give a line");
    near(line.per_session(), 528.1);
    near(line.at(1), 92_989.2);
    near(line.band(1), 5_809.519);
    assert_eq!((line.first(), line.last()), (1, 5));
    let projection = stretch.projection.expect("a test still to come");
    assert_eq!((projection.on, projection.session), (date(2026, 10, 23), 8));
    near(projection.mass, 96_685.9);
    near(projection.low, 82_259.254);
    near(projection.high, 111_112.546);
}

#[test]
fn a_test_performed_closes_its_stretch_and_starts_the_next() {
    let mut points = autumn().expect("points");
    points.push(test_on(date(2026, 10, 23), 97_500).expect("a test"));
    let mut performed = days(&points);
    performed.insert(6, date(2026, 10, 19));
    let stretches = stretches(&points, &performed, Some(NEXT));
    let [first, second] = stretches.as_slice() else {
        panic!("two stretches: {stretches:?}");
    };
    assert_eq!(
        first.closed_by.map(|(at, point)| (at, point.on)),
        Some((7, date(2026, 10, 23))),
        "a session with no estimate still counts"
    );
    assert_eq!(
        first
            .projection
            .map(|projection| (projection.session, projection.on)),
        Some((7, date(2026, 10, 23))),
        "projected to the test that ended it"
    );
    assert_eq!(second.test.on, date(2026, 10, 23));
    assert!(second.estimates.is_empty() && second.line.is_none());
}

#[test]
fn two_estimates_are_points_and_no_line() {
    let points = autumn().expect("points");
    let early: Vec<Point> = points.into_iter().take(3).collect();
    let stretches = stretches(&early, &days(&early), Some(NEXT));
    let stretch = stretches.first().expect("a stretch");
    assert_eq!(stretch.estimates.len(), 2);
    assert_eq!(stretch.line, None);
    assert_eq!(stretch.projection, None);
}

#[test]
fn no_test_is_no_stretch() {
    let points = points(&[(date(2026, 9, 29), 90_625)]).expect("points");
    assert!(stretches(&points, &days(&points), Some(NEXT)).is_empty());
}

#[test]
fn sessions_on_one_day_give_no_line() {
    assert_eq!(
        Line::through(&[(1, 90_000.0), (1, 92_000.0), (1, 94_000.0)]),
        None
    );
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

#[test]
fn a_reserve_recorded_beneath_the_failure_is_no_test() {
    // 2026-07-03: a warm-up at 80, 90 × 1 @ 1, then 95 failed.
    let session = front_squat(
        date(2026, 7, 3),
        vec![
            set(800, Some(1), None),
            set(900, Some(1), Some(Rir::One)),
            set(950, None, None),
        ],
    )
    .expect("a session");
    assert_eq!(
        tested(&Rts, &session, RepsExercise::FrontSquatBarbell),
        None
    );
}
