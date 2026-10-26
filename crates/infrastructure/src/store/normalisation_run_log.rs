//! What happened on each derivation.
//!
//! A parallel of [`super::run_log`] rather than a widening of it. Extraction
//! and derivation fail for different reasons and report different numbers — one
//! counts what a source served, the other counts what our own translation made
//! of it — and one table carrying both would have half its columns empty in
//! either direction.

use application::{Mark, NormalisationRunLog, ReadUpTo, StoreError, Terms};
use domain::{
    landing::{FetchedAt, LandingStream, RecordCount},
    normalised::{
        NormalisationOutcome, NormalisationRun, NormalisationRunId, RefusalCount, WorkoutCount,
    },
};
use sqlx::SqlitePool;

use super::{
    corrupt, count_for_storage, count_from_storage, normalisation_run_for_storage, store_error,
};

/// The derivation history.
#[derive(Debug, Clone)]
pub struct SqliteNormalisationRunLog {
    pool: SqlitePool,
}

impl SqliteNormalisationRunLog {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl NormalisationRunLog for SqliteNormalisationRunLog {
    async fn begin(
        &self,
        stream: &LandingStream,
        at: FetchedAt,
    ) -> Result<NormalisationRunId, StoreError> {
        let stream = stream.to_string();
        let started_at = at.to_string();

        let id = sqlx::query!(
            "INSERT INTO normalisation_run (stream, started_at) VALUES (?, ?) RETURNING id",
            stream,
            started_at
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error(&error))?
        .id;

        NormalisationRunId::try_from(id).map_err(|error| corrupt(&error))
    }

    async fn finish(
        &self,
        run: NormalisationRunId,
        outcome: NormalisationOutcome,
    ) -> Result<(), StoreError> {
        let id = normalisation_run_for_storage(run)?;

        match outcome {
            // Nothing to record. A run that has not finished is exactly a run
            // with no outcome, which is what the row already says.
            NormalisationOutcome::InFlight => Ok(()),
            NormalisationOutcome::Succeeded {
                finished_at,
                records_read,
                workouts_written,
                records_composed,
                records_superseded,
                workouts_retracted,
                retractions_read,
                records_refused,
                refusals_recorded,
                records_covered,
            } => {
                let finished_at = finished_at.to_string();
                let records_read = count_for_storage(records_read.as_usize())?;
                let workouts_written = count_for_storage(workouts_written.as_usize())?;
                let records_composed = count_for_storage(records_composed.as_usize())?;
                let records_superseded = count_for_storage(records_superseded.as_usize())?;
                let workouts_retracted = count_for_storage(workouts_retracted.as_usize())?;
                let retractions_read = count_for_storage(retractions_read.as_usize())?;
                let records_refused = count_for_storage(records_refused.as_usize())?;
                let refusals_recorded = count_for_storage(refusals_recorded.as_usize())?;
                let records_covered = count_for_storage(records_covered.as_usize())?;

                sqlx::query!(
                    r#"
                    UPDATE normalisation_run
                    SET finished_at = ?, outcome = 'succeeded',
                        records_read = ?, workouts_written = ?, records_composed = ?,
                        records_superseded = ?, workouts_retracted = ?,
                        retractions_read = ?, records_refused = ?, refusals_recorded = ?,
                        records_covered = ?
                    WHERE id = ?
                    "#,
                    finished_at,
                    records_read,
                    workouts_written,
                    records_composed,
                    records_superseded,
                    workouts_retracted,
                    retractions_read,
                    records_refused,
                    refusals_recorded,
                    records_covered,
                    id
                )
                .execute(&self.pool)
                .await
                .map_err(|error| store_error(&error))?;
                Ok(())
            }
            NormalisationOutcome::Failed {
                finished_at,
                reason,
            } => {
                let finished_at = finished_at.to_string();
                let reason = reason.as_str();

                sqlx::query!(
                    r#"
                    UPDATE normalisation_run
                    SET finished_at = ?, outcome = 'failed', failure_reason = ?
                    WHERE id = ?
                    "#,
                    finished_at,
                    reason,
                    id
                )
                .execute(&self.pool)
                .await
                .map_err(|error| store_error(&error))?;
                Ok(())
            }
        }
    }

