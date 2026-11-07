//! Heart rate at a fixed power (#621, #688).
//!
//! **As fitness improves, heart rate at the same power falls** (operator,
//! 2026-09-26), and every second of every ride is a heart rate at a known
//! power. A class holds power within a zone, so power is what is fixed, and
//! the reading is heart rate at one power for every ride (2026-11-06).
//!
//! **One model over every ride**, after de Leeuw et al. 2023 (*Data Mining and
//! Knowledge Discovery* 37:1125–1153). Heart rate follows the power of the
//! last [`KERNEL_SECONDS`], weighted by an exponential kernel, which carries
//! heart rate's lag rather than discarding the seconds it affects. A minute
//! into the ride is a second term, for drift. Each ride has its own level and
//! the two slopes are shared, so a ride's level read at [`REFERENCE_WATTS`]
//! and [`REFERENCE_MINUTES`] is its heart rate there.
//!
//! **His lag, not the paper's.** Over his Forerunner 165 rides a recuperation
//! time of 90 s fits best (residual 4.2 bpm, R² 0.91); the paper's riders
//! fitted 40 s over 120 s, which cuts his kernel short (#688).
//!
//! **140 W, fixed** (operator, 2026-11-07): inside nearly every ride's range,
//! and fixed rather than following FTP, so the reading crosses FTP tests. The
//! FTP only bounds which seconds are read, 45 to 105% of it: above threshold
//! the relation bends (de Leeuw et al.).
//!
//! **The line is the mean of the rides in the last 42 days** (2026-11-07), and
//! it is read against the 42 days before it, beyond the noise of the two. The
//! noise is the scatter between successive rides, so no grouping by FTP
//! period is needed: 5.5 bpm on his Forerunner 165.
//!
//! **One series per device** (§ 6), and none for rides whose device is not
//! known. A ride whose heart rate never changed is a stuck sensor, not a
//! reading: 2025-02-20 read 83 bpm every second for 33 minutes.

use std::collections::BTreeMap;

use jiff::{ToSpan as _, civil::Date};

use super::{
    heart_rate::{RideHeartRate, SentBy},
    range::Range,
    resting::{Change, Day},
};
use crate::{
    cycling::{CanonicalBikePlusRide, Ftp},
    device::Device,
};

/// What each ride's figure is.
pub const METHOD: &str = "heart rate at 140 W, 20 minutes in, from a model of every ride's heart \
                          rate against its power over the last 5 minutes";

/// What the line is.
pub const LINE: &str = "mean of the rides in the 42 days ending on the ride, at least 3 of them";

/// The power each ride's heart rate is read at.
pub const REFERENCE_WATTS: f64 = 140.0;

/// The minute into the ride each ride's heart rate is read at.
pub const REFERENCE_MINUTES: f64 = 20.0;

/// How far back the kernel weights power.
const KERNEL_SECONDS: u16 = 300;

/// The kernel's recuperation time.
const RECUPERATION_SECONDS: f64 = 90.0;

/// One second in every this many is read.
const STEP_SECONDS: usize = 5;

/// The share of FTP below and above which a second is not read.
const LOWEST_SHARE: f64 = 0.45;
const HIGHEST_SHARE: f64 = 1.05;

/// How many days the line pools.
const LINE_DAYS: i32 = 42;

/// How few rides make a line.
const LINE_RIDES: usize = 3;

/// How few pairs of successive rides measure the noise.
const NOISE_PAIRS: usize = 5;

/// 95% either side.
const Z: f64 = 1.96;

/// One second read: power over the kernel, in tenths of a watt, when, and the
/// heart rate. Whole numbers, so a ride compares exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Second {
    pub deciwatts: u32,
    pub at: u32,
    pub beats: u32,
}

/// What a ride gives the model, or why it gives nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AtPower {
    Seconds(Vec<Second>),
    /// Heart rate was the same on every second read: a stuck sensor.
    Constant,
    /// No heart rate, no FTP, or no second within the range of power read.
    Nothing,
}

/// The seconds a ride gives the model.
#[must_use]
pub fn seconds(ride: &CanonicalBikePlusRide, ftp: Option<Ftp>) -> AtPower {
    let (Some(series), Some(ftp)) = (ride.heart_rate(), ftp) else {
        return AtPower::Nothing;
    };
    let beats: BTreeMap<u64, u32> = series
        .value()
        .samples()
        .iter()
        .map(|sample| (sample.at.as_seconds(), sample.beats_per_minute.as_u32()))
        .collect();
    let power = per_second(ride);
    let kernel = kernel();
    let ftp = f64::from(ftp.watts().as_u32());
    let (lowest, highest) = (LOWEST_SHARE * ftp, HIGHEST_SHARE * ftp);
    let mut read = Vec::new();
    let back = usize::from(KERNEL_SECONDS);
    for at in (back..power.len()).step_by(STEP_SECONDS) {
        let Some(before) = power.get(at.saturating_sub(back)..=at) else {
            continue;
        };
        let convolved: f64 = before
            .iter()
            .rev()
            .zip(&kernel)
            .map(|(watts, weight)| watts * weight)
            .sum();
        let (Ok(second), Some(&rate)) = (
            u32::try_from(at),
            u64::try_from(at).ok().and_then(|at| beats.get(&at)),
        ) else {
            continue;
        };
        if (lowest..=highest).contains(&convolved) {
            read.push(Second {
                deciwatts: tenths(convolved),
                at: second,
                beats: rate,
            });
        }
    }
    let Some(first) = read.first() else {
        return AtPower::Nothing;
    };
    if read.iter().all(|second| second.beats == first.beats) {
        AtPower::Constant
    } else {
        AtPower::Seconds(read)
    }
}

