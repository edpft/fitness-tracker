//! Heart rate per power zone, for each main ride (#152).
//!
//! **As fitness improves, heart rate at the same power falls** (operator,
//! 2026-09-26), and a ride's average heart rate in each power zone is that
//! signal, one point per zone it held.
//!
//! **One series per device.** A wrist sensor and a chest strap do not read
//! alike, so a ride carries the device that sent its heart rate, and a ride
//! whose device cannot be told is [`SentBy::Unknown`] rather than guessed
//! (§ 6). Nothing here joins two devices' points.
//!
//! **The ride's own monitor where Peloton's account names one**, and otherwise
//! the one device the register shows in use that day.
//!
//! **The method is part of the series** (§ 6), and it is the operator's,
//! 2026-10-14: power is a trailing 30-second mean, and a second counts towards
//! a zone only once the ride has been in that zone for 120 seconds. Heart rate
//! lags power, so pairing every second would give an interval the heart rate of
//! the one before it.
//!
//! **Both are measured in time, not in samples.** Peloton skips single seconds
//! throughout a ride (167 times in the hour of 2024-01-19), so the window is
//! the samples in the last 30 seconds, and the time in a zone is from the
//! sample that entered it.
//!
//! **Beside it, the class's Z3 intervals** (operator, 2026-10-16 and
//! 2026-10-19, #415): the heart rate over every interval the class prescribes
//! at Z3 for at least 3 minutes with no harder interval before it in the ride,
//! read after the same 120 seconds. Mixing whole rides gives a Z3 interval
//! after Z6 a different heart rate from one after Z1; one repeated stretch at
//! one power is compared like with like, as the Lamberts Submaximal Cycle Test
//! does. It is read off the prescription, not the power ridden, so it is the
//! same stretch from ride to ride.

use std::collections::{BTreeMap, VecDeque};

use crate::{
    cycling::{
        CanonicalBikePlusRide, CanonicalCyclingSession, CyclingSession, Ftp, PowerZone, Ride,
        RideVenue, Watts,
    },
    device::{Device, InUse, Measurand, Register},
    measure::{BeatsPerMinute, Duration},
    normalised::StartedAt,
    sequence::NonEmpty,
};

/// What the series is called, so a different method is never mistaken for it.
pub const METHOD: &str = "30 s mean power, counted after 120 s in one zone";

/// What the Z3 intervals series is called.
pub const INTERVALS_METHOD: &str = "the class's Z3 intervals of at least 3 min with no harder \
                                    interval before them, counted after 120 s in each";

/// The shortest Z3 interval read.
const INTERVAL_SECONDS: u64 = 180;

/// How many seconds the power is averaged over.
const WINDOW_SECONDS: u64 = 30;

/// How long a ride must have been in a zone before its heart rate counts.
const STEADY_SECONDS: u64 = 120;

/// The device that sent a ride's heart rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SentBy {
    Device(Device),
    /// None was in use, or more than one was.
    Unknown,
}

/// A ride's average heart rate in one power zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneHeartRate {
    pub zone: PowerZone,
    pub average: BeatsPerMinute,
    /// How many seconds the average is over.
    pub over: Duration,
}

/// A ride's average heart rate over its class's Z3 intervals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntervalHeartRate {
    pub average: BeatsPerMinute,
    /// How many seconds the average is over.
    pub over: Duration,
    /// How many intervals it is read from.
    pub intervals: usize,
}

/// What a ride's heart rate says per zone, or why it says nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ByZone {
    /// Every zone the ride held, lowest first.
    Zones(NonEmpty<ZoneHeartRate>),
    /// Nothing recorded a heart rate.
    NoHeartRate,
    /// No FTP test precedes it, so it has no zones.
    NoFtp,
    /// It held no zone for long enough while heart rate was recorded.
    NoZoneHeld,
}

/// One main ride's heart rate per power zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RideHeartRate {
    pub started_at: StartedAt,
    pub venue: RideVenue,
    pub ftp: Option<Ftp>,
    pub sent_by: SentBy,
    pub by_zone: ByZone,
    /// [`None`] where the class prescribes no such interval, the catalogue
    /// has not read the class, or no heart rate was recorded in them.
    pub in_intervals: Option<IntervalHeartRate>,
}

impl RideHeartRate {
    /// The operator's day the ride started on.
    #[must_use]
    pub fn on(&self) -> jiff::civil::Date {
        self.started_at.wall_clock().date()
    }
}

/// A session's main ride's heart rate per power zone, or [`None`] for a test.
///
/// **Main rides only.** A test's effort is one zone held as hard as can be,
/// and a cool-down is not a ride anything is compared across.
///
/// `ftp` is the value in force on the ride's date (§ 13), and `prescribed`
/// what the ride's class prescribes, where the catalogue has read it.
#[must_use]
pub fn heart_rate_by_zone(
    session: &CanonicalCyclingSession,
    ftp: Option<Ftp>,
    prescribed: Option<&CyclingSession>,
    register: &Register,
) -> Option<RideHeartRate> {
    let CanonicalCyclingSession::Ride { main, .. } = session else {
        return None;
    };
    let started_at = main.started_at().value().clone();
    let sent_by = main.heart_rate_device().map_or_else(
        || match register.sent_by(Measurand::HeartRate, started_at.wall_clock().date()) {
            InUse::One(device) => SentBy::Device(device.clone()),
            InUse::Several(_) | InUse::Unknown => SentBy::Unknown,
        },
        |stated| {
            register
                .stated(stated.value())
                .map_or(SentBy::Unknown, SentBy::Device)
        },
    );
    Some(RideHeartRate {
        venue: main.at().value().clone(),
        by_zone: by_zone(main, ftp),
        in_intervals: prescribed.and_then(|prescribed| in_intervals(main, prescribed)),
        started_at,
        ftp,
        sent_by,
    })
}

