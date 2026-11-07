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
//! **Against the test before, then against the start of the run**, as
//! [`super::retest`] reads any test. Higher is better.
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

pub use super::retest::{Moved, Since};
use super::{
    Weighed,
    heart_rate::{SentBy, sent_by},
    nearest,
    range::Range,
    retest,
};

/// How far a self-paced 20-minute test repeats, as a fraction: its test–retest
/// coefficient of variation (Borszcz et al. 2020).
pub const TEST_RETEST: f64 = 0.029;
use crate::{
    canonical::Occurred,
    cycling::{CanonicalCyclingSession, Ftp, FtpProvenance, Watts},
    device::Register,
    measure::{BeatsPerMinute, Kg},
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

/// How far either side of `earlier` two tests land by chance, at 95%.
pub fn noise(earlier: u32) -> f64 {
    1.96 * std::f64::consts::SQRT_2 * TEST_RETEST * f64::from(earlier)
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
    let watts = retest::read(
        &tests
            .iter()
            .map(|test| (test.on(), Some(test.ftp.watts().as_u32())))
            .collect::<Vec<_>>(),
        noise,
    );
    let per_kilo = retest::read(
        &tests
            .iter()
            .map(|test| (test.on(), test.per_kilo.map(PowerToWeight::as_hundredths)))
            .collect::<Vec<_>>(),
        noise,
    );
    for ((test, watts), per_kilo) in tests.iter_mut().zip(watts).zip(per_kilo) {
        test.watts_moved = watts;
        test.per_kilo_moved = per_kilo;
    }
    tests
}

/// The heart rate a test was ridden at, over its twenty minutes (#548).
///
/// **Whether a test was ridden as deep as the ones before it** (operator,
/// 2026-11-07): the maximum, the mean, and the mean of the last 5 minutes.
/// Not the minimum, which is the heart rate coming out of the warm-up. Each
/// test keeps the device that sent its heart rate, because a wrist sensor and
/// a chest strap do not read alike (§ 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestHeartRate {
    pub on: Date,
    pub max: BeatsPerMinute,
    pub mean: BeatsPerMinute,
    pub last_five_minutes: BeatsPerMinute,
    pub sent_by: SentBy,
}

/// How long the closing stretch is.
pub const LAST_SECONDS: u64 = 300;

/// The heart rate of a test session's effort, the twenty minutes after its
/// warm-up. `None` for a ride that is not a test, or an effort with no heart
/// rate.
pub fn heart_rate(session: &CanonicalCyclingSession, register: &Register) -> Option<TestHeartRate> {
    let CanonicalCyclingSession::Test { effort, .. } = session else {
        return None;
    };
    let samples: Vec<(u64, u32)> = effort
        .heart_rate()?
        .value()
        .samples()
        .iter()
        .map(|sample| (sample.at.as_seconds(), sample.beats_per_minute.as_u32()))
        .collect();
    let (max, mean, last_five_minutes) = figures(&samples, effort.duration().value().as_seconds())?;
    Some(TestHeartRate {
        on: effort.started_at().value().wall_clock().date(),
        max,
        mean,
        last_five_minutes,
        sent_by: sent_by(effort, register),
    })
}

/// A test's maximum, mean and closing mean.
///
/// Over a series of `(second, beats per minute)` from a ride of `duration`
/// seconds; the closing mean is over its last [`LAST_SECONDS`]. Each mean is
/// over the samples, to the nearest beat. `None` where either stretch has no
/// sample.
pub fn figures(
    samples: &[(u64, u32)],
    duration: u64,
) -> Option<(BeatsPerMinute, BeatsPerMinute, BeatsPerMinute)> {
    let from = duration.saturating_sub(LAST_SECONDS);
    let last: Vec<u32> = samples
        .iter()
        .filter(|(at, _)| *at > from)
        .map(|(_, beats)| *beats)
        .collect();
    let all: Vec<u32> = samples.iter().map(|(_, beats)| *beats).collect();
    let max = BeatsPerMinute::new(*all.iter().max()?).ok()?;
    Some((max, mean(&all)?, mean(&last)?))
}

fn mean(beats: &[u32]) -> Option<BeatsPerMinute> {
    let count = u64::try_from(beats.len()).ok().filter(|count| *count > 0)?;
    let total: u64 = beats.iter().map(|beats| u64::from(*beats)).sum();
    let rounded = total.saturating_add(count / 2) / count;
    BeatsPerMinute::new(u32::try_from(rounded).ok()?).ok()
}

/// Only the tests in `range`.
pub fn within(tests: &[Test], range: Range) -> Vec<Test> {
    tests
        .iter()
        .filter(|test| range.covers(test.on()))
        .copied()
        .collect()
}
