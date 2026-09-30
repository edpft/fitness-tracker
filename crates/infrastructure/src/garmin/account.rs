//! Which of Garmin's records is the night that stands.
//!
//! **One record is one night** — `hrv-service/hrv/{date}` answers with the whole
//! of what Garmin says about it, so there is nothing to compose (§ II.3.1's
//! degenerate case). What there *is* to do is supersession, and Garmin makes it
//! ordinary rather than exotic: it revises a night as later nights move the
//! baseline, and the extractor re-asks for the watermark's own night on every
//! run for exactly that reason. Two records for one date are one source
//! contradicting itself, the later one stands (§ 10), and the earlier is set
//! aside rather than dropped — a record with no outcome is what § 38's
//! reconciliation exists to catch.

use application::ports::SourceAccount;
use domain::landing::LandedRecord;

/// One night, and the servings of it that a later one replaced.
#[derive(Debug, Clone)]
pub struct NightAccount {
    night: LandedRecord,
    superseded: Vec<LandedRecord>,
}

impl NightAccount {
    /// The serving that stands.
    pub const fn night(&self) -> &LandedRecord {
        &self.night
    }
}

impl SourceAccount for NightAccount {
    fn records(&self) -> usize {
        self.superseded.len().saturating_add(1)
    }

    fn superseded(&self) -> usize {
        self.superseded.len()
    }
}

/// Keep the last serving of each night and set the earlier ones aside.
///
/// Last by landing id, which is the order the source served them, because raw is
/// append-only. Accounts come back oldest first by that same id, which is what
/// [`application::ports::AccountReader`] promises.
pub fn nights(records: Vec<LandedRecord>) -> Vec<NightAccount> {
    let mut accounts: Vec<NightAccount> = Vec::with_capacity(records.len());

    for record in records {
        if let Some(held) = accounts
            .iter_mut()
            .find(|held| held.night.source_record_id() == record.source_record_id())
        {
            if record.id() > held.night.id() {
                let previous = std::mem::replace(&mut held.night, record);
                held.superseded.push(previous);
            } else {
                held.superseded.push(record);
            }
        } else {
            accounts.push(NightAccount {
                night: record,
                superseded: Vec::new(),
            });
        }
    }

    accounts.sort_by_key(|account| account.night.id().as_i64());
    accounts
}

/// One gym activity, its sets, its recording, and the servings a later one
/// replaced.
///
/// **Three landing tables, one account** (§ 3.1). Garmin serves an activity's
/// start, duration, device and heart-rate summary from the activity list, its
/// sets from `/activity-service/activity/{id}/exerciseSets`, and the file the
/// watch wrote from `/download-service/files/activity/{id}` — all three fetched
/// per activity in the same run. None of them overlaps, so there are no rival
/// claims to prefer between: the list never states a set or a reading, and
/// neither companion states a session.
///
/// **Both companions are optional and their absence is not a refusal.** 179 of
/// the operator's gym activities have no sets, every one from 2015 and 2016
/// among them — the watch recorded a heart rate and nothing else, which is a
/// fact about the session rather than a companion that failed to land. A
/// recording that has not landed is the same: the summary the list states holds
/// on its own.
#[derive(Debug, Clone)]
pub struct ActivityAccount {
    activity: LandedRecord,
    sets: Option<LandedRecord>,
    recording: Option<LandedRecord>,
    superseded: Vec<LandedRecord>,
}

impl ActivityAccount {
    /// The serving of the activity that stands.
    pub const fn activity(&self) -> &LandedRecord {
        &self.activity
    }

    /// The serving of its sets that stands, where any landed.
    pub const fn sets(&self) -> Option<&LandedRecord> {
        self.sets.as_ref()
    }

    /// The serving of the file the watch wrote that stands, where one landed.
    pub const fn recording(&self) -> Option<&LandedRecord> {
        self.recording.as_ref()
    }
}

impl SourceAccount for ActivityAccount {
    fn records(&self) -> usize {
        self.superseded
            .len()
            .saturating_add(1)
            .saturating_add(usize::from(self.sets.is_some()))
            .saturating_add(usize::from(self.recording.is_some()))
    }

    fn superseded(&self) -> usize {
        self.superseded.len()
    }
}

/// Pair each activity with its sets and its recording, keeping the last serving
/// of each.
///
/// Last by landing id, as [`nights`] does it and for the same reason: raw is
/// append-only, so the id is the order the source served them. A companion for
/// an activity that never landed is not an account — there is no session for it
/// to belong to, and the activity walk is what enumerates the corpus.
pub fn activities(
    activities: Vec<LandedRecord>,
    sets: Vec<LandedRecord>,
    recordings: Vec<LandedRecord>,
) -> Vec<ActivityAccount> {
    let mut accounts: Vec<ActivityAccount> = Vec::with_capacity(activities.len());

    for record in activities {
        if let Some(held) = accounts
            .iter_mut()
            .find(|held| held.activity.source_record_id() == record.source_record_id())
        {
            if record.id() > held.activity.id() {
                let previous = std::mem::replace(&mut held.activity, record);
                held.superseded.push(previous);
            } else {
                held.superseded.push(record);
            }
        } else {
            accounts.push(ActivityAccount {
                activity: record,
                sets: None,
                recording: None,
                superseded: Vec::new(),
            });
        }
    }

    for record in sets {
        let Some(held) = accounts
            .iter_mut()
            .find(|held| held.activity.source_record_id() == record.source_record_id())
        else {
            continue;
        };
        keep_latest(&mut held.sets, &mut held.superseded, record);
    }

    for record in recordings {
        let Some(held) = accounts
            .iter_mut()
            .find(|held| held.activity.source_record_id() == record.source_record_id())
        else {
            continue;
        };
        keep_latest(&mut held.recording, &mut held.superseded, record);
    }

    accounts.sort_by_key(|account| account.activity.id().as_i64());
    accounts
}

/// Hold the later serving of a companion and set the other one aside.
fn keep_latest(
    held: &mut Option<LandedRecord>,
    superseded: &mut Vec<LandedRecord>,
    record: LandedRecord,
) {
    match held {
        Some(standing) if standing.id() >= record.id() => superseded.push(record),
        Some(_) => {
            if let Some(previous) = held.replace(record) {
                superseded.push(previous);
            }
        }
        None => *held = Some(record),
    }
}
