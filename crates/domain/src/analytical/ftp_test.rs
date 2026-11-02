//! The FTP tests, in watts and over body weight (#469).
//!
//! **Tests alone** (operator, 2026-10-28). A ride's best twenty minutes is what
//! its class asked for, since a Power Zone class sets its targets from the FTP,
//! so only a test is maximal. And only its twenty minutes: the effort is paced
//! for twenty, so its best five or ten is not maximal either. One figure per
//! test, the FTP it gave.
//!
//! **A movement is one beyond the noise of two tests** (operator,
//! 2026-11-02, #688), which withdrew 2026-10-28's "any movement is a
//! movement". A test is a real performance, but the same rider tested twice
//! with no training between gives two figures: a self-paced 20-minute test
//! repeats to a CV of 2.9% (Borszcz et al. 2020, 25 competitive cyclists, from
//! a search summary). Two tests then differ by chance within 1.96 × √2 × 2.9%,
//! about 8% of the earlier one: ± 14.6 W at 182 W. His own noise is not
//! measured: an open retest would not be blind, and a first blind one would
//! be unfamiliar (operator, 2026-11-02).
//!
//! **Against the test before, then against the start of the run.** A test is
//! rising or falling where it differs from the test before by more than the
//! noise. Where it does not, it is read against the first test of the
//! current run, the first since the last clear change, so steps too small to
//! read one at a time are caught once they add up, as a CUSUM catches drift
//! without its tuning. A clear change starts a new run. Otherwise there is no
//! clear change. Higher is better.
//!
//! **Per kilo, against the test immediately before, where both were weighed**
//! (operator, 2026-10-28). Where the one before was not, there is no per-kilo
//! direction, rather than one read across tests that had none, and its run
//! starts again at the next weighed test. Per kilo carries the same noise as
//! a fraction: a weigh-in's ± 0.35 kg adds about ± 0.01 W/kg. The weigh-in is
//! the one [`super::weigh_in_for`]'s rule finds, and per kilo is compared at
//! the hundredths it is shown in.
//!
//! **Read against the whole record**, so a range only chooses which tests are
//! shown and the first in it still has the one before.

use std::fmt;

use jiff::civil::Date;

use super::{Weighed, body_weight::Direction, nearest, range::Range};

/// How far a self-paced 20-minute test repeats, as a fraction: its test–retest
/// coefficient of variation (Borszcz et al. 2020).
pub const TEST_RETEST: f64 = 0.029;
use crate::{
    canonical::Occurred,
    cycling::{Ftp, FtpProvenance, Watts},
    measure::Kg,
    normalised::StartedAt,
};

/// Power over body weight, in hundredths of a watt a kilo, rounded down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PowerToWeight(u32);

impl PowerToWeight {
    /// `None` for a zero body mass, which no scale serves.
    pub fn of(power: Watts, body: Kg) -> Option<Self> {
        u64::from(power.as_u32())
            .checked_mul(100_000)?
            .checked_div(body.as_grams())
            .and_then(|hundredths| u32::try_from(hundredths).ok())
            .map(Self)
    }

    pub const fn as_hundredths(self) -> u32 {
        self.0
    }
}

impl fmt::Display for PowerToWeight {
    /// Two decimal places: `2.16W/kg`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:02}W/kg", self.0 / 100, self.0 % 100)
    }
}

/// How far a figure moved from the test before, and what it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moved {
    /// Against the test before, in the figure's own unit: watts, or
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

/// How far either side of `earlier` two tests land by chance, at 95%.
pub fn noise(earlier: u32) -> f64 {
    1.96 * std::f64::consts::SQRT_2 * TEST_RETEST * f64::from(earlier)
}

/// `now` against `earlier`: which way, where it is beyond the noise.
fn beyond(earlier: u32, now: u32) -> Option<Direction> {
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
/// its run. A missing figure has no reading, nor does the one after it, and
/// the run starts again after it.
fn read(figures: &[(Date, Option<u32>)]) -> Vec<Option<Moved>> {
    let mut start: Option<(Date, u32)> = None;
    let mut before: Option<(Date, u32)> = None;
    let mut read = Vec::with_capacity(figures.len());
    for &(on, figure) in figures {
        let moved = before.zip(figure).map(|((then, earlier), now)| {
            let by = i64::from(now) - i64::from(earlier);
            match (beyond(earlier, now), start) {
                (Some(direction), _) => Moved {
                    by,
                    direction: Some(direction),
                    since: None,
                },
                (None, Some((from, first))) if from != then => {
                    let direction = beyond(first, now);
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

/// One test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Test {
    pub ftp: Ftp,
    /// The weigh-in it is read against, where there was one that day.
    pub body: Option<Kg>,
    pub per_kilo: Option<PowerToWeight>,
    /// Against the test before and its run; `None` for the first.
    pub watts_moved: Option<Moved>,
    /// Against the test before and its run, where both were weighed.
    pub per_kilo_moved: Option<Moved>,
}

impl Test {
    pub const fn on(&self) -> Date {
        self.ftp.from()
    }
}

/// Every test, oldest first, each read against the one before and the start
/// of its run.
///
/// `tested` pairs each FTP with when its test started, which is what finds the
/// weigh-in. An FTP no test produced is not a test, and is left out.
pub fn tests(tested: &[(Ftp, StartedAt)], weigh_ins: &[Weighed]) -> Vec<Test> {
    let mut tested: Vec<&(Ftp, StartedAt)> = tested
        .iter()
        .filter(|(ftp, _)| ftp.provenance() != FtpProvenance::Asserted)
        .collect();
    tested.sort_by_key(|(ftp, _)| ftp.from());
    let mut tests: Vec<Test> = tested
        .into_iter()
        .map(|(ftp, started)| {
            let body =
                nearest(&Occurred::At(started.clone()), weigh_ins).map(|weighed| weighed.mass);
            Test {
                ftp: *ftp,
                body,
                per_kilo: body.and_then(|body| PowerToWeight::of(ftp.watts(), body)),
                watts_moved: None,
                per_kilo_moved: None,
            }
        })
        .collect();
    let watts = read(
        &tests
            .iter()
            .map(|test| (test.on(), Some(test.ftp.watts().as_u32())))
            .collect::<Vec<_>>(),
    );
    let per_kilo = read(
        &tests
            .iter()
            .map(|test| (test.on(), test.per_kilo.map(PowerToWeight::as_hundredths)))
            .collect::<Vec<_>>(),
    );
    for ((test, watts), per_kilo) in tests.iter_mut().zip(watts).zip(per_kilo) {
        test.watts_moved = watts;
        test.per_kilo_moved = per_kilo;
    }
    tests
}

/// Only the tests in `range`.
pub fn within(tests: &[Test], range: Range) -> Vec<Test> {
    tests
        .iter()
        .filter(|test| range.covers(test.on()))
        .copied()
        .collect()
}