/// Power at every second from the ride's first, a second Peloton skipped
/// carrying the one before it.
fn per_second(ride: &CanonicalBikePlusRide) -> Vec<f64> {
    let mut power = Vec::new();
    let mut last = 0.0;
    for sample in ride.samples().value().iter() {
        let at = usize::try_from(sample.at.as_seconds()).unwrap_or(usize::MAX);
        while power.len() < at {
            power.push(last);
        }
        last = f64::from(sample.power.as_u32());
        if power.len() == at {
            power.push(last);
        }
    }
    power
}

/// The kernel's weights, the latest second first, summing to one.
fn kernel() -> Vec<f64> {
    let raw: Vec<f64> = (0..=KERNEL_SECONDS)
        .map(|back| (-f64::from(back) / RECUPERATION_SECONDS).exp())
        .collect();
    let total: f64 = raw.iter().sum();
    raw.into_iter().map(|weight| weight / total).collect()
}

/// Watts to the nearest tenth, as a whole number of tenths.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "read only between 45 and 105% of an FTP of a few hundred watts, so non-negative and far inside u32"
)]
fn tenths(watts: f64) -> u32 {
    (watts * 10.0).round().max(0.0) as u32
}

/// One ride's heart rate at [`REFERENCE_WATTS`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ride {
    pub on: Date,
    pub bpm: f64,
}

/// One device's rides, the line through them, and the slopes they share.
#[derive(Debug, Clone, PartialEq)]
pub struct Trended {
    pub device: Device,
    pub rides: Vec<Ride>,
    /// The rides whose heart rate never changed.
    pub constant: Vec<Date>,
    /// Runs of the line, broken where fewer than [`LINE_RIDES`] rides fall in
    /// its window.
    pub line: Vec<Vec<Day>>,
    /// The scatter of one ride, in bpm; `None` where too few rides follow
    /// one another to measure it.
    pub noise: Option<f64>,
    /// Heart rate per watt, and per minute into a ride.
    pub per_watt: f64,
    pub per_minute: f64,
}

impl Trended {
    /// The rides in the [`LINE_DAYS`] ending on the latest ride against the
    /// [`LINE_DAYS`] before them, or `None` where either holds too few.
    #[must_use]
    pub fn change(&self) -> Option<Change> {
        let last = self.rides.last()?.on;
        let now = self.window(last)?;
        let then = self.window(last.checked_sub(LINE_DAYS.days()).ok()?)?;
        Some(Change::of(
            last,
            (now.bpm, now.margin),
            (then.bpm, then.margin),
        ))
    }

    /// The mean of the rides in the [`LINE_DAYS`] ending on `on`, and its
    /// margin, where at least [`LINE_RIDES`] fall in it.
    #[must_use]
    pub fn window(&self, on: Date) -> Option<Day> {
        let from = on.checked_sub(LINE_DAYS.days()).ok()?;
        let within: Vec<f64> = self
            .rides
            .iter()
            .filter(|ride| ride.on > from && ride.on <= on)
            .map(|ride| ride.bpm)
            .collect();
        if within.len() < LINE_RIDES {
            return None;
        }
        let count = f64::from(u32::try_from(within.len()).ok()?);
        Some(Day {
            on,
            bpm: within.iter().sum::<f64>() / count,
            margin: self.noise.map(|noise| Z * noise / count.sqrt()),
        })
    }

    /// The rides, line and constant rides inside `range`; the noise and
    /// slopes are the whole record's.
    #[must_use]
    pub fn within(&self, range: Range) -> Self {
        let inside = |on: Date| range.from <= on && on <= range.to;
        Self {
            device: self.device.clone(),
            rides: self
                .rides
                .iter()
                .copied()
                .filter(|ride| inside(ride.on))
                .collect(),
            constant: self
                .constant
                .iter()
                .copied()
                .filter(|on| inside(*on))
                .collect(),
            line: self
                .line
                .iter()
                .map(|run| {
                    run.iter()
                        .copied()
                        .filter(|day| inside(day.on))
                        .collect::<Vec<_>>()
                })
                .filter(|run| !run.is_empty())
                .collect(),
            noise: self.noise,
            per_watt: self.per_watt,
            per_minute: self.per_minute,
        }
    }
}

