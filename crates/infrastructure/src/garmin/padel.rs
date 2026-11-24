//! Turning Garmin's account of a padel match into a [`PadelMatch`] (#517).
//!
//! **`paddelball` is Garmin's word for it**, and the one place it is named:
//! the watch's Padel profile files a match under that type. The operator's
//! record holds 15, from 2026-03-22.
//!
//! **The list's figures and the recording's readings**: a start, a duration,
//! the heart rate summary the watch stated, and the readings its activity file
//! holds (#919), each read as a gym session's are ([`super::gym`]).

use domain::{
    measure::MeasuredHeartRate,
    normalised::{RefusalReason, StatedZones},
    padel::{PadelMatch, PadelMatchRecord},
};

use crate::scribe::Scribe;

use super::{
    ActivityAccount,
    gym::{Activity, Stated, heart_rate, recorded_series, seconds, started},
};

/// Garmin's activity type for a match of padel.
const PADEL: &str = "paddelball";

/// The match `account` is, or why it is not one. [`None`] where the activity
/// is not padel, or cannot be read at all, which the gym translator reports.
///
/// **A recording that will not read costs the readings and not the match**,
/// noted on `scribe`, as a gym session's does.
pub(super) fn translate(
    account: &ActivityAccount,
    zones: &StatedZones,
    scribe: &mut Scribe,
) -> Option<Result<PadelMatch, RefusalReason>> {
    let record = account.activity();
    let activity: Activity = serde_json::from_slice(record.payload().as_bytes()).ok()?;
    (activity.kind.key == PADEL).then(|| played(&activity, account, zones, scribe))
}

fn played(
    activity: &Activity,
    account: &ActivityAccount,
    zones: &StatedZones,
    scribe: &mut Scribe,
) -> Result<PadelMatch, RefusalReason> {
    let record = account.activity();
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
        heart_rate: heart_rate(activity)
            .map(|stated| MeasuredHeartRate::new(stated, recorded_series(account, scribe))),
        landed_as: record.id(),
        source_record_id: record.source_record_id().clone(),
        provenance: record.provenance().clone(),
    }))
}
