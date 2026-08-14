//! What happened on each invocation.
//!
//! The table that makes a silently broken extraction visible: a run that found
//! nothing new, a run that found nothing at all, and a run that failed are
//! three different rows rather than three absences.

use application::{ExtractionRunLog, StoreError};
use domain::landing::{
    EventCount, ExtractionRun, FetchedAt, LandingStream, RecordCount, RunId, RunOutcome,
};
use sqlx::SqlitePool;

use super::{run_id_for_storage, run_id_from_row, store_error};

#[derive(Debug, Clone)]
pub struct SqliteExtractionRunLog {
    pool: SqlitePool,
}

impl SqliteExtractionRunLog {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn corrupt(detail: impl Into<String>) -> StoreError {
    StoreError::Corrupt {
        detail: detail.into(),
    }
}

impl ExtractionRunLog for SqliteExtractionRunLog {
    async fn begin(&self, stream: &LandingStream, at: FetchedAt) -> Result<RunId, StoreError> {
        let stream = stream.to_string();
        let started_at = at.to_string();

        let row = sqlx::query!(
            r#"
            INSERT INTO extraction_run (stream, started_at)
            VALUES (?, ?)
            RETURNING id AS "id!: i64"
            "#,
            stream,
            started_at
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        run_id_from_row(row.id)
    }

    async fn finish(&self, run: RunId, outcome: RunOutcome) -> Result<(), StoreError> {
        let id = run_id_for_storage(run)?;

        let (finished_at, label, events_seen, records_landed, reason) = match &outcome {
            // Nothing to write: the row is already in flight, which is exactly
            // "no outcome yet".
            RunOutcome::InFlight => return Ok(()),
            RunOutcome::Succeeded {
                finished_at,
                events_seen,
                records_landed,
            } => (
                finished_at.to_string(),
                "succeeded",
                Some(i64::try_from(events_seen.as_usize()).unwrap_or(i64::MAX)),
                Some(i64::try_from(records_landed.as_usize()).unwrap_or(i64::MAX)),
                None,
            ),
            RunOutcome::Failed {
                finished_at,
                reason,
            } => (
                finished_at.to_string(),
                "failed",
                Some(0),
                Some(0),
                Some(reason.as_str()),
            ),
        };

        sqlx::query!(
            r#"
            UPDATE extraction_run
            SET finished_at = ?, outcome = ?, events_seen = ?,
                records_landed = ?, failure_reason = ?
            WHERE id = ?
            "#,
            finished_at,
            label,
            events_seen,
            records_landed,
            reason,
            id
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        Ok(())
    }

    async fn latest_success(
        &self,
        stream: &LandingStream,
    ) -> Result<Option<ExtractionRun>, StoreError> {
        let key = stream.to_string();

        let row = sqlx::query!(
            r#"
            SELECT id            AS "id!: i64",
                   started_at    AS "started_at!: String",
                   finished_at   AS "finished_at!: String",
                   events_seen   AS "events_seen!: i64",
                   records_landed AS "records_landed!: i64"
            FROM extraction_run
            WHERE stream = ? AND outcome = 'succeeded'
            ORDER BY finished_at DESC
            LIMIT 1
            "#,
            key
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let Some(row) = row else { return Ok(None) };

        let started_at = FetchedAt::try_from(row.started_at.as_str())
            .map_err(|error| corrupt(error.to_string()))?;
        let finished_at = FetchedAt::try_from(row.finished_at.as_str())
            .map_err(|error| corrupt(error.to_string()))?;

        Ok(Some(ExtractionRun::new(
            run_id_from_row(row.id)?,
            stream.clone(),
            started_at,
            RunOutcome::Succeeded {
                finished_at,
                events_seen: EventCount::from(super::count_from_storage(Some(row.events_seen))?),
                records_landed: RecordCount::from(super::count_from_storage(Some(
                    row.records_landed,
                ))?),
            },
        )))
    }
}
