//! Resting heart rate as a slow line and an illness flag (#417).

use domain::{
    analytical::{
        per_device::Reading,
        resting::{flags, line},
    },
    measure::{BeatsPerMinute, InvalidQuantity},
};
use jiff::civil::{Date, date};

type Night = Reading<BeatsPerMinute>;

fn nights(each: impl IntoIterator<Item = (Date, u32)>) -> Result<Vec<Night>, InvalidQuantity> {
    each.into_iter()
        .map(|(on, beats)| {
            Ok(Reading {
                on,
                value: BeatsPerMinute::new(beats)?,
            })
        })
        .collect()
}

fn days(from: Date, count: usize) -> impl Iterator<Item = Date> {
    from.series(jiff::Span::new().days(1)).take(count)
}

/// 28 nights alternating 50 and 52, then `after` from 29 January.
fn normal_then(after: &[u32]) -> Result<Vec<Night>, InvalidQuantity> {
    let normal = days(date(2026, 1, 1), 28)
        .enumerate()
        .map(|(index, on)| (on, if index % 2 == 0 { 50 } else { 52 }));
    let then = days(date(2026, 1, 29), after.len()).zip(after.iter().copied());
    nights(normal.chain(then))
}

#[test]
fn the_line_is_the_mean_of_ninety_days_and_needs_thirty_nine() {
    let readings = nights(days(date(2026, 1, 1), 50).map(|on| (on, 50))).expect("nights");
    let runs = line(&readings);
    assert_eq!(runs.len(), 1);
    // The 39th day is the first with 39 of the 90 recorded.
    assert_eq!(runs[0].first().map(|day| day.on), Some(date(2026, 2, 8)));
    assert!(runs[0].iter().all(|day| (day.bpm - 50.0).abs() < 1e-9));
}

#[test]
fn two_high_nights_running_flag_the_second() {
    // The normal is 51 with a spread of about 1; 56 is far above it.
    let flagged = flags(&normal_then(&[56, 56, 51]).expect("nights"));
    assert_eq!(
        flagged.iter().map(|flag| flag.on).collect::<Vec<_>>(),
        [date(2026, 1, 30)]
    );
    // The second night's 28 include the first high one.
    assert!((flagged[0].normal - 51.25).abs() < 0.1);
}

#[test]
fn one_high_night_is_not_a_flag() {
    assert!(flags(&normal_then(&[60, 51, 60]).expect("nights")).is_empty());
}

#[test]
fn a_missing_night_between_two_high_ones_is_not_two_running() {
    let mut readings = normal_then(&[56]).expect("nights");
    readings.extend(nights([(date(2026, 1, 31), 56)]).expect("a night"));
    assert!(flags(&readings).is_empty());
}

#[test]
fn too_few_nights_before_is_no_normal() {
    let readings = nights(
        days(date(2026, 1, 1), 11)
            .map(|on| (on, 50))
            .chain([(date(2026, 1, 12), 70), (date(2026, 1, 13), 70)]),
    )
    .expect("nights");
    assert!(flags(&readings).is_empty());
}
