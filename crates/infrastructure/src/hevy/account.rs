//! Everything Hevy says about one gym session.
//!
//! **One level of composition, where Peloton has two.** Hevy serves a workout
//! whole — its exercises and every set are in the one payload — so there is no
//! second endpoint to go back to. What composes here is only the session: one or
//! more workout records, because the operator split single sessions across
//! several routines so the parts could be composed.
//!
//! Hevy's shape rather than the store's, which is why it is declared here and
//! not beside the query that fills it. What one source's account of one session
//! consists of is a fact about that source.

use application::SourceAccount;
use domain::{
    landing::{EventKind, LandedRecord, Provenance},
    sequence::NonEmpty,
};

/// One session, as far as Hevy has been asked about it.
///
/// Its workouts are in the order they were performed. Unlike
/// [`crate::peloton::SessionAccount`] the order carries no roles — it is the
/// sequence and nothing more — because Hevy states nothing about what a workout
/// was for.
#[derive(Debug, Clone)]
pub struct SessionAccount {
    workouts: NonEmpty<LandedRecord>,
    /// Earlier servings of workouts in this session, kept only to be counted.
    ///
    /// **§ 10, and § 3.1 is explicit that they do not compose.** Two records
    /// sharing a source id are one source contradicting itself; treating the
    /// second as another part would turn one workout told twice into a session
    /// of two. They are carried rather than dropped because a record with no
    /// outcome is what § 38's reconciliation exists to catch.
    ///
    /// The operator's store holds none of them today. It held none on the
    /// Peloton side either until the arithmetic was made to say so, and then it
    /// held 51.
    superseded: Vec<LandedRecord>,
}

impl SessionAccount {
    /// A session of the workouts given, or `None` where there are none.
    ///
    /// Non-empty by construction rather than by check: an account of no records
    /// would read as a record with no outcome in the run's reconciliation.
    ///
    /// `superseded` is the earlier servings of those same workouts, carried only
    /// so that they are counted.
    pub fn of(workouts: Vec<LandedRecord>, superseded: Vec<LandedRecord>) -> Option<Self> {
        NonEmpty::new(workouts).ok().map(|workouts| Self {
            workouts,
            superseded,
        })
    }

    /// The session's workouts, in the order they were performed.
    ///
    /// Non-empty as a type rather than as a promise, so reading the first one
    /// needs no index and can raise no panic (§ 26).
    pub const fn workouts(&self) -> &NonEmpty<LandedRecord> {
        &self.workouts
    }
}

impl SourceAccount for SessionAccount {
    /// One per record, standing or superseded. A Hevy workout is one record, so
    /// this is the count of parts plus the count of re-servings.
    fn records(&self) -> usize {
        self.workouts.count() + self.superseded.len()
    }

    fn superseded(&self) -> usize {
        self.superseded.len()
    }
}

/// Keep the last serving of each workout and set the earlier ones aside.
///
/// **Across everything, before anything is grouped**, for the reason
/// [`crate::peloton::supersede`] does it there: a re-serving states the same
/// times as the serving it replaces, so it would land in the same session and
/// read as a second part.
///
/// Last by landing id, which is the order the source served them, because raw
/// is append-only.
///
/// **Servings only. A deletion is never superseded and never supersedes.** This
/// source's events feed serves `updated` and `deleted` under the same source
/// identifier — that is how a deletion is expressed — so treating the pair as
/// two servings would let one silently swallow the other. Which way it went
/// would then depend on landing order: a tombstone that landed before a later
/// update would be dropped, and a retraction that only fires in one order is not
/// the absorbing rule § 7 asks for.
///
/// So a deletion always reaches the translator and always becomes a retraction,
/// and the use case withdraws whatever entity composed the record it names,
/// wherever that entity sat in the sequence. What is left to supersede here is
/// what § 10 is actually about: one source contradicting itself about a workout
/// it is still asserting.
pub fn supersede(records: Vec<LandedRecord>) -> (Vec<LandedRecord>, Vec<LandedRecord>) {
    let mut current: Vec<LandedRecord> = Vec::with_capacity(records.len());
    let mut superseded = Vec::new();

    for record in records {
        if !is_serving(&record) {
            current.push(record);
            continue;
        }
        let id = record.source_record_id().clone();
        if let Some(existing) = current
            .iter_mut()
            .find(|held| is_serving(held) && *held.source_record_id() == id)
        {
            if record.id() > existing.id() {
                superseded.push(std::mem::replace(existing, record));
            } else {
                superseded.push(record);
            }
        } else {
            current.push(record);
        }
    }

    (current, superseded)
}

/// Whether this record is the source asserting a workout, rather than
/// withdrawing one or saying something we do not translate.
fn is_serving(record: &LandedRecord) -> bool {
    let Provenance::Event(event) = record.provenance();
    *event.kind() == EventKind::Updated
}
