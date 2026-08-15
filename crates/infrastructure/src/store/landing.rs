//! Raw landing for `hevy.workouts`.

use application::{LandingStore, StoreError};
use domain::landing::{
    InvalidStream, LandingRecord, LandingStream, PayloadDigest, Provenance, RecordCount, RunId,
    SourceRecordId,
};
use sqlx::SqlitePool;

use super::{count_from_storage, run_id_for_storage, store_error};

/// The landing table for Hevy workouts.
///
/// Bound to one table, so there is no stream argument to pass wrongly. A store
/// for another stream is a different type with a different table, not this one
/// pointed elsewhere.
#[derive(Debug, Clone)]
pub struct HevyWorkoutLandingStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl HevyWorkoutLandingStore {
    /// Which stream this table holds.
    ///
    /// Declared here, beside the queries that name `hevy_workout_landing`,
    /// because this is the one link in the chain no type can check: the table
    /// is a string inside `sqlx::query!`. Everything downstream — what a run
    /// locks, what it logs, which resumption point it advances, how each
    /// record is tagged — derives from this constant rather than being passed
    /// in beside it, so this is the only place the pairing can be got wrong.
    pub const STREAM: &'static str = "hevy.workouts";

    /// # Errors
    ///
    /// [`InvalidStream`] if [`Self::STREAM`] is not a stream name. Pinned by a
    /// test below, so it is a mistake in this file rather than in a call.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(Self::STREAM)?,
        })
    }
}

/// A stored digest is 32 bytes. Anything else means the file holds something
/// this program did not write.
fn digest_from_row(bytes: &[u8]) -> Result<PayloadDigest, StoreError> {
    PayloadDigest::try_from(bytes).map_err(|error| StoreError::Corrupt {
        detail: error.to_string(),
    })
}

impl LandingStore for HevyWorkoutLandingStore {
    fn stream(&self) -> &LandingStream {
        &self.stream
    }

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

        row.map(|row| digest_from_row(&row.payload_digest))
            .transpose()
    }

    async fn append(
        &self,
        run: RunId,
        records: Vec<LandingRecord>,
    ) -> Result<RecordCount, StoreError> {
        if records.is_empty() {
            return Ok(RecordCount::default());
        }

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        let run_id = run_id_for_storage(run)?;
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
        let mut landed = 0_usize;

        for record in &records {
            ordinal = ordinal.saturating_add(1);

            // This table is the landing table for an HTTP events feed, and its
            // columns say so. A record that reached us some other way belongs
            // in a table shaped for that, not in this one with three columns
            // left blank — so the destructuring is deliberately exhaustive,
            // and a second variant will fail to compile here.
            let Provenance::Event(event) = record.provenance();

            let endpoint = event.endpoint().as_str();
            let fetched_at = record.fetched_at().to_string();
            let source_record_id = record.source_record_id().as_str();
            let event_kind = event.kind().as_str();
            let event_time = event.occurred_at().map(|at| at.to_string());
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

        Ok(RecordCount::from(landed))
    }

    async fn count(&self) -> Result<RecordCount, StoreError> {
        let row = sqlx::query!(r#"SELECT COUNT(*) AS "total!: i64" FROM hevy_workout_landing"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;

        Ok(RecordCount::from(count_from_storage(Some(row.total))?))
    }
}

#[cfg(test)]
mod tests {
    use super::{HevyWorkoutLandingStore, LandingStream};

    /// The constant every run's identity is derived from must name a stream.
    #[test]
    fn the_declared_stream_is_a_stream() {
        let stream =
            LandingStream::try_from(HevyWorkoutLandingStore::STREAM).expect("a stream name");
        assert_eq!(stream.to_string(), "hevy.workouts");
    }
}
