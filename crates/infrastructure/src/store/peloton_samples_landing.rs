//! Raw landing for `peloton.workout_samples`.
//!
//! **A copy of [`super::landing`] rather than a generalisation of it**, and
//! deliberately. The table name is a string inside `sqlx::query!` and cannot be
//! a parameter, so "one store, two tables" would mean building the SQL at
//! runtime and losing the compile-time checking that makes these queries safe.
//! What the two share is the shape of a landing record, which is § II.1's and
//! already lives in `domain`.

use application::{LandingStore, StoreError};
use domain::landing::{
    InvalidStream, LandingRecord, LandingStream, PayloadDigest, Provenance, RecordCount, RunId,
    SourceRecordId,
};
use sqlx::SqlitePool;

use super::{count_from_storage, digest_from_row, run_id_for_storage, store_error};

/// The landing table for Peloton performance graphs.
///
/// One graph per workout, filed under the same identifier
/// `peloton_workout_landing` files the workout under — which is what makes the
/// two joinable without either depending on the other.
#[derive(Debug, Clone)]
pub struct PelotonWorkoutSampleLandingStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl PelotonWorkoutSampleLandingStore {
    /// Which stream this table holds.
    ///
    /// Declared beside the queries that name `peloton_workout_sample_landing`, for the
    /// reason given on [`super::landing::HevyWorkoutLandingStore::STREAM`]:
    /// this is the one link no type can check.
    pub const STREAM: &'static str = "peloton.workout_samples";

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

impl LandingStore for PelotonWorkoutSampleLandingStore {
    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn latest_digest(
        &self,
        id: &SourceRecordId,
    ) -> Result<Option<PayloadDigest>, StoreError> {
        let id = id.as_str();
        let row = sqlx::query!(
            r#"
            SELECT COALESCE(revision_digest, payload_digest) AS "payload_digest!: Vec<u8>"
            FROM peloton_workout_sample_landing
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
        let next = sqlx::query!(
            r#"
            SELECT COALESCE(MAX(serve_ordinal), -1) AS "highest!: i64"
            FROM peloton_workout_sample_landing
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

            // Exhaustive for the same reason it is on Hevy's table: these
            // columns are an HTTP feed's, and a second `Provenance` variant
            // must fail to compile here rather than land with blanks.
            let Provenance::Event(event) = record.provenance();

            let endpoint = event.endpoint().as_str();
            let fetched_at = record.fetched_at().to_string();
            let source_record_id = record.source_record_id().as_str();
            let event_kind = event.kind().as_str();
            let event_time = event.occurred_at().map(|at| at.to_string());
            let payload = record.payload().as_bytes();
            let digest = record.digest();
            let digest = digest.as_bytes().as_slice();
            // Expected to equal the digest here: a graph is the operator's own
            // performance, with none of the class's public counters in it. The
            // column exists so that a source which turns out to churn can be
            // handled without another migration.
            let revision = record.revision();
            let revision = revision.as_bytes().as_slice();

            sqlx::query!(
                r#"
                INSERT INTO peloton_workout_sample_landing (
                    endpoint, fetched_at, source_record_id, event_kind,
                    event_time, payload, payload_digest, revision_digest,
                    run_id, serve_ordinal
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                endpoint,
                fetched_at,
                source_record_id,
                event_kind,
                event_time,
                payload,
                digest,
                revision,
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
        let row =
            sqlx::query!(r#"SELECT COUNT(*) AS "total!: i64" FROM peloton_workout_sample_landing"#)
                .fetch_one(&self.pool)
                .await
                .map_err(|error| store_error(&error))?;

        Ok(RecordCount::from(count_from_storage(Some(row.total))?))
    }
}

#[cfg(test)]
mod tests {
    use super::{LandingStream, PelotonWorkoutSampleLandingStore};

    /// The constant every run's identity is derived from must name a stream.
    #[test]
    fn the_declared_stream_is_a_stream() {
        let stream = LandingStream::try_from(PelotonWorkoutSampleLandingStore::STREAM)
            .expect("a stream name");
        assert_eq!(stream.to_string(), "peloton.workout_samples");
    }
}
