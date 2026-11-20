//! A slow line on one day against itself 90 days before (#688).

use domain::analytical::{
    body_weight::Direction,
    resting::{Change, Day},
};
use jiff::civil::date;

/// A line of `bpm` on 2 November, its band `margin` either side.
const fn on_the_line(day: i8, month: i8, bpm: f64, margin: Option<f64>) -> Day {
    Day {
        on: date(2026, month, day),
        bpm,
        margin,
    }
}

/// The line on one day against the line on another.
fn between(now: &Day, then: &Day) -> Change {
    Change::of(now.on, (now.bpm, now.margin), (then.bpm, then.margin))
}

/// The change from a line at 50 ± 0.8 on 4 August to `bpm` ± 0.6 on 2
/// November: the band of the change is ± 1.0.
fn read(bpm: f64) -> Option<Direction> {
    between(
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
    let change = between(
        &on_the_line(2, 11, 40.0, None),
        &on_the_line(4, 8, 50.0, Some(0.8)),
    );
    assert!((change.by + 10.0).abs() < 1e-9);
    assert_eq!(change.margin, None);
    assert_eq!(change.direction, None);
}
