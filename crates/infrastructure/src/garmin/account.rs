//! Which of Garmin's records is the night, or the day, that stands.
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
    /// Garmin's sleep record for the same date, which names the watch (#569).
    ///
    /// **Read, not counted**: it is the sleep stream's record, composed here
    /// for the one thing it says about this night.
    sleep: Option<LandedRecord>,
}

impl NightAccount {
    /// The serving that stands.
    pub const fn night(&self) -> &LandedRecord {
        &self.night
    }

    /// This night, with Garmin's sleep record for its date.
    #[must_use]
    pub fn slept(self, sleep: Option<LandedRecord>) -> Self {
        Self { sleep, ..self }
    }

    /// Garmin's sleep record for its date, where one landed.
    pub const fn sleep(&self) -> Option<&LandedRecord> {
        self.sleep.as_ref()
    }
}

impl SourceAccount for NightAccount {
    fn records(&self) -> usize {
        self.superseded.len().saturating_add(1)
    }

    fn landed(&self) -> Vec<&LandedRecord> {
        std::iter::once(&self.night)
            .chain(&self.superseded)
            .chain(self.sleep.as_ref())
            .collect()
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
    last_of_each(records)
        .into_iter()
        .map(|(night, superseded)| NightAccount {
            night,
            superseded,
            sleep: None,
        })
        .collect()
}

/// One day of the wrist's heart rate, and the servings of it that a later one
/// replaced.
///
/// **Supersession for the reason a night has it**: the walk re-asks for the
/// watermark's own day, because today's answer grows until the day is over.
#[derive(Debug, Clone)]
pub struct DayAccount {
    day: LandedRecord,
    superseded: Vec<LandedRecord>,
    /// Garmin's sleep record for the same date, which names the watch (#569).
    ///
    /// **Read, not counted**: it is the sleep stream's record, composed here
    /// for the one thing it says about this day.
    sleep: Option<LandedRecord>,
}

impl DayAccount {
    /// The serving that stands.
    pub const fn day(&self) -> &LandedRecord {
        &self.day
    }

    /// This day, with Garmin's sleep record for its date.
    #[must_use]
    pub fn slept(self, sleep: Option<LandedRecord>) -> Self {
        Self { sleep, ..self }
    }

    /// Garmin's sleep record for its date, where one landed.
    pub const fn sleep(&self) -> Option<&LandedRecord> {
        self.sleep.as_ref()
    }
}

impl SourceAccount for DayAccount {
    fn records(&self) -> usize {
        self.superseded.len().saturating_add(1)
    }

    fn landed(&self) -> Vec<&LandedRecord> {
        std::iter::once(&self.day)
            .chain(&self.superseded)
            .chain(self.sleep.as_ref())
            .collect()
    }

    fn superseded(&self) -> usize {
        self.superseded.len()
    }
}

/// Keep the last serving of each day and set the earlier ones aside, as
/// [`nights`] does.
pub fn days(records: Vec<LandedRecord>) -> Vec<DayAccount> {
    last_of_each(records)
        .into_iter()
        .map(|(day, superseded)| DayAccount {
            day,
            superseded,
            sleep: None,
        })
        .collect()
}

/// The last serving of each source record and the ones it replaced, oldest
/// first by the serving that stands.
fn last_of_each(records: Vec<LandedRecord>) -> Vec<(LandedRecord, Vec<LandedRecord>)> {
    let mut accounts: Vec<(LandedRecord, Vec<LandedRecord>)> = Vec::with_capacity(records.len());

    for record in records {
        if let Some((held, superseded)) = accounts
            .iter_mut()
            .find(|(held, _)| held.source_record_id() == record.source_record_id())
        {
            if record.id() > held.id() {
                let previous = std::mem::replace(held, record);
                superseded.push(previous);
            } else {
                superseded.push(record);
            }
        } else {
            accounts.push((record, Vec::new()));
        }
    }

    accounts.sort_by_key(|(held, _)| held.id().as_i64());
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

    fn landed(&self) -> Vec<&LandedRecord> {
        std::iter::once(&self.activity)
            .chain(self.sets.as_ref())
            .chain(self.recording.as_ref())
            .chain(&self.superseded)
            .collect()
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
