//! A test read against the noise of two tests (#688).
//!
//! **Against the test before, then against the start of the run.** A test is
//! rising or falling where it differs from the test before by more than the
//! noise. Where it does not, it is read against the first test of the
//! current run, the first since the last clear change, so steps too small to
//! read one at a time are caught once they add up, as a CUSUM catches drift
//! without its tuning. A clear change starts a new run. Otherwise there is no
//! clear change.
//!
//! **The order of a run adds nothing to it** (operator, 2026-11-03). Given
//! that the latest test is beyond the noise of the first, chance puts a test
//! between them 81% of the time and a real steady gain 85 to 91%, so a run
//! that rose at every step is read no differently from one that did not.
//!
//! The noise is each test's own: its metric's test–retest variation.

use jiff::civil::Date;

use super::body_weight::Direction;

/// How far a figure moved from the test before, and what it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moved {
    /// Against the test before, in the figure's own unit: watts, grams, or
    /// hundredths of a watt a kilo.
    pub by: i64,
    /// Rising or falling beyond the noise; `None` is no clear change.
    pub direction: Option<Direction>,
    /// Where the direction is read against the start of the run rather than
    /// the test before.
    pub since: Option<Since>,
}

/// The start of a run, and how far the figure has moved from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Since {
    pub on: Date,
    pub by: i64,
}

/// `now` against `earlier`: which way, where it is beyond `noise`.
fn beyond(earlier: u32, now: u32, noise: impl Fn(u32) -> f64) -> Option<Direction> {
    let by = f64::from(now) - f64::from(earlier);
    let margin = noise(earlier);
    if by > margin {
        Some(Direction::Rising)
    } else if by < -margin {
        Some(Direction::Falling)
    } else {
        None
    }
}

/// Each figure, oldest first, read against the one before and the start of
/// its run.
///
/// `noise` says how far either side of an earlier figure two tests land by
/// chance. A missing figure has no reading, nor does the one after it, and the
/// run starts again after it.
pub fn read(
    figures: &[(Date, Option<u32>)],
    noise: impl Fn(u32) -> f64 + Copy,
) -> Vec<Option<Moved>> {
    let mut start: Option<(Date, u32)> = None;
    let mut before: Option<(Date, u32)> = None;
    let mut read = Vec::with_capacity(figures.len());
    for &(on, figure) in figures {
        let moved = before.zip(figure).map(|((then, earlier), now)| {
            let by = i64::from(now) - i64::from(earlier);
            match (beyond(earlier, now, noise), start) {
                (Some(direction), _) => Moved {
                    by,
                    direction: Some(direction),
                    since: None,
                },
                (None, Some((from, first))) if from != then => {
                    let direction = beyond(first, now, noise);
                    Moved {
                        by,
                        direction,
                        since: direction.map(|_| Since {
                            on: from,
                            by: i64::from(now) - i64::from(first),
                        }),
                    }
                }
                (None, _) => Moved {
                    by,
                    direction: None,
                    since: None,
                },
            }
        });
        if let Some(now) = figure {
            if start.is_none() || moved.is_some_and(|moved| moved.direction.is_some()) {
                start = Some((on, now));
            }
        } else {
            start = None;
        }
        before = figure.map(|now| (on, now));
        read.push(moved);
    }
    read
}
