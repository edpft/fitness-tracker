//! The FTP tests, in watts and over body weight (#469).

use domain::{
    analytical::{
        Weighed,
        body_weight::Direction,
        ftp_test::{Moved, PowerToWeight, Test, tests, within},
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

#[test]
fn each_test_is_read_against_the_one_before_by_any_difference() {
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
        tests[3].watts_moved,
        Some(Moved {
            by: 5,
            direction: Direction::Rising
        })
    );
    assert_eq!(
        tests[1].watts_moved.map(|moved| moved.direction),
        Some(Direction::Rising)
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
            direction: Direction::Rising
        }),
        "2.03 to 2.11"
    );
    assert_eq!(
        tests[3].per_kilo_moved.map(|moved| moved.direction),
        Some(Direction::Rising)
    );
}

#[test]
fn the_same_figure_is_maintaining() {
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
            direction: Direction::Maintaining
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
