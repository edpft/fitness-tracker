//! Which of Garmin's sets a figures correction would reach.
//!
//! The operator names the set by the day he trained and the figures he can see
//! are wrong — "2018-06-11, the set recorded as 58 × 6 kg" — and what has to be
//! stored is the source's own identity for each set that answers to it.
//! Resolving one into the other needs Garmin's payloads, so it belongs here
//! rather than in a use case.
//!
//! **Enumerated from the payloads rather than from the normalised layer**, for
//! [`crate::hevy::standins`]'s reason: what the correction anchors on is the
//! instant Garmin states the set began, and the normalised layer holds that
//! instant placed in a zone rather than as the source wrote it.
//!
//! **Read through the translator's own shape**, so a set offered here is a set
//! the derivation will correct: the same `ACTIVE` filter and the same reading
//! of `repetitionCount` and `weight`.
//!
//! **One day at a time, and deliberately.** An exercise correction reaches a
//! span of months because "every pull-up since I left CrossFit" is one
//! assertion about a habit. A number typed wrongly is one assertion about one
//! session, and letting it reach a range would quietly correct every set in the
//! record that happened to share those figures.

use application::StoreError;
use domain::{
    landing::SourceRecordId,
    normalised::{CorrectedTerm, OperatorZone, SetFigures, SourceTerm},
};
use jiff::civil::{Date, Time};
use sqlx::SqlitePool;
use std::collections::{BTreeMap, BTreeSet};

use crate::store::store_error;

use super::gym::{ACTIVE, ExerciseSets, recorded_figures, started, term};

/// One set a figures correction would reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundSet {
    /// What the correction is anchored by: the record, and Garmin's own
    /// designation for the set inside it.
    pub set: CorrectedTerm,
    /// Garmin's own term for the movement, so the operator can recognise which
    /// set he is correcting. Absent where its classifier proposed nothing.
    pub movement: Option<String>,
    /// When it began on his own clock, for the report. The anchor holds the
    /// stamp as the source wrote it; this is the same instant in a form he
    /// reads off a session.
    pub began: Time,
}

/// Garmin's landed exercise sets, asked which of them record some figures.
#[derive(Debug, Clone)]
pub struct GarminRecordedSets {
    pool: SqlitePool,
}

impl GarminRecordedSets {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Every active set on `day` whose figures are `recorded`, in the order it
    /// was performed.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read or holds a set list that is
    /// not one.
    pub async fn recording(
        &self,
        recorded: SetFigures,
        zone: &OperatorZone,
        day: Date,
    ) -> Result<Vec<FoundSet>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT source_record_id AS "source_record_id!: String",
                   event_kind       AS "event_kind!: String",
                   payload          AS "payload!: Vec<u8>"
            FROM garmin_exercise_set_landing
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        // The effective serving of each record, the way a derivation sees it: a
        // later serving replaces an earlier one, and a deletion *anywhere* in a
        // record's history withdraws it whatever order the events arrived in.
        let mut withdrawn: BTreeSet<String> = BTreeSet::new();
        let mut latest: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for row in rows {
            if row.event_kind == "deleted" {
                withdrawn.insert(row.source_record_id);
                continue;
            }
            latest.insert(row.source_record_id, row.payload);
        }

        let mut found = Vec::new();
        for (source_record_id, payload) in latest {
            if withdrawn.contains(&source_record_id) {
                continue;
            }

            let served: ExerciseSets =
                serde_json::from_slice(&payload).map_err(|error| StoreError::Corrupt {
                    detail: format!("{source_record_id}: {error}"),
                })?;

            let record = SourceRecordId::try_from(source_record_id.clone()).map_err(|error| {
                StoreError::Corrupt {
                    detail: error.to_string(),
                }
            })?;

            for set in &served.sets {
                if set.set_type.as_deref() != Some(ACTIVE) {
                    continue;
                }
                let Some(stamp) = set.started_at.as_deref() else {
                    continue;
                };
                // A set whose start will not read is a set the derivation
                // refuses, so it is not a candidate here either.
                let Ok(at) = started(stamp, zone) else {
                    continue;
                };
                if at.wall_clock().date() != day {
                    continue;
                }
                if recorded_figures(set) != Some(recorded) {
                    continue;
                }
                let Ok(anchor) = SourceTerm::try_from(stamp) else {
                    continue;
                };
                found.push(FoundSet {
                    set: CorrectedTerm::new(record.clone(), anchor),
                    began: at.wall_clock().time(),
                    movement: set.exercises.first().and_then(|candidate| {
                        candidate
                            .category
                            .as_deref()
                            .map(|category| term(category, candidate.name.as_deref()))
                    }),
                });
            }
        }

        found.sort_by(|left, right| {
            left.set
                .record()
                .as_str()
                .cmp(right.set.record().as_str())
                .then_with(|| left.set.term().as_str().cmp(right.set.term().as_str()))
        });

        Ok(found)
    }
}
