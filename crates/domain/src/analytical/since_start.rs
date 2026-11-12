//! An estimate's change from the start of the macrocycle to today (#769).
//!
//! **A test is read from the entry test** ([`super::since_entry`]); an
//! estimate has no entry test, so it is read from the macrocycle's start
//! (operator, 2026-11-12: *"e1RM and BPM@140w should show change since
//! beginning of macrocycle"*).
//!
//! **A line's first day in the macrocycle against its latest**, each with its
//! own band, the two combined as [`super::resting::Change`] combines them. A
//! line that begins after the start, because too few figures fell in its
//! first days, is read from the day it begins.

use jiff::civil::Date;

/// A line's value on its first day in the macrocycle, and on its latest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SinceStart {
    pub from: f64,
    pub now: f64,
    /// How far either side of [`Self::by`] its 95% band runs; `None` where
    /// either end has no band.
    pub margin: Option<f64>,
}

impl SinceStart {
    /// The latest value less the first.
    pub fn by(&self) -> f64 {
        self.now - self.from
    }
}

/// `line`, oldest first as `(day, value, margin)`, read over the macrocycle
/// that `began` on a day. `None` where fewer than two of its days fall
/// between `began` and `today`.
pub fn since_start(
    line: impl IntoIterator<Item = (Date, f64, Option<f64>)>,
    began: Date,
    today: Date,
) -> Option<SinceStart> {
    let mut within = line
        .into_iter()
        .filter(|(on, _, _)| *on >= began && *on <= today);
    let (first_on, from, from_margin) = within.next()?;
    let (last_on, now, now_margin) = within.last()?;
    (last_on > first_on).then(|| SinceStart {
        from,
        now,
        margin: from_margin
            .zip(now_margin)
            .map(|(from, now)| from.hypot(now)),
    })
}

#[cfg(test)]
mod tests {
    use jiff::civil::{Date, date};

    use super::since_start;

    const BEGAN: Date = date(2026, 9, 14);
    const TODAY: Date = date(2026, 11, 12);

    /// **Heart rate at 140 W on autumn 2026's line**: its first ride in the
    /// macrocycle, 16 September, against its latest.
    #[test]
    fn the_first_day_in_the_macrocycle_is_read_against_the_latest() {
        let line = [
            (date(2026, 8, 30), 142.0, Some(4.0)),
            (date(2026, 9, 16), 139.0, Some(3.0)),
            (date(2026, 10, 7), 135.5, Some(3.0)),
            (date(2026, 11, 1), 137.6, Some(4.0)),
        ];
        let read = since_start(line, BEGAN, TODAY).expect("two days in it");
        assert!((read.from - 139.0).abs() < 1e-9);
        assert!((read.by() + 1.4).abs() < 1e-9, "{}", read.by());
        assert_eq!(read.margin, Some(5.0));
    }

    #[test]
    fn a_line_with_one_day_in_the_macrocycle_has_no_change() {
        let line = [
            (date(2026, 9, 1), 84.0, Some(0.4)),
            (date(2026, 9, 20), 84.5, Some(0.4)),
        ];
        assert_eq!(since_start(line, BEGAN, TODAY), None);
    }

    #[test]
    fn an_end_without_a_band_leaves_the_change_without_one() {
        let line = [
            (date(2026, 9, 20), 18.9, None),
            (date(2026, 11, 11), 18.4, Some(0.5)),
        ];
        let read = since_start(line, BEGAN, TODAY).expect("two days in it");
        assert_eq!(read.margin, None);
    }
}