fn by_zone(ride: &CanonicalBikePlusRide, ftp: Option<Ftp>) -> ByZone {
    let Some(series) = ride.heart_rate() else {
        return ByZone::NoHeartRate;
    };
    let Some(ftp) = ftp else {
        return ByZone::NoFtp;
    };
    let beats: BTreeMap<u64, u32> = series
        .value()
        .samples()
        .iter()
        .map(|sample| (sample.at.as_seconds(), sample.beats_per_minute.as_u32()))
        .collect();

    // Seconds counted and beats summed, per zone.
    let mut held: BTreeMap<PowerZone, (u64, u64)> = BTreeMap::new();
    let mut window: VecDeque<(u64, u64)> = VecDeque::new();
    let mut sum: u64 = 0;
    let mut entered: Option<(PowerZone, u64)> = None;
    for sample in ride.samples().value().iter() {
        let at = sample.at.as_seconds();
        window.push_back((at, u64::from(sample.power.as_u32())));
        sum = sum.saturating_add(u64::from(sample.power.as_u32()));
        while let Some(&(oldest, power)) = window.front()
            && oldest.saturating_add(WINDOW_SECONDS) <= at
        {
            window.pop_front();
            sum = sum.saturating_sub(power);
        }
        let zone = PowerZone::at(mean(sum, window.len()), ftp);

        let since = match entered {
            Some((current, since)) if current == zone => since,
            _ => {
                entered = Some((zone, at));
                at
            }
        };
        if at.saturating_sub(since) >= STEADY_SECONDS
            && let Some(&beats) = beats.get(&at)
        {
            let (seconds, total) = held.entry(zone).or_default();
            *seconds = seconds.saturating_add(1);
            *total = total.saturating_add(u64::from(beats));
        }
    }

    let zones: Vec<ZoneHeartRate> = held
        .into_iter()
        .filter_map(|(zone, (seconds, total))| {
            let average = u32::try_from(rounded(total, seconds)).ok()?;
            Some(ZoneHeartRate {
                zone,
                average: BeatsPerMinute::new(average).ok()?,
                over: Duration::from_seconds(seconds),
            })
        })
        .collect();
    NonEmpty::new(zones).map_or(ByZone::NoZoneHeld, ByZone::Zones)
}

/// The heart rate over the class's Z3 intervals.
///
/// **The ride's clock is the class's**: its first second is the warm-up's,
/// so an interval starts the warm-up and every interval before it into the
/// ride.
fn in_intervals(
    ride: &CanonicalBikePlusRide,
    prescribed: &CyclingSession,
) -> Option<IntervalHeartRate> {
    let Ride::Intervals(intervals) = prescribed.ride() else {
        return None;
    };
    let beats: BTreeMap<u64, u32> = ride
        .heart_rate()?
        .value()
        .samples()
        .iter()
        .map(|sample| (sample.at.as_seconds(), sample.beats_per_minute.as_u32()))
        .collect();
    let mut starts = prescribed.warm_up().as_seconds();
    let mut hardest = PowerZone::One;
    let (mut read, mut seconds, mut total) = (0_usize, 0_u64, 0_u64);
    for interval in intervals.iter() {
        let length = interval.duration().as_seconds();
        if interval.zone() == PowerZone::Three
            && length >= INTERVAL_SECONDS
            && hardest <= PowerZone::Three
        {
            read += 1;
            for (_, &beats) in
                beats.range(starts.saturating_add(STEADY_SECONDS)..starts.saturating_add(length))
            {
                seconds = seconds.saturating_add(1);
                total = total.saturating_add(u64::from(beats));
            }
        }
        hardest = hardest.max(interval.zone());
        starts = starts.saturating_add(length);
    }
    let average = BeatsPerMinute::new(u32::try_from(rounded(total, seconds)).ok()?).ok()?;
    (seconds > 0).then_some(IntervalHeartRate {
        average,
        over: Duration::from_seconds(seconds),
        intervals: read,
    })
}

/// The window's mean power, to the nearest watt.
fn mean(sum: u64, count: usize) -> Watts {
    let count = u64::try_from(count).unwrap_or(u64::MAX);
    Watts::from_u32(u32::try_from(rounded(sum, count)).unwrap_or(u32::MAX))
}

/// `total / count`, to the nearest whole number; nothing over nothing is zero.
fn rounded(total: u64, count: u64) -> u64 {
    total
        .saturating_add(count / 2)
        .checked_div(count)
        .unwrap_or(0)
}
