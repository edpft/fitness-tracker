//! Resting heart rate as a slow line and an illness flag (#417).

use domain::{
    analytical::{
        body_weight::Direction,
        per_device::{DeviceSeries, Reading, SentBy},
        resting::{Change, Day, Trended, flags, line},
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

/// A line of `bpm` on 2 November, its band `margin` either side.
const fn on_the_line(day: i8, month: i8, bpm: f64, margin: Option<f64>) -> Day {
    Day {
        on: date(2026, month, day),
        bpm,
        margin,
    }
}

/// The change from a line at 50 ± 0.8 on 4 August to `bpm` ± 0.6 on 2
/// November: the band of the change is ± 1.0.
fn read(bpm: f64) -> Option<Direction> {
    Change::between(
        &on_the_line(2, 11, bpm, Some(0.6)),
        &on_the_line(4, 8, 50.0, Some(0.8)),
    )
    .direction
}

#[test]
fn rising_or_falling_needs_the_whole_band_one_side_of_no_change() {
    assert_eq!(read(51.01), Some(Direction::Rising));
    assert_eq!(read(51.0), None, "the band reaches no change");
    assert_eq!(read(48.99), Some(Direction::Falling));
    assert_eq!(read(49.0), None, "the band reaches no change");
    assert_eq!(read(50.0), None, "no change is never read as steady");
}

#[test]
fn a_change_with_no_band_has_no_reading() {
    let change = Change::between(
        &on_the_line(2, 11, 40.0, None),
        &on_the_line(4, 8, 50.0, Some(0.8)),
    );
    assert!((change.by + 10.0).abs() < 1e-9);
    assert_eq!(change.margin, None);
    assert_eq!(change.direction, None);
}

#[test]
fn the_change_is_the_latest_day_against_ninety_days_before() {
    // 180 nights at 50, then 30 at 54: the line on the last day is 51.3.
    let readings = nights(
        days(date(2026, 1, 1), 210)
            .enumerate()
            .map(|(index, on)| (on, if index < 180 { 50 } else { 54 })),
    )
    .expect("nights");
    let trended = Trended::of(DeviceSeries {
        sent_by: SentBy::Unknown,
        readings,
    });
    let change = trended.change().expect("a line 90 days before");
    assert_eq!(change.on, date(2026, 7, 29));
    assert!((change.by - 4.0 / 3.0).abs() < 1e-9, "{change:?}");

    let short = Trended::of(DeviceSeries {
        sent_by: SentBy::Unknown,
        readings: nights(days(date(2026, 1, 1), 100).map(|on| (on, 50))).expect("nights"),
    });
    assert_eq!(short.change(), None, "no line 90 days before the last");
}
