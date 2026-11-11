//! Turning Garmin's account of a padel game into a [`PadelGame`] (#517).
//!
//! **`paddelball` is Garmin's word for it**, and the one place it is named:
//! the watch's Padel profile files a game under that type. The operator's
//! record holds 15, from 2026-03-22.
//!
//! **The list's own figures, and no more**: a start, a duration and the heart
//! rate summary the watch stated, read as a gym session's are
//! ([`super::gym`]). The recording's samples stay in raw.

use domain::{
    landing::LandedRecord,
    normalised::{RefusalReason, StatedZones},
    padel::{PadelGame, PadelGameRecord},
};

use super::gym::{Activity, Stated, heart_rate, seconds, started};

/// Garmin's activity type for a game of padel.
const PADEL: &str = "paddelball";

/// The game `record` is, or why it is not one. [`None`] where the activity is
/// not padel, or cannot be read at all, which the gym translator reports.
pub(super) fn game(
    record: &LandedRecord,
    zones: &StatedZones,
) -> Option<Result<PadelGame, RefusalReason>> {
    let activity: Activity = serde_json::from_slice(record.payload().as_bytes()).ok()?;
    (activity.kind.key == PADEL).then(|| played(&activity, record, zones))
}

fn played(
    activity: &Activity,
    record: &LandedRecord,
    zones: &StatedZones,
) -> Result<PadelGame, RefusalReason> {
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
    Ok(PadelGame::new(PadelGameRecord {
        started_at,
        duration,
        heart_rate: heart_rate(activity),
        landed_as: record.id(),
        source_record_id: record.source_record_id().clone(),
        provenance: record.provenance().clone(),
    }))
}
