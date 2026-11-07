//! The FTP tests, in watts and over body weight (#469).

use domain::{
    analytical::{
        Weighed,
        body_weight::Direction,
        ftp_test::{Moved, PowerToWeight, Since, Test, figures, tests, within},
        range::Range,
    },
    canonical::Occurred,
    cycling::{Ftp, FtpProvenance, Watts},
    measure::Kg,
    normalised::{OperatorZone, StartedAt},
};
use jiff::civil::{Date, date};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

fn at(on: Date, hour: i8) -> Fallible<StartedAt> {
    let zone = OperatorZone::try_from("Europe/London".to_owned())?;
    let instant = on
        .at(hour, 0, 0, 0)
        .to_zoned(zone.as_time_zone())?
        .timestamp();
    Ok(StartedAt::new(instant, zone))
}

fn ftp(on: Date, watts: u32, provenance: FtpProvenance) -> Fallible<(Ftp, StartedAt)> {
    Ok((
        Ftp::new(Watts::from_u32(watts), on, provenance)?,
        at(on, 12)?,
    ))
}

fn tested(on: Date, watts: u32) -> Fallible<(Ftp, StartedAt)> {
    ftp(on, watts, FtpProvenance::Estimated)
}

fn weighed(on: Date, hour: i8, grams: u64) -> Fallible<Weighed> {
    Ok(Weighed {
        at: Occurred::At(at(on, hour)?),
        mass: Kg::from_grams(grams),
        fat_free: None,
    })
}

/// The operator's tests from May to October 2026, as the store holds them:
/// 10 May unweighed, the rest weighed that morning.
fn autumn() -> Fallible<Vec<Test>> {
    Ok(tests(
        &[
            tested(date(2026, 9, 23), 177)?,
            tested(date(2026, 5, 10), 155)?,
            tested(date(2026, 10, 25), 182)?,
            tested(date(2026, 7, 22), 172)?,
        ],
        &[
            weighed(date(2026, 7, 22), 7, 84_362)?,
            weighed(date(2026, 9, 23), 7, 83_877)?,
            weighed(date(2026, 10, 25), 7, 84_191)?,
            weighed(date(2026, 10, 26), 7, 80_000)?,
        ],
    ))
}

fn watts(tests: &[Test]) -> Vec<Option<Direction>> {
    tests
        .iter()
        .map(|test| test.watts_moved.and_then(|moved| moved.direction))
        .collect()
}

#[test]
fn a_test_moves_only_beyond_the_noise_of_two_tests() {
    let tests = autumn().expect("the tests");
    let days: Vec<Date> = tests.iter().map(Test::on).collect();
    assert_eq!(
        days,
        [
            date(2026, 5, 10),
            date(2026, 7, 22),
            date(2026, 9, 23),
            date(2026, 10, 25)
        ],
        "oldest first"
    );
    assert_eq!(
        tests[0].watts_moved, None,
        "the first has nothing before it"
    );
    assert_eq!(
        tests[1].watts_moved,
        Some(Moved {
            by: 17,
            direction: Some(Direction::Rising),
            since: None
        }),
        "+17 W against 155 W's ± 12.5"
    );
    assert_eq!(
        tests[3].watts_moved,
        Some(Moved {
            by: 5,
            direction: None,
            since: None
        }),
        "+5 W against 177 W's ± 14.2, and +10 W against 172 W's ± 13.8"
    );
    assert_eq!(watts(&tests), [None, Some(Direction::Rising), None, None]);
}

#[test]
fn small_steps_one_way_move_once_they_add_up_from_the_start_of_the_run() {
    // The operator, 2026-11-02: "what if my next test is 187?"
    let tests = tests(
        &[
            tested(date(2026, 5, 10), 155).expect("a test"),
            tested(date(2026, 7, 22), 172).expect("a test"),
            tested(date(2026, 9, 23), 177).expect("a test"),
            tested(date(2026, 10, 25), 182).expect("a test"),
            tested(date(2026, 11, 25), 187).expect("a test"),
        ],
        &[],
    );
    assert_eq!(
        tests[4].watts_moved,
        Some(Moved {
            by: 5,
            direction: Some(Direction::Rising),
            since: Some(Since {
                on: date(2026, 7, 22),
                by: 15
            })
        }),
        "+15 W against 172 W's ± 13.8"
    );
}

#[test]
fn a_clear_change_starts_a_new_run() {
    let tests = tests(
        &[
            tested(date(2026, 7, 22), 172).expect("a test"),
            tested(date(2026, 9, 23), 182).expect("a test"),
            tested(date(2026, 10, 25), 160).expect("a test"),
            tested(date(2026, 11, 25), 170).expect("a test"),
        ],
        &[],
    );
    assert_eq!(
        watts(&tests),
        [None, None, Some(Direction::Falling), None],
        "170 is read against 160, where the run starts, not against 172"
    );
}

