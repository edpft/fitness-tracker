//! Garmin's overnight heart-rate variability.
//!
//! **Here rather than in a module of its own**, for the reason
//! [`super::weigh_in`]'s nerve and vascular readings are here: this is a
//! measurement of a body by the instrument that measured it, and the watch is
//! another instrument. § 6 lists HRV beside body fat percentage for the same
//! reason — the figure is inseparable from the sensor and the algorithm behind
//! it.
//!
//! **Read off all 634 nights the operator's watch has served** (2024-12-22 to
//! 2026-09-18, #161), which is where every `Option` and every absence below
//! comes from:
//!
//! - **The night's figures and the week's are two different things.** Garmin's
//!   `status` classifies the *weekly* average against the baseline, not last
//!   night's: bucketing the weekly average reproduces the status Garmin states
//!   on 592 of 592 nights, and last night's average on 411. So the weekly
//!   average, the baseline it is judged against and the status it is judged to
//!   be are one value ([`WeeklyStatus`]), and last night's reading is another.
//! - **The 5-minutely readings only go back about 140 days.** 504 of 634 nights
//!   are served with `hrvReadings` empty, every one of them older than
//!   2026-05-01, so a night's detail is gone rather than uncollected and
//!   [`LastNight::readings`] is the one thing here that may be absent.
//! - **A night with no baseline is not a night with HRV.** The operator's
//!   ruling, 2026-09-18: *"without a baseline, you can't actually report HRV"*.
//!   Garmin says `status: "NONE"` for the first 18 nights of a watch's life,
//!   while it gathers three weeks of sleep, and serves no baseline with them.
//!   Those nights are refused by the translator, which is why nothing here is
//!   optional to accommodate them.
//! - **The status is stated, never derived**, though it is reproducible from the
//!   two fields beside it. The operator: *"it's not transparently derivable in
//!   the way that, say, date is derivable from datetime"* — the thresholds and
//!   the rule are Garmin's method (§ 6), so a boundary they move is a value we
//!   would silently disagree with.
//!
//! **What stays in raw**: `feedbackPhrase`, which is `HRV_<STATUS>_<n>` and
//! carries the status we already hold plus which of the app's eight wordings it
//! showed; `baseline.markerValue`, the needle's position on the app's gauge,
//! clamped to zero on every LOW night; the sleep span, which is a fact about
//! sleep rather than about HRV — Garmin serves it here because HRV is measured
//! during sleep, and on all 130 nights that carry it the sleep window strictly
//! contains the measurement window; and `createTimeStamp` and `userProfilePk`,
//! which say when Garmin computed a summary and whose account it is.

use std::fmt;

use jiff::civil::Date;

use crate::{
    landing::{LandingRecordId, Provenance, SourceRecordId},
    measure::HeartRateVariability,
    normalised::{NormalisedEntity, StartedAt},
    sequence::NonEmpty,
};

/// Where Garmin says a week of HRV sits against what is normal for this body.
///
/// The three bounds are Garmin's, computed from several weeks of nights we do
/// not hold, which is why they are carried rather than derived (§ 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HrvBaseline {
    /// The top of the low band: at or below this, the week is [`HrvStatus::Low`].
    pub low_upper: HeartRateVariability,
    pub balanced_low: HeartRateVariability,
    pub balanced_upper: HeartRateVariability,
}

/// What Garmin makes of the week.
///
/// Three positions, which are the three the operator's 592 nights hold. A fourth
/// string refuses the night rather than being guessed at (§ 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HrvStatus {
    /// Within the balanced band.
    Balanced,
    /// Outside it, either side.
    Unbalanced,
    /// At or below the low bound.
    Low,
}

impl HrvStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Balanced => "balanced",
            Self::Unbalanced => "unbalanced",
            Self::Low => "low",
        }
    }
}

impl fmt::Display for HrvStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The week Garmin judged, and what it judged it to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeeklyStatus {
    /// Garmin's seven-day average. What [`Self::status`] classifies.
    pub average: HeartRateVariability,
    pub baseline: HrvBaseline,
    pub status: HrvStatus,
}

