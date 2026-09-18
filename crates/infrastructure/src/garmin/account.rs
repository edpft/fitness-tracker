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
