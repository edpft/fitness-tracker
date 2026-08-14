//! What the domain would not accept, persisted so it can be read back.
//!
//! FR-023: refusals are queryable after a derivation, so what the domain will
//! not accept is visible rather than surfacing only in a log. The reason is
//! stored as a key rather than a sentence, which is what makes "the refusals
//! are exactly the named set" a `WHERE` clause instead of a grep.

use application::{RefusalStore, StoreError};
use domain::{
    gym::{
        Exercise, NormalisationRunId, Refusal, RefusalCount, RefusalLocus, RefusalReason,
        exercise::{DistanceExercise, DurationExercise, RepsExercise, TimedDistanceExercise},
    },
    landing::{InvalidStream, LandingRecordId, LandingStream, SourceRecordId},
};
use sqlx::SqlitePool;

use super::{corrupt, normalisation_run_for_storage, store_error};

/// Refusals for Hevy workouts.
#[derive(Debug, Clone)]
pub struct SqliteRefusalStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteRefusalStore {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(super::HevyWorkoutLandingStore::STREAM)?,
        })
    }
}

/// The locus, written flat, with `CHECK` constraints in the file mirroring the
/// sum type.
const fn locus_columns(
    locus: RefusalLocus,
) -> (&'static str, Option<i64>, Option<i64>, Option<i64>) {
    match locus {
        RefusalLocus::Record => ("record", None, None, None),
        RefusalLocus::Entry { entry } => ("entry", Some(entry as i64), None, None),
        RefusalLocus::Set { entry, set } => ("set", Some(entry as i64), Some(set as i64), None),
        RefusalLocus::Grouping { group } => ("grouping", None, None, Some(group as i64)),
    }
}

fn locus_from_row(
    kind: &str,
    entry: Option<i64>,
    set: Option<i64>,
    group: Option<i64>,
) -> Result<RefusalLocus, StoreError> {
    let index = |value: Option<i64>, field: &str| {
        value
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| StoreError::Corrupt {
                detail: format!("a {kind} refusal is missing its {field}"),
            })
    };

    match kind {
        "record" => Ok(RefusalLocus::Record),
        "entry" => Ok(RefusalLocus::Entry {
            entry: index(entry, "exercise index")?,
        }),
        "set" => Ok(RefusalLocus::Set {
            entry: index(entry, "exercise index")?,
            set: index(set, "set index")?,
        }),
        "grouping" => Ok(RefusalLocus::Grouping {
            group: index(group, "superset id")?,
        }),
        other => Err(StoreError::Corrupt {
            detail: format!("{other:?} is not a refusal locus this version knows"),
        }),
    }
}

/// Read an exercise back, whichever vocabulary it belongs to.
///
/// The keys are distinct across all four — a property test asserts it — so
/// trying each in turn resolves exactly one.
fn exercise_from_row(key: &str) -> Result<Exercise, StoreError> {
    RepsExercise::try_from(key)
        .map(Exercise::Reps)
        .or_else(|_| DurationExercise::try_from(key).map(Exercise::Duration))
        .or_else(|_| DistanceExercise::try_from(key).map(Exercise::Distance))
        .or_else(|_| TimedDistanceExercise::try_from(key).map(Exercise::TimedDistance))
        .map_err(|error| corrupt(&error))
}

/// Rebuild a reason from its key and whatever the source said.
///
/// A key this version does not know is a refusal recorded by a version that
/// knew something this one does not, and it is an error here so the caller
/// decides — the same treatment `FailureReason` gets.
fn reason_from_row(reason: &str, detail: Option<String>) -> Result<RefusalReason, StoreError> {
    let detail = detail.unwrap_or_default();
    match reason {
        "zero-on-absolute-load" => Ok(RefusalReason::ZeroOnAbsoluteLoad),
        "band-resistance" => Ok(RefusalReason::BandResistance),
        "zero-reps" => Ok(RefusalReason::ZeroReps),
        "non-contiguous-grouping" => Ok(RefusalReason::NonContiguousGrouping),
        "single-member-grouping" => Ok(RefusalReason::SingleMemberGrouping),
        "no-sets-in-entry" => Ok(RefusalReason::NoSetsInEntry),
        "unknown-set-kind" => Ok(RefusalReason::UnknownSetKind { kind: detail }),
        "unrecognised-intensity" => Ok(RefusalReason::UnrecognisedIntensity { value: detail }),
        "nothing-translatable" => Ok(RefusalReason::NothingTranslatable),
        "unreadable-payload" => Ok(RefusalReason::UnreadablePayload { detail }),
        "unreadable-value" => Ok(RefusalReason::UnreadableValue {
            field: "value",
            detail,
        }),
        other => Err(StoreError::Corrupt {
            detail: format!("{other:?} is not a refusal reason this version knows"),
        }),
    }
}

