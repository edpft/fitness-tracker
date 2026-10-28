//! The FTP tests, in watts and over body weight (#469).
//!
//! **Tests alone** (operator, 2026-10-28). A ride's best twenty minutes is what
//! its class asked for, since a Power Zone class sets its targets from the FTP,
//! so only a test is maximal. And only its twenty minutes: the effort is paced
//! for twenty, so its best five or ten is not maximal either. One figure per
//! test, the FTP it gave.
//!
//! **Any movement is a movement** (operator, 2026-10-28). A test is a
//! measurement compared with a measurement, with no trend to find and no test
//! to estimate, so it is rising or falling against the test before it by any
//! difference at all, as 2.5 kg on a tested 1RM is an increase whatever the
//! 1RM.
//!
//! **Per kilo, against the test immediately before, where both were weighed**
//! (operator, 2026-10-28). Where the one before was not, there is no per-kilo
//! direction, rather than one read across tests that had none. The weigh-in is
//! the one [`super::weigh_in_for`]'s rule finds, and per kilo is compared at
//! the hundredths it is shown in.
//!
//! **Read against the whole record**, so a range only chooses which tests are
//! shown and the first in it still has the one before.

use std::fmt;

use jiff::civil::Date;

use super::{Weighed, body_weight::Direction, nearest, range::Range};
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

/// How far a figure moved from the test before, and which way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moved {
    /// In the figure's own unit: watts, or hundredths of a watt a kilo.
    pub by: i64,
    pub direction: Direction,
}

impl Moved {
    fn between(before: u32, now: u32) -> Self {
        let by = i64::from(now) - i64::from(before);
        let direction = match by.signum() {
            1 => Direction::Rising,
            -1 => Direction::Falling,
            _ => Direction::Maintaining,
        };
        Self { by, direction }
    }
}

/// One test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Test {
    pub ftp: Ftp,
    /// The weigh-in it is read against, where there was one that day.
    pub body: Option<Kg>,
    pub per_kilo: Option<PowerToWeight>,
    /// Against the test before; `None` for the first.
    pub watts_moved: Option<Moved>,
    /// Against the test before, where both were weighed.
    pub per_kilo_moved: Option<Moved>,
}

impl Test {
    pub const fn on(&self) -> Date {
        self.ftp.from()
    }
}

/// Every test, oldest first, each read against the one before.
///
/// `tested` pairs each FTP with when its test started, which is what finds the
/// weigh-in. An FTP no test produced is not a test, and is left out.
pub fn tests(tested: &[(Ftp, StartedAt)], weigh_ins: &[Weighed]) -> Vec<Test> {
    let mut tested: Vec<&(Ftp, StartedAt)> = tested
        .iter()
        .filter(|(ftp, _)| ftp.provenance() != FtpProvenance::Asserted)
        .collect();
    tested.sort_by_key(|(ftp, _)| ftp.from());
    let mut tests: Vec<Test> = Vec::with_capacity(tested.len());
    for (ftp, started) in tested {
        let body = nearest(&Occurred::At(started.clone()), weigh_ins).map(|weighed| weighed.mass);
        let per_kilo = body.and_then(|body| PowerToWeight::of(ftp.watts(), body));
        let before = tests.last();
        tests.push(Test {
            ftp: *ftp,
            body,
            per_kilo,
            watts_moved: before
                .map(|before| Moved::between(before.ftp.watts().as_u32(), ftp.watts().as_u32())),
            per_kilo_moved: before
                .and_then(|before| before.per_kilo)
                .zip(per_kilo)
                .map(|(before, now)| Moved::between(before.as_hundredths(), now.as_hundredths())),
        });
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
