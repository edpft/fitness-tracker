//! Everything Withings says about one weigh-in.
//!
//! **Every measure group stamped with the same second.** A Body Scan files one
//! step onto the scale as three or four groups (composition, heart, nerves,
//! vascular), each with its own id and none naming the others; the second they
//! share is the only thing that joins them (#153, agreed tentatively
//! 2026-09-17).

use std::collections::BTreeMap;

use application::SourceAccount;
use domain::{landing::LandedRecord, sequence::NonEmpty};
use serde::Deserialize;

/// One weigh-in's groups, in the order they landed.
#[derive(Debug, Clone)]
pub struct WeighInAccount {
    groups: NonEmpty<LandedRecord>,
    /// Earlier servings of these groups, kept only to be counted (§ 10): a
    /// group Withings modified comes back under the same id.
    superseded: Vec<LandedRecord>,
}

impl WeighInAccount {
    pub const fn groups(&self) -> &NonEmpty<LandedRecord> {
        &self.groups
    }
}

impl SourceAccount for WeighInAccount {
    fn records(&self) -> usize {
        self.groups.count().saturating_add(self.superseded.len())
    }

    fn superseded(&self) -> usize {
        self.superseded.len()
    }
}

/// The one field grouping reads. Everything else is the translator's.
#[derive(Deserialize)]
struct Stamp {
    date: i64,
}

/// Where a group is filed: its second, or its own landing id where the
/// payload will not say, so the translator can refuse it by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    At(i64),
    Unreadable(i64),
}

fn key_of(record: &LandedRecord) -> Key {
    serde_json::from_slice::<Stamp>(record.payload().as_bytes()).map_or_else(
        |_| Key::Unreadable(record.id().as_i64()),
        |stamp| Key::At(stamp.date),
    )
}

/// Records, oldest landed first, into weigh-ins, earliest first.
///
/// The latest serving of each group stands and the earlier ones go with it, so
/// every record lands in exactly one account.
pub fn group(records: Vec<LandedRecord>) -> Vec<WeighInAccount> {
    let mut latest: BTreeMap<String, LandedRecord> = BTreeMap::new();
    let mut earlier: BTreeMap<String, Vec<LandedRecord>> = BTreeMap::new();
    for record in records {
        let id = record.source_record_id().as_str().to_owned();
        if let Some(previous) = latest.insert(id.clone(), record) {
            earlier.entry(id).or_default().push(previous);
        }
    }

    let mut by_key: BTreeMap<Key, (Vec<LandedRecord>, Vec<LandedRecord>)> = BTreeMap::new();
    for (id, record) in latest {
        let entry = by_key.entry(key_of(&record)).or_default();
        entry.1.extend(earlier.remove(&id).unwrap_or_default());
        entry.0.push(record);
    }

    by_key
        .into_values()
        .filter_map(|(mut groups, superseded)| {
            groups.sort_by_key(LandedRecord::id);
            NonEmpty::new(groups)
                .ok()
                .map(|groups| WeighInAccount { groups, superseded })
        })
        .collect()
}
