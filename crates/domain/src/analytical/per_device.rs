//! A watch's own daily figures, one series per device (#254).
//!
//! **Overnight HRV and resting heart rate** are, in the operator's words,
//! *"good indicators of cardiovascular fitness"*. Both are method-dependent
//! (§ 6): the figure is the watch's and its algorithm's, so two watches' figures
//! are two series and nothing here joins them.
//!
//! **The watch Garmin's own records name for the night** (#569): its sleep
//! record names the device on every night since 2024-12-22. Before that nothing
//! does, and a day is placed on the one wrist device the register shows worn
//! that day ([`Register::worn_on`]), the rule for history alone. A day it
//! cannot place is [`SentBy::Unknown`] rather than guessed — a series of its
//! own, as a ride's is in [`super::heart_rate`].
//!
//! **A gap is a gap** (§ 37): a series holds the days that have a figure and
//! nothing for the days between.

use jiff::civil::Date;

use super::range::Range;
use crate::device::{DeviceIdentifier, InUse, Measurand, Register};

pub use super::heart_rate::SentBy;

/// One day's figure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading<T> {
    pub on: Date,
    pub value: T,
}

/// One day's figure, and the device its own source's records name for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded<T> {
    pub reading: Reading<T>,
    /// [`None`] where nothing names one.
    pub by: Option<DeviceIdentifier>,
}

/// Every figure one device took, in date order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSeries<T> {
    pub sent_by: SentBy,
    pub readings: Vec<Reading<T>>,
}

impl<T: Copy> DeviceSeries<T> {
    /// Only the readings on `range`'s days; a device with none keeps its
    /// place, so it keeps its colour.
    #[must_use]
    pub fn within(&self, range: Range) -> Self {
        Self {
            sent_by: self.sent_by.clone(),
            readings: self
                .readings
                .iter()
                .filter(|reading| range.covers(reading.on))
                .copied()
                .collect(),
        }
    }
}

/// The readings, one series per device, the device that took the earliest
/// first and the readings it cannot place last.
///
/// `what` is what the figure is a reading of: a watch that measured no heart
/// rate cannot have taken one.
#[must_use]
pub fn per_device<T>(
    readings: Vec<Recorded<T>>,
    what: Measurand,
    register: &Register,
) -> Vec<DeviceSeries<T>> {
    let mut readings = readings;
    readings.sort_by_key(|recorded| recorded.reading.on);
    let mut series: Vec<DeviceSeries<T>> = Vec::new();
    let mut unplaced: Vec<Reading<T>> = Vec::new();
    for Recorded { reading, by } in readings {
        let device = match by {
            Some(named) => register.recorded_by(&named, what, reading.on),
            None => match register.worn_on(what, reading.on) {
                InUse::One(device) => device.clone(),
                InUse::Several(_) | InUse::Unknown => {
                    unplaced.push(reading);
                    continue;
                }
            },
        };
        let held = series.iter_mut().find(|one| match &one.sent_by {
            SentBy::Device(held) => held.identifiers() == device.identifiers(),
            SentBy::Unknown => false,
        });
        match held {
            Some(one) => one.readings.push(reading),
            None => series.push(DeviceSeries {
                sent_by: SentBy::Device(device),
                readings: vec![reading],
            }),
        }
    }
    if !unplaced.is_empty() {
        series.push(DeviceSeries {
            sent_by: SentBy::Unknown,
            readings: unplaced,
        });
    }
    series
}