/// Each device's heart rate at [`REFERENCE_WATTS`], in the order its first
/// ride came; rides whose device is not known have none.
#[must_use]
pub fn trended(rides: &[RideHeartRate]) -> Vec<Trended> {
    let mut devices: Vec<(Device, Vec<&RideHeartRate>)> = Vec::new();
    for ride in rides {
        let SentBy::Device(device) = &ride.sent_by else {
            continue;
        };
        if let Some((_, its)) = devices.iter_mut().find(|(known, _)| known == device) {
            its.push(ride);
        } else {
            devices.push((device.clone(), vec![ride]));
        }
    }
    devices
        .into_iter()
        .filter_map(|(device, rides)| of(device, &rides))
        .collect()
}

/// One device's model, its rides read at the reference, and its line.
fn of(device: Device, rides: &[&RideHeartRate]) -> Option<Trended> {
    let constant: Vec<Date> = rides
        .iter()
        .filter(|ride| ride.at_power == AtPower::Constant)
        .map(|ride| ride.on())
        .collect();
    let read: Vec<(Date, Means)> = rides
        .iter()
        .filter_map(|ride| match &ride.at_power {
            AtPower::Seconds(seconds) => Means::of(seconds).map(|means| (ride.on(), means)),
            AtPower::Constant | AtPower::Nothing => None,
        })
        .collect();
    let (per_watt, per_minute) = slopes(read.iter().map(|(_, means)| means))?;
    let readings: Vec<Ride> = read
        .iter()
        .map(|(on, means)| Ride {
            on: *on,
            bpm: per_minute.mul_add(
                REFERENCE_MINUTES - means.minutes,
                per_watt.mul_add(REFERENCE_WATTS - means.watts, means.beats),
            ),
        })
        .collect();
    let mut trended = Trended {
        device,
        noise: noise(&readings),
        rides: readings,
        constant,
        line: Vec::new(),
        per_watt,
        per_minute,
    };
    trended.line = line(&trended);
    Some(trended)
}

/// The line on each ride's day, in runs.
fn line(trended: &Trended) -> Vec<Vec<Day>> {
    let mut runs: Vec<Vec<Day>> = Vec::new();
    let mut run: Vec<Day> = Vec::new();
    for ride in &trended.rides {
        if let Some(day) = trended.window(ride.on) {
            if run.last().is_none_or(|last| last.on != day.on) {
                run.push(day);
            }
        } else if !run.is_empty() {
            runs.push(std::mem::take(&mut run));
        }
    }
    if !run.is_empty() {
        runs.push(run);
    }
    runs
}

/// One ride's scatter, from successive rides fewer than [`LINE_DAYS`] apart:
/// half the mean squared difference, so a slow change in fitness does not
/// count as noise.
fn noise(rides: &[Ride]) -> Option<f64> {
    let squares: Vec<f64> = rides
        .windows(2)
        .filter_map(|pair| match pair {
            [before, after] if (after.on - before.on).get_days() < LINE_DAYS => {
                Some((after.bpm - before.bpm).powi(2))
            }
            _ => None,
        })
        .collect();
    if squares.len() < NOISE_PAIRS {
        return None;
    }
    let pairs = f64::from(u32::try_from(squares.len()).ok()?);
    Some((squares.iter().sum::<f64>() / (2.0 * pairs)).sqrt())
}

/// A ride's means and the sums of squares and products about them.
#[derive(Debug, Clone, Copy)]
struct Means {
    watts: f64,
    minutes: f64,
    beats: f64,
    ww: f64,
    mm: f64,
    wm: f64,
    wb: f64,
    mb: f64,
}

impl Means {
    fn of(seconds: &[Second]) -> Option<Self> {
        let count = f64::from(u32::try_from(seconds.len()).ok()?);
        if seconds.is_empty() {
            return None;
        }
        let point = |second: &Second| {
            (
                f64::from(second.deciwatts) / 10.0,
                f64::from(second.at) / 60.0,
                f64::from(second.beats),
            )
        };
        let (mut watts, mut minutes, mut beats) = (0.0, 0.0, 0.0);
        for second in seconds {
            let (w, m, b) = point(second);
            watts += w;
            minutes += m;
            beats += b;
        }
        let (watts, minutes, beats) = (watts / count, minutes / count, beats / count);
        let mut means = Self {
            watts,
            minutes,
            beats,
            ww: 0.0,
            mm: 0.0,
            wm: 0.0,
            wb: 0.0,
            mb: 0.0,
        };
        for second in seconds {
            let (w, m, b) = point(second);
            let (w, m, b) = (w - watts, m - minutes, b - beats);
            means.ww += w * w;
            means.mm += m * m;
            means.wm += w * m;
            means.wb += w * b;
            means.mb += m * b;
        }
        Some(means)
    }
}

/// The two slopes every ride shares, each ride about its own level.
fn slopes<'a>(rides: impl Iterator<Item = &'a Means>) -> Option<(f64, f64)> {
    let (mut ww, mut mm, mut wm, mut wb, mut mb) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for ride in rides {
        ww += ride.ww;
        mm += ride.mm;
        wm += ride.wm;
        wb += ride.wb;
        mb += ride.mb;
    }
    let determinant = ww.mul_add(mm, -(wm * wm));
    if determinant.abs() < f64::EPSILON {
        return None;
    }
    Some((
        wb.mul_add(mm, -(mb * wm)) / determinant,
        mb.mul_add(ww, -(wb * wm)) / determinant,
    ))
}
