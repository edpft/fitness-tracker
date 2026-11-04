//! Estimated 1RM over the macrocycle, by one line through every session's
//! estimate (#420, #688), and over a body mass (#253).

use domain::{
    analytical::Rts,
    analytical::{
        Estimate, Weighed,
        one_rep_max::{
            Against, Line, Point, Reading, points as read, read_tests, relative, tested, trend,
        },
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

/// The autumn macrocycle's nine sessions on the RTS chart, as a copy of the
/// store held them on 2026-11-04.
fn autumn() -> Option<Vec<Point>> {
    points(&[
        (date(2026, 9, 29), 89_395),
        (date(2026, 10, 2), 98_425),
        (date(2026, 10, 5), 92_699),
        (date(2026, 10, 9), 95_596),
        (date(2026, 10, 12), 93_184),
        (date(2026, 10, 16), 97_613),
        (date(2026, 10, 19), 89_605),
        (date(2026, 10, 23), 97_137),
        (date(2026, 10, 30), 96_899),
    ])
}

const MACROCYCLE: Date = date(2026, 9, 14);
const NEXT: Date = date(2026, 11, 20);

fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1.0,
        "{actual} is not {expected} to the gram"
    );
}

#[test]
fn one_line_runs_through_every_session_by_day() {
    let trend = trend(&autumn().expect("points"), MACROCYCLE, Some(NEXT));
    let line = trend.line.expect("nine sessions give a line");
    assert_eq!((line.first(), line.last()), (15, 46), "days since 14 Sep");
    near(line.per_day(), 104.345);
    near(trend.now().expect("a line"), 96_302.944);
}

#[test]
fn the_change_along_the_line_is_read_against_its_margin() {
    let trend = trend(&autumn().expect("points"), MACROCYCLE, Some(NEXT));
    let change = trend.change.expect("a line");
    near(change.by, 3_234.699);
    near(change.margin, 8_856.855);
    assert_eq!(change.direction, None, "+3.2 kg against ± 8.9 kg");
}

#[test]
fn the_band_narrows_as_sessions_accrue() {
    let trend = trend(&autumn().expect("points"), MACROCYCLE, Some(NEXT));
    assert_eq!(trend.narrowing.len(), 7, "from the third session");
    let first = trend.narrowing.first().expect("a band");
    assert_eq!(first.on, date(2026, 10, 5));
    near(first.value, 95_158.333);
    near(first.band, 69_873.299);
    let last = trend.narrowing.last().expect("a band");
    near(last.value, 96_302.944);
    near(last.band, 5_634.984);
}

#[test]
fn the_line_is_carried_on_to_the_next_test_without_a_range() {
    let trend = trend(&autumn().expect("points"), MACROCYCLE, Some(NEXT));
    let next = trend.next.expect("a test still to come");
    assert_eq!(next.on, NEXT);
    near(next.value, 98_494.192);
}

#[test]
fn sessions_before_the_macrocycle_are_not_in_it() {
    let mut points = points(&[(date(2026, 9, 7), 80_000)]).expect("a point");
    points.extend(autumn().expect("points"));
    let trend = trend(&points, MACROCYCLE, Some(NEXT));
    assert_eq!(trend.figures.len(), 9);
}

