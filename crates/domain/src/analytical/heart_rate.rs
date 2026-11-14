//! A main ride's heart rate, and the device that sent it (#152, #621).
//!
//! **One series per device.** A wrist sensor and a chest strap do not read
//! alike, so a ride carries the device that sent its heart rate, and a ride
//! whose device cannot be told is [`SentBy::Unknown`] rather than guessed
//! (§ 6). Nothing here joins two devices' points.
//!
//! **The ride's own monitor where Peloton's account names one**, and otherwise
//! the one device the register shows in use that day.

use super::heart_rate_at_power::{AtPower, seconds};
use crate::{
    cycling::{CanonicalBikePlusRide, CanonicalCyclingSession, Ftp},
    device::{Device, InUse, Measurand, Register},
    normalised::StartedAt,
};

/// The device that sent a ride's heart rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SentBy {
    Device(Device),
    /// None was in use, or more than one was.
    Unknown,
}

/// One main ride's heart rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RideHeartRate {
    pub started_at: StartedAt,
    pub sent_by: SentBy,
    /// What the ride gives heart rate at a fixed power (#621).
    pub at_power: AtPower,
}

impl RideHeartRate {
    /// The operator's day the ride started on.
    #[must_use]
    pub fn on(&self) -> jiff::civil::Date {
        self.started_at.wall_clock().date()
    }
}

/// A session's main ride's heart rate, or [`None`] for a test.
///
/// **Main rides only.** A test's effort is one zone held as hard as can be,
/// and a cool-down is not a ride anything is compared across.
///
/// `ftp` is the value in force on the ride's date (§ 13).
#[must_use]
pub fn ride_heart_rate(
    session: &CanonicalCyclingSession,
    ftp: Option<Ftp>,
    register: &Register,
) -> Option<RideHeartRate> {
    let CanonicalCyclingSession::Ride { main, .. } = session else {
        return None;
    };
    Some(RideHeartRate {
        started_at: main.started_at().value().clone(),
        sent_by: sent_by(main, register),
        at_power: seconds(main, ftp),
    })
}

/// The device that sent `ride`'s heart rate: the ride's own monitor where
/// Peloton's account names one, and otherwise the one device the register
/// shows in use that day.
pub fn sent_by(ride: &CanonicalBikePlusRide, register: &Register) -> SentBy {
    ride.heart_rate_device().map_or_else(
        || {
            let day = ride.started_at().value().wall_clock().date();
            match register.sent_by(Measurand::HeartRate, day) {
                InUse::One(device) => SentBy::Device(device.clone()),
                InUse::Several(_) | InUse::Unknown => SentBy::Unknown,
            }
        },
        |stated| {
            register
                .stated(stated.value())
                .map_or(SentBy::Unknown, SentBy::Device)
        },
    )
}