impl RefusalStore for SqliteRefusalStore {
    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn replace(
        &self,
        run: NormalisationRunId,
        refusals: Vec<Refusal>,
    ) -> Result<RefusalCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        sqlx::query!("DELETE FROM normalisation_refusal")
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;

        let written = u64::try_from(refusals.len()).unwrap_or(u64::MAX);
        for refusal in &refusals {
            let (locus_kind, entry_index, set_index, group_id) = locus_columns(refusal.locus);
            let landing_record_id = refusal.landed_as.as_i64();
            let source_record_id = refusal.source_record_id.as_str();
            let exercise = refusal.exercise.map(domain::gym::Exercise::as_str);
            let reason = refusal.reason.as_str();
            let kind = refusal.kind().as_str();
            let detail = refusal.reason.detail();

            sqlx::query!(
                r#"
                INSERT INTO normalisation_refusal (
                    run_id, landing_record_id, source_record_id,
                    locus_kind, entry_index, set_index, group_id,
                    exercise, reason, kind, detail
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                run_id,
                landing_record_id,
                source_record_id,
                locus_kind,
                entry_index,
                set_index,
                group_id,
                exercise,
                reason,
                kind,
                detail
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(RefusalCount::from(written))
    }

    async fn all(&self) -> Result<Vec<Refusal>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT landing_record_id AS "landing_record_id!: i64",
                   source_record_id AS "source_record_id!: String",
                   locus_kind AS "locus_kind!: String",
                   entry_index AS "entry_index: i64",
                   set_index AS "set_index: i64",
                   group_id AS "group_id: i64",
                   exercise AS "exercise: String",
                   reason AS "reason!: String",
                   detail AS "detail: String"
            FROM normalisation_refusal
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut refusals = Vec::with_capacity(rows.len());
        for row in rows {
            refusals.push(Refusal {
                landed_as: LandingRecordId::try_from(row.landing_record_id)
                    .map_err(|error| corrupt(&error))?,
                source_record_id: SourceRecordId::try_from(row.source_record_id.as_str())
                    .map_err(|error| corrupt(&error))?,
                locus: locus_from_row(
                    &row.locus_kind,
                    row.entry_index,
                    row.set_index,
                    row.group_id,
                )?,
                exercise: row.exercise.as_deref().map(exercise_from_row).transpose()?,
                reason: reason_from_row(&row.reason, row.detail)?,
            });
        }

        Ok(refusals)
    }
}

/// The stored `kind` is derived from the reason, and this is what keeps the two
/// honest: reading a refusal back and re-deriving its kind must give what was
/// written.
#[cfg(test)]
mod tests {
    use domain::gym::RefusalKind;

    use super::reason_from_row;

    #[test]
    fn a_reason_round_trips_through_its_key() {
        for (key, kind) in [
            ("zero-on-absolute-load", RefusalKind::WrongData),
            ("band-resistance", RefusalKind::DeclaredLimitation),
            ("zero-reps", RefusalKind::Unmodelled),
            ("non-contiguous-grouping", RefusalKind::WrongData),
            ("single-member-grouping", RefusalKind::WrongData),
        ] {
            let reason = reason_from_row(key, None).expect("a known reason reads back");
            assert_eq!(reason.as_str(), key);
            assert_eq!(reason.kind(), kind);
        }
    }

    #[test]
    fn a_reason_this_version_does_not_know_is_corrupt_rather_than_guessed() {
        assert!(reason_from_row("invented-later", None).is_err());
    }
}