#[test]
fn two_sessions_are_points_and_no_line() {
    let early: Vec<Point> = autumn().expect("points").into_iter().take(2).collect();
    let trend = trend(&early, MACROCYCLE, Some(NEXT));
    assert_eq!(trend.figures.len(), 2);
    assert_eq!(trend.line, None);
    assert!(trend.narrowing.is_empty());
    assert_eq!((trend.change, trend.next), (None, None));
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

/// The entry test's day, a 1RM test.
fn declared() -> Option<Vec<(Date, RepCount)>> {
    Some(vec![(date(2026, 9, 25), RepCount::new(1).ok()?)])
}

#[test]
fn a_test_is_its_heaviest_completed_set_at_none_in_reserve() {
    let session = entry_test().expect("a session");
    let one = RepCount::new(1).expect("one rep");
    let (estimate, failed) =
        tested(&Rts, &session, RepsExercise::FrontSquatBarbell, one).expect("a test");
    assert_eq!(estimate.one_rep_max, Kg::from_grams(92_500));
    assert_eq!(failed, Some(Kg::from_grams(95_000)));
    let points = read(
        &Rts,
        &[session],
        RepsExercise::FrontSquatBarbell,
        &declared().expect("the test day"),
    );
    assert_eq!(
        points.first().map(|point| point.reading),
        Some(Reading::Tested {
            failed: Some(Kg::from_grams(95_000))
        })
    );
}

#[test]
fn a_test_needs_no_failure() {
    // The operator, 2026-11-03: a test may end at 0 or 0–1 in reserve.
    let session = front_squat(
        date(2026, 9, 25),
        vec![set(900, Some(1), None), set(925, Some(1), Some(Rir::Zero))],
    )
    .expect("a session");
    let points = read(
        &Rts,
        &[session],
        RepsExercise::FrontSquatBarbell,
        &declared().expect("the test day"),
    );
    assert_eq!(
        points
            .first()
            .map(|point| (point.estimate.one_rep_max, point.reading)),
        Some((Kg::from_grams(92_500), Reading::Tested { failed: None }))
    );
}

#[test]
fn a_failed_attempt_on_a_day_not_prescribed_as_a_test_is_no_test() {
    // A linear progression's miss (operator, 2026-11-03): 80 kg × 5 @ 1 reads
    // 94.1 kg, and the failure at 85 is not a test.
    let session = front_squat(
        date(2026, 10, 9),
        vec![
            set(800, Some(5), Some(Rir::One)),
            set(825, Some(1), None),
            set(850, None, None),
        ],
    )
    .expect("a session");
    let points = read(
        &Rts,
        &[session],
        RepsExercise::FrontSquatBarbell,
        &declared().expect("the test day"),
    );
    assert_eq!(
        points
            .first()
            .map(|point| (point.reading, point.estimate.one_rep_max)),
        // 80 kg × 5 @ 1 on the RTS chart, 83.7%.
        Some((Reading::Estimated, Kg::from_grams(95_579)))
    );
}

#[test]
fn a_reserve_recorded_on_the_heaviest_set_still_tests() {
    // 2026-07-03, or 2026-10-23: 95 kg × 1 @ 0–1, then 100 kg failed (#617).
    let session = front_squat(
        date(2026, 9, 25),
        vec![
            set(800, Some(1), None),
            set(950, Some(1), Some(Rir::One)),
            set(1000, None, None),
        ],
    )
    .expect("a session");
    let one = RepCount::new(1).expect("one rep");
    assert_eq!(
        tested(&Rts, &session, RepsExercise::FrontSquatBarbell, one)
            .map(|(estimate, failed)| (estimate.one_rep_max, failed)),
        Some((Kg::from_grams(95_000), Some(Kg::from_grams(100_000))))
    );
}

#[test]
fn a_rep_max_test_takes_the_heaviest_set_of_its_reps() {
    // The operator, 2026-10-30: 3 × 90 @ 1, 3 × 92.5 @ 0–1, 2 × 95 @ 0 is a
    // 92.5 kg 3RM, whatever the 2 × 95 kg.
    let session = front_squat(
        date(2026, 9, 25),
        vec![
            set(900, Some(3), Some(Rir::One)),
            set(925, Some(3), Some(Rir::Zero)),
            set(950, Some(2), Some(Rir::Zero)),
        ],
    )
    .expect("a session");
    let three = RepCount::new(3).expect("three reps");
    assert_eq!(
        tested(&Rts, &session, RepsExercise::FrontSquatBarbell, three).map(|(estimate, failed)| (
            estimate.load,
            estimate.reps,
            failed
        )),
        Some((Kg::from_grams(92_500), three, None))
    );
}

fn weighed(on: Date, grams: u64, fat_free: Option<u64>) -> Weighed {
    Weighed {
        at: Occurred::On(on),
        mass: Kg::from_grams(grams),
        fat_free: fat_free.map(Kg::from_grams),
    }
}

fn after_the_test() -> Option<Vec<CanonicalGymSession>> {
    Some(vec![
        entry_test()?,
        front_squat(date(2026, 9, 29), vec![set(725, Some(5), Some(Rir::Two))])?,
        front_squat(date(2026, 10, 2), vec![set(750, Some(8), Some(Rir::One))])?,
    ])
}

#[test]
fn a_ratio_is_read_against_that_days_weigh_in_and_a_day_without_one_is_a_gap() {
    let sessions = after_the_test().expect("sessions");
    // 2026-09-29 has no weigh-in; 2026-10-02 has no fat-free mass.
    let weigh_ins = [
        weighed(date(2026, 9, 25), 84_419, Some(65_810)),
        weighed(date(2026, 9, 28), 84_105, Some(65_610)),
        weighed(date(2026, 10, 2), 84_248, None),
    ];
    let lift = RepsExercise::FrontSquatBarbell;
    let body = relative(
        &Rts,
        &sessions,
        lift,
        &declared().expect("the test day"),
        &weigh_ins,
        Against::BodyWeight,
    );
    let read: Vec<(Date, String)> = body
        .iter()
        .map(|one| (one.point.on, one.ratio.to_string()))
        .collect();
    assert_eq!(
        read,
        [
            (date(2026, 9, 25), "1.09×".to_owned()),
            // 75 kg × 8 @ 1 on the RTS chart, 76.2%: 98.4 kg over 84.2 kg.
            (date(2026, 10, 2), "1.16×".to_owned())
        ]
    );
    let lean = relative(
        &Rts,
        &sessions,
        lift,
        &declared().expect("the test day"),
        &weigh_ins,
        Against::FatFreeMass,
    );
    let read: Vec<(Date, String)> = lean
        .iter()
        .map(|one| (one.point.on, one.ratio.to_string()))
        .collect();
    assert_eq!(read, [(date(2026, 9, 25), "1.40×".to_owned())]);
}

/// A test as `points` reads one: its heaviest completed single, and the
/// attempt above it that failed.
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
            failed: Some(Kg::from_grams(grams + 2_500)),
        },
    })
}

