//! A night's sleep as the series read it: how long, and when (#255).
//!
//! **The data points, not their trend.** The trend over them is #418's,
//! computed once the method is settled on what these show.
//!
//! **One device's nights**, as HRV's are (§ 6); the attribution is
//! [`super::per_device`]'s.

use jiff::civil::DateTime;

use crate::measure::PositiveDuration;

/// One night, as both series read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Night {
    /// Garmin's time asleep.
    pub asleep: PositiveDuration,
    /// The wall clock at falling asleep, in the zone in force then.
    pub fell_asleep: DateTime,
    /// The wall clock at waking.
    pub woke: DateTime,
}