/// One of the watch's readings, at the resolution it took it (§ II.3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HrvReading {
    pub taken_at: StartedAt,
    pub value: HeartRateVariability,
}

/// What Garmin says about the night just measured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastNight {
    pub average: HeartRateVariability,
    /// The highest five-minute reading. Exactly `max(readings)` on all 130
    /// nights that have readings — and carried anyway, because the 462 that do
    /// not still state it.
    pub five_minute_high: HeartRateVariability,
    /// The individual readings, five minutes apart, where Garmin still serves
    /// them. Absent before 2026-05-01 and never coming back.
    pub readings: Option<NonEmpty<HrvReading>>,
}

/// Why a window could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a measurement window spans time, and this one ends at {at}, having started at {from}")]
pub struct InvalidWindow {
    from: String,
    at: String,
}

/// When the watch was measuring.
///
/// Two instants rather than an instant and a length, because the source states
/// both and the length is a function of them (§ 5). Both are [`StartedAt`]:
/// carrying an end in a type named for a start reads oddly, and is worth it for
/// the guarantee the type exists to give — a window cannot be built out of naive
/// times (§ II.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasurementWindow {
    from: StartedAt,
    until: StartedAt,
}

impl MeasurementWindow {
    /// # Errors
    ///
    /// [`InvalidWindow`] if the window ends before it starts, or spans no time
    /// at all. A window of no width is not a measurement (§ 24).
    pub fn new(from: StartedAt, until: StartedAt) -> Result<Self, InvalidWindow> {
        if until.instant() <= from.instant() {
            return Err(InvalidWindow {
                from: from.instant().to_string(),
                at: until.instant().to_string(),
            });
        }
        Ok(Self { from, until })
    }

    pub const fn from(&self) -> &StartedAt {
        &self.from
    }

    pub const fn until(&self) -> &StartedAt {
        &self.until
    }

    /// The morning the night is reported under.
    ///
    /// **Derived, and derived from the end.** It is Garmin's `calendarDate` on
    /// all 592 of the operator's nights, and the operator's own reading of
    /// "night of" is the evening — which is why this is not called that. The
    /// evening would not do as a label anyway: 501 of 592 windows cross
    /// midnight and 91 begin after it, so 75 dates in his record carry two
    /// different nights' windows, while every morning carries one.
    pub fn morning_of(&self) -> Date {
        self.until.wall_clock().date()
    }
}

/// Everything one night's record says, on its way into [`OvernightHrv`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OvernightHrvRecord {
    pub measured: MeasurementWindow,
    pub last_night: LastNight,
    pub weekly: WeeklyStatus,
    pub landed_as: LandingRecordId,
    pub source_record_id: SourceRecordId,
    pub provenance: Provenance,
}

/// One night of HRV, as Garmin told it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OvernightHrv {
    measured: MeasurementWindow,
    last_night: LastNight,
    weekly: WeeklyStatus,
    landed_as: LandingRecordId,
    source_record_id: SourceRecordId,
    provenance: Provenance,
}

impl OvernightHrv {
    pub fn new(record: OvernightHrvRecord) -> Self {
        Self {
            measured: record.measured,
            last_night: record.last_night,
            weekly: record.weekly,
            landed_as: record.landed_as,
            source_record_id: record.source_record_id,
            provenance: record.provenance,
        }
    }

    pub const fn measured(&self) -> &MeasurementWindow {
        &self.measured
    }

    pub const fn last_night(&self) -> &LastNight {
        &self.last_night
    }

    pub const fn weekly(&self) -> &WeeklyStatus {
        &self.weekly
    }

    /// The morning this night is reported under. See
    /// [`MeasurementWindow::morning_of`].
    pub fn morning_of(&self) -> Date {
        self.measured.morning_of()
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

impl NormalisedEntity for OvernightHrv {
    fn composes(&self) -> Vec<&SourceRecordId> {
        vec![&self.source_record_id]
    }
}

impl fmt::Display for OvernightHrv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} — {} ({})",
            self.morning_of(),
            self.last_night.average,
            self.weekly.status
        )
    }
}
