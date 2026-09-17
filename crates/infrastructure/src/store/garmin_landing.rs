//! Raw landing for `garmin.hrv`.
//!
//! **A copy of [`super::withings_landing`] rather than a generalisation of it**,
//! for the reason that file gives: a table name inside `sqlx::query!` cannot
//! be a parameter.

use application::{LandingStore, StoreError};
use domain::landing::{
    InvalidStream, LandingRecord, LandingStream, PayloadDigest, Provenance, RecordCount, RunId,
    SourceRecordId,
};
use sqlx::SqlitePool;

use super::{count_from_storage, digest_from_row, run_id_for_storage, store_error};

/// The landing table for Garmin's nightly HRV answers.
#[derive(Debug, Clone)]
pub struct GarminHrvLandingStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl GarminHrvLandingStore {
    /// Which stream this table holds.
    ///
    /// Declared beside the queries that name `garmin_hrv_landing`, for the
    /// reason given on [`super::landing::HevyWorkoutLandingStore::STREAM`]:
    /// this is the one link no type can check.
    pub const STREAM: &'static str = "garmin.hrv";

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

impl LandingStore for GarminHrvLandingStore {
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
            FROM garmin_hrv_landing
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
            FROM garmin_hrv_landing
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

            // Exhaustive, as on the other landing tables: a second
            // `Provenance` variant must fail to compile here.
            let Provenance::Event(event) = record.provenance();

            let endpoint = event.endpoint().as_str();
            let fetched_at = record.fetched_at().to_string();
            let source_record_id = record.source_record_id().as_str();
            let event_kind = event.kind().as_str();
            let event_time = event.occurred_at().map(|at| at.to_string());
            let payload = record.payload().as_bytes();
            let digest = record.digest();
            let digest = digest.as_bytes().as_slice();
            let revision = record.revision();
            let revision = revision.as_bytes().as_slice();

            sqlx::query!(
                r#"
                INSERT INTO garmin_hrv_landing (
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
        let row = sqlx::query!(r#"SELECT COUNT(*) AS "total!: i64" FROM garmin_hrv_landing"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;

        Ok(RecordCount::from(count_from_storage(Some(row.total))?))
    }
}

/// How much raw this stream holds, for the derivation's status.
///
/// A second, narrower answer to a question [`application::LandingStore`] can
/// also answer, and separate because reporting how far behind a derivation is
/// needs the count and must not be handed an `append`.
impl application::RawExtent for GarminHrvLandingStore {
    async fn records(&self) -> Result<RecordCount, StoreError> {
        application::LandingStore::count(self).await
    }
}

#[cfg(test)]
mod tests {
    use super::{GarminHrvLandingStore, LandingStream};

    /// The constant every run's identity is derived from must name a stream.
    #[test]
    fn the_declared_stream_is_a_stream() {
        let stream = LandingStream::try_from(GarminHrvLandingStore::STREAM).expect("a stream name");
        assert_eq!(stream.to_string(), "garmin.hrv");
    }
}