/// The front squat tests on a copy of the store, 2026-11-03: each against the
/// one before, beyond 9.7% of it (#688).
#[test]
fn a_test_moves_only_beyond_the_noise_of_two_tests() {
    let tests: Vec<Point> = [
        (date(2026, 7, 3), 90_000),
        (date(2026, 9, 25), 92_500),
        (date(2026, 10, 23), 95_000),
        (date(2026, 12, 18), 105_000),
    ]
    .into_iter()
    .map(|(on, grams)| test_on(on, grams).expect("a test"))
    .collect();
    let read = read_tests(&tests);
    let said: Vec<(Option<i64>, Option<bool>)> = read
        .iter()
        .map(|read| {
            (
                read.moved.map(|moved| moved.by),
                read.moved.map(|moved| moved.direction.is_some()),
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            (None, None),
            (Some(2_500), Some(false)),
            (Some(2_500), Some(false)),
            (Some(10_000), Some(true)),
        ]
    );
    assert_eq!(
        read.get(1).and_then(|read| read.noise).map(f64::round),
        Some(8_731.0),
        "1.96 × √2 × 3.5% of 90 kg"
    );
}

/// Over a macrocycle, a line through all of its tests (operator, 2026-11-03).
#[test]
fn a_macrocycle_is_read_by_a_line_through_its_tests() {
    use domain::{
        analytical::{
            body_weight::Direction,
            one_rep_max::{line_through, macrocycle_line},
        },
        plan::Span,
    };
    let tests = |at: &[(Date, u64)]| -> Vec<Point> {
        at.iter()
            .map(|(on, grams)| test_on(*on, *grams).expect("a test"))
            .collect()
    };
    let autumn = Span::new(date(2026, 9, 14), 14);
    // Two tests give the noise of two tests, of their mean: ± 9.1 kg.
    let two = tests(&[(date(2026, 9, 25), 92_500), (date(2026, 10, 23), 95_000)]);
    let line = line_through(&two, autumn).expect("two tests");
    assert_eq!((line.tests, line.direction), (2, None));
    assert!((line.change - 2_500.0).abs() < 1.0);
    assert!(
        (line.margin - 9_095.0).abs() < 1.0,
        "1.96 × √2 × 3.5% of 93.75 kg"
    );
    // A steady 2.5 kg a test over four tests four weeks apart: the line's
    // margin, 9.2% of their mean (± 8.6 kg), is narrower than first against
    // last (± 8.7 kg at 90 kg), and +7.5 kg is still within it.
    let four = tests(&[
        (date(2026, 9, 18), 90_000),
        (date(2026, 10, 16), 92_500),
        (date(2026, 11, 13), 95_000),
        (date(2026, 12, 11), 97_500),
    ]);
    let line = line_through(&four, autumn).expect("four tests");
    assert!((line.change - 7_500.0).abs() < 1.0);
    assert!((line.margin - 8_628.0).abs() < 1.0, "{line:?}");
    assert_eq!(line.direction, None);
    // A test outside the span is not the macrocycle's.
    let line = line_through(
        &tests(&[(date(2026, 7, 3), 90_000), (date(2026, 9, 25), 92_500)]),
        autumn,
    );
    assert_eq!(line, None);
    assert_eq!(macrocycle_line(&two, &[]), None, "no plan holds them");
    let _ = Direction::Rising;
}
