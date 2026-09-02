//! What a cycling session instructs.
//!
//! **A session is duration × power zone, and nothing else** (decision 0025).
//! The operator settled it on 2026-09-01: *"otherwise we're importing Peloton
//! into our domain model."* A Peloton class is a thing at a destination that
//! *realises* one of these; its identifier is a reference at the adapter, in
//! exactly the position decision 0022 gives Hevy's.
//!
//! **The ride may prescribe no zone at all, and that is the test.** An FTP test
//! is twenty minutes with no zone attached, because a zone is a share of FTP and
//! this ride is what measures FTP — prescribing it in zones would be circular.
//!
//! That is the same shape decision 0023 reached for the gym: a `WorkUp` carrying
//! a repetition count and no load, because the load is discovered rather than
//! derived. **That code is parked and not on `main`**, so these are kin by
//! reasoning rather than by a shared type:
//!
//! ```text
//! gym       WorkUp   reps,     no load   →  discovers the load
//! cycling   Effort   duration, no zone   →  discovers the output
//! ```
//!
//! Both sit at the end of their programme, and both measure the number every
//! other prescription in that programme is a share of. Neither was designed for
//! the other.
//!
//! **Cadence is deliberately absent.** Settled by the operator. The only cadence
//! anywhere in the transcribed programme is the warm-up spin-ups, and no working
//! interval prescribes an RPM — so an axis carried by every interval to serve
//! one section would be a type built for a single case.

use std::fmt;

use crate::gym::{PositiveDuration, sequence::NonEmpty};

use super::zone::{Ftp, PowerZone};

/// One stretch at one zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    zone: PowerZone,
    duration: PositiveDuration,
}

impl Interval {
    pub const fn new(zone: PowerZone, duration: PositiveDuration) -> Self {
        Self { zone, duration }
    }

    pub const fn zone(self) -> PowerZone {
        self.zone
    }

    pub const fn duration(self) -> PositiveDuration {
        self.duration
    }
}

impl fmt::Display for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.zone, clock(self.duration))
    }
}

/// The working part of a session.
///
/// **Two cases and no third.** Either the programme says which zones and for how
/// long, or it says how long and leaves the intensity to be discovered. There is
/// no case that prescribes neither, because a ride that instructs nothing is not
/// a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ride {
    /// Zones, in order, each for a stated time.
    Intervals(NonEmpty<Interval>),
    /// A duration and no zone: ride as hard as can be held for this long.
    ///
    /// The FTP test, and so far only the FTP test.
    Effort(PositiveDuration),
}

impl Ride {
    /// How long the working part runs.
    #[must_use]
    pub fn duration(&self) -> PositiveDuration {
        match self {
            Self::Intervals(intervals) => {
                let seconds = intervals
                    .iter()
                    .map(|interval| interval.duration().as_seconds())
                    .sum();
                // A non-empty list of positive durations sums to a positive
                // one, so the fallback is unreachable — but § 26 forbids
                // asserting that with a panic, so it is answered instead.
                PositiveDuration::from_seconds(seconds)
                    .unwrap_or_else(|_| intervals.first().duration())
            }
            Self::Effort(duration) => *duration,
        }
    }

    /// How long this ride spends in each zone, in seconds.
    ///
    /// Empty for an [`Effort`](Self::Effort) — not because the time is unknown
    /// but because it belongs to no zone until it has been ridden.
    #[must_use]
    pub fn time_in_zone(&self) -> Vec<(PowerZone, u64)> {
        let Self::Intervals(intervals) = self else {
            return Vec::new();
        };
        let mut totals: Vec<(PowerZone, u64)> = Vec::new();
        for interval in intervals.iter() {
            match totals.iter_mut().find(|(zone, _)| *zone == interval.zone()) {
                Some((_, seconds)) => *seconds += interval.duration().as_seconds(),
                None => totals.push((interval.zone(), interval.duration().as_seconds())),
            }
        }
        totals.sort_by_key(|(zone, _)| *zone);
        totals
    }

    /// The hardest zone this ride reaches, if it names any.
    #[must_use]
    pub fn peak_zone(&self) -> Option<PowerZone> {
        match self {
            Self::Intervals(intervals) => intervals.iter().map(|interval| interval.zone()).max(),
            Self::Effort(_) => None,
        }
    }
}

/// A whole cycling session: warm up, ride, cool down.
///
/// **The warm-up and the cool-down are durations rather than interval
/// sequences**, and that is a finding rather than a simplification. The operator
/// described the warm-up every class in the programme uses — *"a couple of
/// minutes @ Z1, 2-3 x spin up 20-40 seconds @ 120RPM, a build touching each of
/// the zones the ride touches, a final minute or so in Z1"* — and read as
/// `Z1 + spin-ups + a build to the ride's peak zone + Z1` it predicts the app's
/// own movement counts to within one across all twenty-three classes that state
/// one. **The warm-up is a function of the ride**, so carrying it as a duration
/// loses nothing that could not be re-derived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CyclingSession {
    warm_up: PositiveDuration,
    ride: Ride,
    /// **Absent, not zero, for the FTP test**, which the app ships with no
    /// cool-down section at all. A zero-length cool-down and no cool-down are
    /// different claims and only one of them is true.
    cool_down: Option<PositiveDuration>,
}

impl CyclingSession {
    pub const fn new(
        warm_up: PositiveDuration,
        ride: Ride,
        cool_down: Option<PositiveDuration>,
    ) -> Self {
        Self {
            warm_up,
            ride,
            cool_down,
        }
    }

    pub const fn warm_up(&self) -> PositiveDuration {
        self.warm_up
    }

    pub const fn ride(&self) -> &Ride {
        &self.ride
    }

    pub const fn cool_down(&self) -> Option<PositiveDuration> {
        self.cool_down
    }

    /// Warm-up, ride and cool-down together — what the session costs in time.
    #[must_use]
    pub fn total(&self) -> PositiveDuration {
        let seconds = self.warm_up.as_seconds()
            + self.ride.duration().as_seconds()
            + self.cool_down.map_or(0, PositiveDuration::as_seconds);
        PositiveDuration::from_seconds(seconds).unwrap_or(self.warm_up)
    }

    /// The same session with more cool-down on the end.
    ///
    /// The operator rides five minutes of his own after the minute Peloton
    /// builds in. That is a generation parameter (§ 14) applied when a session
    /// is prescribed, kept out of the transcribed programme so the record of
    /// what the class actually contains stays faithful.
    #[must_use]
    pub fn with_extra_cool_down(&self, extra: PositiveDuration) -> Self {
        let seconds = self.cool_down.map_or(0, PositiveDuration::as_seconds) + extra.as_seconds();
        Self {
            warm_up: self.warm_up,
            ride: self.ride.clone(),
            cool_down: Some(PositiveDuration::from_seconds(seconds).unwrap_or(extra)),
        }
    }

    /// Each interval, with the watts it means for a rider at this FTP.
    ///
    /// Empty for an effort, which names no zone to convert.
    #[must_use]
    pub fn in_watts(&self, ftp: Ftp) -> Vec<(Interval, super::zone::WattRange)> {
        let Ride::Intervals(intervals) = &self.ride else {
            return Vec::new();
        };
        intervals
            .iter()
            .map(|interval| (*interval, interval.zone().band().watts_at(ftp)))
            .collect()
    }
}

/// `m:ss`, the way the app writes an interval.
#[must_use]
pub fn clock(duration: PositiveDuration) -> String {
    let seconds = duration.as_seconds();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