#[test]
fn per_kilo_reads_the_same_day_s_weigh_in() {
    let tests = autumn().expect("the tests");
    assert_eq!(tests[0].body, None, "10 May was not weighed");
    assert_eq!(tests[0].per_kilo, None);
    assert_eq!(
        tests[3].body,
        Some(Kg::from_grams(84_191)),
        "not the next day's"
    );
    assert_eq!(
        tests[3].per_kilo.map(|ratio| ratio.to_string()),
        Some("2.16W/kg".to_owned())
    );
    assert_eq!(
        tests[1].per_kilo.map(|ratio| ratio.to_string()),
        Some("2.03W/kg".to_owned()),
        "172 / 84.362 is 2.0388, rounded down"
    );
}

#[test]
fn per_kilo_moves_only_against_a_weighed_test_immediately_before() {
    let tests = autumn().expect("the tests");
    assert_eq!(
        tests[1].per_kilo_moved, None,
        "22 Jul's test before, 10 May, was not weighed"
    );
    assert_eq!(
        tests[2].per_kilo_moved,
        Some(Moved {
            by: 8,
            direction: None,
            since: None
        }),
        "2.03 to 2.11, within 2.03's ± 0.16"
    );
    assert_eq!(
        tests[3].per_kilo_moved.and_then(|moved| moved.direction),
        None,
        "2.16 is within the noise of 2.11 and of 22 Jul's 2.03, where its run starts"
    );
}

#[test]
fn the_same_figure_is_no_clear_change() {
    let tests = tests(
        &[
            tested(date(2026, 9, 23), 177).expect("a test"),
            tested(date(2026, 10, 25), 177).expect("a test"),
        ],
        &[],
    );
    assert_eq!(
        tests[1].watts_moved,
        Some(Moved {
            by: 0,
            direction: None,
            since: None
        })
    );
}

#[test]
fn an_asserted_ftp_is_not_a_test() {
    let asserted = ftp(date(2026, 8, 1), 200, FtpProvenance::Asserted).expect("an FTP");
    let tests = tests(
        &[
            tested(date(2026, 7, 22), 172).expect("a test"),
            asserted,
            tested(date(2026, 9, 23), 177).expect("a test"),
        ],
        &[],
    );
    assert_eq!(tests.len(), 2);
    assert_eq!(tests[1].watts_moved.map(|moved| moved.by), Some(5));
}

#[test]
fn a_range_keeps_the_reading_against_a_test_outside_it() {
    let shown = within(
        &autumn().expect("the tests"),
        Range {
            from: date(2026, 9, 1),
            to: date(2026, 10, 28),
        },
    );
    assert_eq!(shown.len(), 2);
    assert_eq!(shown[0].watts_moved.map(|moved| moved.by), Some(5));
}

#[test]
fn no_body_mass_gives_no_ratio() {
    assert_eq!(PowerToWeight::of(Watts::from_u32(182), Kg::NONE), None);
}

/// A test's heart rate (#548): the maximum, the mean, and the mean of the
/// last 5 minutes, over the samples Peloton served, which skip seconds.
#[test]
fn a_tests_heart_rate_is_its_max_mean_and_last_five_minutes() {
    // 140 bpm for the first 15 minutes, 180 for the last 5, a 186 spike at
    // the end, and second 1000 skipped.
    let samples: Vec<(u64, u32)> = (1..=1200)
        .filter(|at| *at != 1000)
        .map(|at| {
            (
                at,
                if at == 1200 {
                    186
                } else if at > 900 {
                    180
                } else {
                    140
                },
            )
        })
        .collect();
    let (max, mean, last) = figures(&samples, 1200).expect("a series with samples has figures");
    assert_eq!(max.as_u32(), 186);
    // 900 × 140 + 298 × 180 + 186 over 1199 samples: 150.0 to the nearest beat.
    assert_eq!(mean.as_u32(), 150);
    // 298 × 180 + 186 over 299: 180.02.
    assert_eq!(last.as_u32(), 180);
}

/// The closing stretch is the last 5 minutes of the ride's own length, so a
/// test Peloton cut a second short is read over its own end.
#[test]
fn the_last_five_minutes_end_at_the_rides_end() {
    let samples: Vec<(u64, u32)> = (1..=1199)
        .map(|at| (at, if at > 899 { 170 } else { 150 }))
        .collect();
    let (_, _, last) = figures(&samples, 1199).expect("a series with samples has figures");
    assert_eq!(last.as_u32(), 170);
}

#[test]
fn no_samples_have_no_figures() {
    assert!(figures(&[], 1200).is_none());
}
