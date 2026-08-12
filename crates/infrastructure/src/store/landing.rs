//! Raw landing for `hevy.workouts`.

use application::{LandingStore, StoreError};
use domain::landing::{LandingRecord, PayloadDigest, RecordCount, RunId, SourceRecordId};
use sqlx::SqlitePool;

use super::store_error;

/// The landing table for Hevy workouts.
///
/// Bound to one table, so there is no stream argument to pass wrongly. A store
/// for another stream is a different type with a different table, not this one
/// pointed elsewhere.
#[derive(Debug, Clone)]
pub struct HevyWorkoutLandingStore {
    pool: SqlitePool,
}

impl HevyWorkoutLandingStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// A stored digest is 32 bytes. Anything else means the file holds something
/// this program did not write.
fn digest_from_row(bytes: Vec<u8>) -> Result<PayloadDigest, StoreError> {
    let width = bytes.len();
    <[u8; 32]>::try_from(bytes)
        .map(PayloadDigest::from_storage)
        .map_err(|_| StoreError::Corrupt {
            detail: format!("a payload digest should be 32 bytes, found {width}"),
        })
}

impl LandingStore for HevyWorkoutLandingStore {
    async fn latest_digest(
        &self,
        id: &SourceRecordId,
    ) -> Result<Option<PayloadDigest>, StoreError> {
        let id = id.as_str();
        // Most recent, not any. A workout edited to X, then Y, then back to X
        // is the source serving three payloads, and the third differs from the
        // second even though it matches the first.
        let row = sqlx::query!(
            r#"
            SELECT payload_digest AS "payload_digest!: Vec<u8>"
            FROM hevy_workout_landing
            WHERE source_record_id = ?
            ORDER BY id DESC
            LIMIT 1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        row.map(|row| digest_from_row(row.payload_digest))
            .transpose()
    }

    async fn append(
        &self,
        run: RunId,
        records: Vec<LandingRecord>,
    ) -> Result<RecordCount, StoreError> {
        if records.is_empty() {
            return Ok(RecordCount::new(0));
        }

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        let run_id = run.as_i64();
        // The serve ordinal continues across pages within a run, so the order
        // the source served events in survives a walk that commits per page.
        let next = sqlx::query!(
            r#"
            SELECT COALESCE(MAX(serve_ordinal), -1) AS "highest!: i64"
            FROM hevy_workout_landing
            WHERE run_id = ?
            "#,
            run_id
        )
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| store_error(&error))?;

        let mut ordinal = next.highest;
        let mut landed = 0_u64;

        for record in &records {
            ordinal = ordinal.saturating_add(1);

            let endpoint = record.endpoint().as_str();
            let fetched_at = record.fetched_at().to_string();
            let source_record_id = record.source_record_id().as_str();
            let event_kind = record.event_kind().as_source_str();
            let event_time = record.event_time().map(|at| at.to_string());
            let payload = record.payload().as_bytes();
            let digest = record.digest();
            let digest = digest.as_bytes().as_slice();

            sqlx::query!(
                r#"
                INSERT INTO hevy_workout_landing (
                    endpoint, fetched_at, source_record_id, event_kind,
                    event_time, payload, payload_digest, run_id, serve_ordinal
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                endpoint,
                fetched_at,
                source_record_id,
                event_kind,
                event_time,
                payload,
                digest,
                run_id,
                ordinal
            )
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error(&error))?;

            landed = landed.saturating_add(1);
        }

        transaction
            .commit()
            .await
            .map_err(|error| store_error(&error))?;

        Ok(RecordCount::new(landed))
    }

    async fn count(&self) -> Result<RecordCount, StoreError> {
        let row = sqlx::query!(r#"SELECT COUNT(*) AS "total!: i64" FROM hevy_workout_landing"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;

        Ok(RecordCount::new(row.total.unsigned_abs()))
    }
}
