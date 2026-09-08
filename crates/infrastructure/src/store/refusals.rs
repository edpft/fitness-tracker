//! What the domain would not accept, persisted so it can be read back.
//!
//! Persisted so they can be read back: what the domain will not accept has to
//! be visible rather than surfacing only in a log. The reason is stored as a key
//! rather than a sentence, which is what makes "the refusals are exactly the
//! named set" a `WHERE` clause instead of a grep.

use application::{RefusalStore, StoreError};
use domain::{
    gym::{
        Exercise,
        exercise::{DistanceExercise, DurationExercise, RepsExercise},
    },
    landing::{InvalidStream, LandingRecordId, LandingStream, SourceRecordId},
    normalised::{NormalisationRunId, Refusal, RefusalCount, RefusalLocus, RefusalReason},
};
use sqlx::SqlitePool;

use super::{corrupt, normalisation_run_for_storage, store_error};

/// Refusals for one stream.
///
/// **The stream is a constructor argument, and this is the one store where it
/// has to be.** Every other one is bound to a table and is asked which stream
/// it is about; refusals from every stream share a table, because what a
/// refusal *is* does not vary by source and a column per landing table would
/// grow with the catalogue. So the stream cannot be read off the table, and is
/// taken from the landing store the derivation is already bound to rather than
/// named again here — which keeps a run's identity derived rather than passed.
#[derive(Debug, Clone)]
pub struct SqliteRefusalStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteRefusalStore {
    /// # Errors
    ///
    /// [`InvalidStream`] if the stream constant it is given is not a stream
    /// name.
    pub fn new(pool: SqlitePool, stream: &str) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(stream)?,
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
        "unmodelled" => Ok(RefusalReason::Unmodelled { detail }),
        "companion-not-landed" => Ok(RefusalReason::CompanionNotLanded { stream: detail }),
        // The series is a `&'static str` on the way out and text on the way
        // back, so what is read is the name and not the identity. Both series a
        // ride can refuse are named here; anything else is a version that knew
        // something this one does not, which is the arm below.
        "no-readings-in-series" => Ok(RefusalReason::NoReadingsInSeries {
            series: series_named(&detail)?,
        }),
        "missing-series" => Ok(RefusalReason::MissingSeries {
            series: series_named(&detail)?,
        }),
        other => Err(StoreError::Corrupt {
            detail: format!("{other:?} is not a refusal reason this version knows"),
        }),
    }
}

/// A series name, back as the `&'static str` the reason carries.
///
/// The set is closed on purpose. A refusal is read against the derivation that
/// produced it, and a name this version does not have is a row written by a
/// version that knew a series this one does not.
fn series_named(detail: &str) -> Result<&'static str, StoreError> {
    match detail {
        "heart rate" => Ok("heart rate"),
        "bike sample" => Ok("bike sample"),
        "output" => Ok("output"),
        "cadence" => Ok("cadence"),
        "resistance" => Ok("resistance"),
        "speed" => Ok("speed"),
        other => Err(StoreError::Corrupt {
            detail: format!("{other:?} is not a series this version knows"),
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

        // This stream's, and only this stream's. Emptying the table would be
        // one derivation deleting what another had just recorded.
        let stream = self.stream.to_string();
        sqlx::query!("DELETE FROM normalisation_refusal WHERE stream = ?", stream)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;

        let written = refusals.len();
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
                    run_id, stream, landing_record_id, source_record_id,
                    locus_kind, entry_index, set_index, group_id,
                    exercise, reason, kind, detail
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                run_id,
                stream,
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
        let stream = self.stream.to_string();
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
            WHERE stream = ?
            ORDER BY id ASC
            "#,
            stream
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
    use domain::normalised::RefusalKind;

    use super::reason_from_row;

    #[test]
    fn a_reason_round_trips_through_its_key() {
        for (key, kind) in [
            // `zero-reps` is deliberately absent: a set of zero repetitions is a
            // failed attempt now, so no build writes that key and this one refuses
            // to read it. Migration `0008` clears the rows that hold it.
            ("non-contiguous-grouping", RefusalKind::WrongData),
            ("single-member-grouping", RefusalKind::WrongData),
            ("no-sets-in-entry", RefusalKind::WrongData),
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
