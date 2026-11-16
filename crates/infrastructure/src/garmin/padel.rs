//! Turning Garmin's account of a padel match into a [`PadelMatch`] (#517).
//!
//! **`paddelball` is Garmin's word for it**, and the one place it is named:
//! the watch's Padel profile files a match under that type. The operator's
//! record holds 15, from 2026-03-22.
//!
//! **The list's own figures, and no more**: a start, a duration and the heart
//! rate summary the watch stated, read as a gym session's are
//! ([`super::gym`]). The recording's samples stay in raw.

use domain::{
    landing::LandedRecord,
    normalised::{RefusalReason, StatedZones},
    padel::{PadelMatch, PadelMatchRecord},
};

use super::gym::{Activity, Stated, heart_rate, seconds, started};

/// Garmin's activity type for a match of padel.
const PADEL: &str = "paddelball";

/// The match `record` is, or why it is not one. [`None`] where the activity is
/// not padel, or cannot be read at all, which the gym translator reports.
pub(super) fn translate(
    record: &LandedRecord,
    zones: &StatedZones,
) -> Option<Result<PadelMatch, RefusalReason>> {
    let activity: Activity = serde_json::from_slice(record.payload().as_bytes()).ok()?;
    (activity.kind.key == PADEL).then(|| played(&activity, record, zones))
}

fn played(
    activity: &Activity,
    record: &LandedRecord,
    zones: &StatedZones,
) -> Result<PadelMatch, RefusalReason> {
    let started_at = activity
        .started_at
        .as_deref()
        .ok_or(RefusalReason::MissingFigure { figure: "a start" })
        .and_then(|stamp| started(stamp, zones))?;
    let duration = match seconds(activity.duration.as_deref()) {
        Stated::Length(duration) => duration,
        Stated::NoLength => return Err(RefusalReason::NothingHappened),
        Stated::Nothing => {
            return Err(RefusalReason::MissingFigure {
                figure: "a duration",
            });
        }
    };
    Ok(PadelMatch::new(PadelMatchRecord {
        started_at,
        duration,
        heart_rate: heart_rate(activity),
        landed_as: record.id(),
        source_record_id: record.source_record_id().clone(),
        provenance: record.provenance().clone(),
    }))
}
