//! Garmin's night of sleep: how long, and when (#255).
//!
//! **Here beside [`super::hrv`], for its reason**: what the wrist makes of a
//! night is the watch's and its algorithm's (§ 6), so another watch's nights
//! are another series.
//!
//! **Duration and the window, and nothing else** (operator, 2026-10-16, #418).
//! Wrist staging agrees only moderately with polysomnography, and Garmin's
//! score is built partly on overnight stress, which duplicates HRV. The window
//! is the regularity series' data point: any index over windows is the trend,
//! which is #418's.
//!
//! **Read off the operator's 3,217 landed nights** (2018-01-06 to 2026-10-27):
//! 2,223 state a night's sleep and 994 state nothing at all, refused as
//! [`crate::normalised::RefusalReason::NoSleepRecorded`]. On every night that
//! states one, the time asleep is no longer than the window, and Garmin's
//! `calendarDate` is the local date of waking.
//!
//! **What stays in raw**: the stages, the score and its feedback, naps, the
//! overnight heart rate, respiration, `SpO2` and stress series, and Garmin's
//! automatic window beside the confirmed one.

use std::fmt;

use jiff::civil::Date;

use crate::device::DeviceIdentifier;
use crate::{
    landing::{LandingRecordId, Provenance, SourceRecordId},
    measure::PositiveDuration,
    normalised::{NormalisedEntity, StartedAt},
};

/// Why a night could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidSleep {
    #[error(
        "a night's sleep spans time, and this one ends at {woke}, having started at {fell_asleep}"
    )]
    Window { fell_asleep: String, woke: String },
    #[error("{asleep}s asleep does not fit in a window of {window}s")]
    LongerThanWindow { asleep: u64, window: u64 },
}

/// When the operator fell asleep and woke, by Garmin's account.
///
/// Two instants, as [`super::MeasurementWindow`] is, for its reason: a window
/// cannot be built out of naive times (§ II.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SleepWindow {
    fell_asleep: StartedAt,
    woke: StartedAt,
}

impl SleepWindow {
    /// # Errors
    ///
    /// [`InvalidSleep::Window`] if it ends before it starts, or spans no time.
    pub fn new(fell_asleep: StartedAt, woke: StartedAt) -> Result<Self, InvalidSleep> {
        if woke.instant() <= fell_asleep.instant() {
            return Err(InvalidSleep::Window {
                fell_asleep: fell_asleep.instant().to_string(),
                woke: woke.instant().to_string(),
            });
        }
        Ok(Self { fell_asleep, woke })
    }

    pub const fn fell_asleep(&self) -> &StartedAt {
        &self.fell_asleep
    }

    pub const fn woke(&self) -> &StartedAt {
        &self.woke
    }

    /// How long the window is, in whole seconds.
    fn seconds(&self) -> u64 {
        let span = self.woke.instant().as_second() - self.fell_asleep.instant().as_second();
        u64::try_from(span).unwrap_or(0)
    }
}

/// Everything one night's record says, on its way into [`Sleep`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SleepRecord {
    /// The morning Garmin reports the night under, as the watch's calendar
    /// has it.
    pub morning_of: Date,
    pub window: SleepWindow,
    /// Garmin's time asleep: the window less what it counted as awake.
    pub asleep: PositiveDuration,
    pub landed_as: LandingRecordId,
    pub source_record_id: SourceRecordId,
    pub provenance: Provenance,
    /// The watch the record names, where it names one (#569): every night
    /// since 2024-12-22.
    pub recorded_by: Option<DeviceIdentifier>,
}

/// One night's sleep, as Garmin told it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sleep {
    morning_of: Date,
    window: SleepWindow,
    asleep: PositiveDuration,
    landed_as: LandingRecordId,
    source_record_id: SourceRecordId,
    provenance: Provenance,
    recorded_by: Option<DeviceIdentifier>,
}

impl Sleep {
    /// # Errors
    ///
    /// [`InvalidSleep::LongerThanWindow`] if more time was asleep than the
    /// window holds.
    pub fn new(record: SleepRecord) -> Result<Self, InvalidSleep> {
        let window = record.window.seconds();
        if record.asleep.as_seconds() > window {
            return Err(InvalidSleep::LongerThanWindow {
                asleep: record.asleep.as_seconds(),
                window,
            });
        }
        Ok(Self {
            morning_of: record.morning_of,
            window: record.window,
            asleep: record.asleep,
            landed_as: record.landed_as,
            source_record_id: record.source_record_id,
            provenance: record.provenance,
            recorded_by: record.recorded_by,
        })
    }

    pub const fn morning_of(&self) -> Date {
        self.morning_of
    }

    pub const fn window(&self) -> &SleepWindow {
        &self.window
    }

    pub const fn asleep(&self) -> PositiveDuration {
        self.asleep
    }

    /// The watch that recorded it, where Garmin's records name one.
    pub const fn recorded_by(&self) -> Option<&DeviceIdentifier> {
        self.recorded_by.as_ref()
    }

    pub const fn landed_as(&self) -> LandingRecordId {
        self.landed_as
    }

    pub const fn source_record_id(&self) -> &SourceRecordId {
        &self.source_record_id
    }

    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

impl NormalisedEntity for Sleep {
    fn composes(&self) -> Vec<&SourceRecordId> {
        vec![&self.source_record_id]
    }
}

impl fmt::Display for Sleep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} — {} asleep, {} to {}",
            self.morning_of,
            self.asleep,
            self.window.fell_asleep.wall_clock().time(),
            self.window.woke.wall_clock().time()
        )
    }
}
