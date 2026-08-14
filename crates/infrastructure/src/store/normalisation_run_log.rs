//! What happened on each derivation.
//!
//! A parallel of [`super::run_log`] rather than a widening of it. Extraction
//! and derivation fail for different reasons and report different numbers — one
//! counts what a source served, the other counts what our own translation made
//! of it — and one table carrying both would have half its columns empty in
//! either direction.

use application::{NormalisationRunLog, StoreError};
use domain::{
    gym::{
        NormalisationOutcome, NormalisationRun, NormalisationRunId,
        RefusalCount, WorkoutCount,
    },
    landing::{FetchedAt, LandingStream, RecordCount},
};
use sqlx::SqlitePool;

use super::{corrupt, normalisation_run_for_storage, store_error};

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

/// A count that came out of the store, which is `i64` there and never negative
/// here.
fn count_from_row(value: Option<i64>) -> u64 {
    value.and_then(|value| u64::try_from(value).ok()).unwrap_or(0)
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
                workouts_withdrawn,
                retractions_applied,
                records_refused,
                refusals_recorded,
            } => {
                let finished_at = finished_at.to_string();
                let records_read = i64::try_from(records_read.as_u64()).unwrap_or(i64::MAX);
                let workouts_written = i64::try_from(workouts_written.as_u64()).unwrap_or(i64::MAX);
                let workouts_withdrawn =
                    i64::try_from(workouts_withdrawn.as_u64()).unwrap_or(i64::MAX);
                let retractions_applied =
                    i64::try_from(retractions_applied.as_u64()).unwrap_or(i64::MAX);
                let records_refused = i64::try_from(records_refused.as_u64()).unwrap_or(i64::MAX);
                let refusals_recorded =
                    i64::try_from(refusals_recorded.as_u64()).unwrap_or(i64::MAX);

                sqlx::query!(
                    r#"
                    UPDATE normalisation_run
                    SET finished_at = ?, outcome = 'succeeded',
                        records_read = ?, workouts_written = ?, workouts_withdrawn = ?,
                        retractions_applied = ?, records_refused = ?, refusals_recorded = ?
                    WHERE id = ?
                    "#,
                    finished_at,
                    records_read,
                    workouts_written,
                    workouts_withdrawn,
                    retractions_applied,
                    records_refused,
                    refusals_recorded,
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
                   workouts_withdrawn AS "workouts_withdrawn: i64",
                   retractions_applied AS "retractions_applied: i64",
                   records_refused AS "records_refused: i64",
                   refusals_recorded AS "refusals_recorded: i64"
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
                records_read: RecordCount::from(count_from_row(row.records_read)),
                workouts_written: WorkoutCount::from(count_from_row(row.workouts_written)),
                workouts_withdrawn: WorkoutCount::from(count_from_row(row.workouts_withdrawn)),
                retractions_applied: RecordCount::from(count_from_row(row.retractions_applied)),
                records_refused: RecordCount::from(count_from_row(row.records_refused)),
                refusals_recorded: RefusalCount::from(count_from_row(row.refusals_recorded)),
            },
        )))
    }
}

/// Unused today, and kept because a failure reason has to read back for § 38 to
/// mean anything: a derivation that broke must be distinguishable from one that
/// found nothing.
#[cfg(test)]
mod tests {
    use domain::gym::NormalisationFailure;

    #[test]
    fn a_failure_reason_round_trips_through_its_stored_form() {
        for reason in [
            NormalisationFailure::StoreFailure,
            NormalisationFailure::UnmappedExercise,
            NormalisationFailure::MissingZone,
        ] {
            let stored = reason.as_str();
            let read = NormalisationFailure::try_from(stored).expect("a known reason reads back");
            assert_eq!(read, reason);
        }
    }
}