    async fn latest_success(
        &self,
        stream: &LandingStream,
    ) -> Result<Option<NormalisationRun>, StoreError> {
        let stream_name = stream.to_string();

        let row = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   started_at AS "started_at!: String",
                   finished_at AS "finished_at!: String",
                   records_read AS "records_read: i64",
                   workouts_written AS "workouts_written: i64",
                   records_composed AS "records_composed: i64",
                   records_superseded AS "records_superseded: i64",
                   workouts_retracted AS "workouts_retracted: i64",
                   retractions_read AS "retractions_read: i64",
                   records_refused AS "records_refused: i64",
                   refusals_recorded AS "refusals_recorded: i64",
                   records_covered AS "records_covered: i64"
            FROM normalisation_run
            WHERE stream = ? AND outcome = 'succeeded'
            ORDER BY finished_at DESC
            LIMIT 1
            "#,
            stream_name
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(NormalisationRun::new(
            NormalisationRunId::try_from(row.id).map_err(|error| corrupt(&error))?,
            stream.clone(),
            FetchedAt::try_from(row.started_at.as_str()).map_err(|error| corrupt(&error))?,
            NormalisationOutcome::Succeeded {
                finished_at: FetchedAt::try_from(row.finished_at.as_str())
                    .map_err(|error| corrupt(&error))?,
                records_read: RecordCount::from(count_from_storage(row.records_read)?),
                workouts_written: WorkoutCount::from(count_from_storage(row.workouts_written)?),
                // Null on a run recorded before the column existed. Reading it
                // as the record count would make an unreconciled past run look
                // reconciled, so it reads as nothing composed and the check
                // fails honestly.
                records_composed: RecordCount::from(count_from_storage(row.records_composed)?),
                records_superseded: RecordCount::from(count_from_storage(row.records_superseded)?),
                workouts_retracted: WorkoutCount::from(count_from_storage(row.workouts_retracted)?),
                retractions_read: RecordCount::from(count_from_storage(row.retractions_read)?),
                records_refused: RecordCount::from(count_from_storage(row.records_refused)?),
                refusals_recorded: RefusalCount::from(count_from_storage(row.refusals_recorded)?),
                // Null on a run recorded before a derivation could read part of
                // raw, every one of which read all of it.
                records_covered: RecordCount::from(count_from_storage(
                    row.records_covered.or(row.records_read),
                )?),
            },
        )))
    }

    /// **The latest success's, or none**: a mark from an earlier success than
    /// the latest would name rows the latest has since replaced.
    async fn mark(&self, stream: &LandingStream) -> Result<Option<Mark>, StoreError> {
        let stream = stream.to_string();
        let row = sqlx::query!(
            r#"
            SELECT terms AS "terms: String", read_up_to AS "read_up_to: String"
            FROM normalisation_run
            WHERE stream = ? AND outcome = 'succeeded'
            ORDER BY finished_at DESC, id DESC
            LIMIT 1
            "#,
            stream
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(row.and_then(|row| {
            Some(Mark {
                terms: Terms::from(row.terms?),
                up_to: ReadUpTo::from(row.read_up_to?),
            })
        }))
    }

    async fn record_mark(&self, run: NormalisationRunId, mark: &Mark) -> Result<(), StoreError> {
        let id = normalisation_run_for_storage(run)?;
        let terms = mark.terms.as_str();
        let up_to = mark.up_to.as_str();
        sqlx::query!(
            "UPDATE normalisation_run SET terms = ?, read_up_to = ? WHERE id = ?",
            terms,
            up_to,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(())
    }
}

/// Unused today, and kept because a failure reason has to read back for § 38 to
/// mean anything: a derivation that broke must be distinguishable from one that
/// found nothing.
#[cfg(test)]
mod tests {
    use domain::normalised::NormalisationFailure;

    #[test]
    fn a_failure_reason_round_trips_through_its_stored_form() {
        for reason in [
            NormalisationFailure::StoreFailure,
            NormalisationFailure::UnmappedExercise,
            NormalisationFailure::MissingTimeZone,
        ] {
            let stored = reason.as_str();
            let read = NormalisationFailure::try_from(stored).expect("a known reason reads back");
            assert_eq!(read, reason);
        }
    }
}
