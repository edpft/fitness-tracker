//! Raw landing for `btwb.exports`.
//!
//! **A copy of [`super::spreadsheet_file_landing`] rather than a
//! generalisation of it**: a table name inside `sqlx::query!` cannot be a
//! parameter, and the two are separate tables because they are separate
//! sources (§ II.3).

use application::{LandingStore, StoreError};
use domain::landing::{
    InvalidStream, LandingRecord, LandingStream, PayloadDigest, RecordCount, RunId, SourceRecordId,
};
use sqlx::SqlitePool;

use super::{count_from_storage, digest_from_row, run_id_for_storage, store_error};

/// The landing table for Beyond The White Board's export: one record per file
/// it sent, holding its bytes exactly as they were.
///
/// **A file's identity is the digest of its bytes**, which the adapter sets as
/// its source record id. So a byte-identical copy is the same record served
/// again and lands nothing, and two versions that differ land as two.
#[derive(Debug, Clone)]
pub struct BtwbExportLandingStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl BtwbExportLandingStore {
    /// Which stream this table holds.
    ///
    /// Declared beside the queries that name `btwb_export_landing`, for the
    /// reason given on [`super::landing::HevyWorkoutLandingStore::STREAM`]:
    /// this is the one link no type can check.
    pub const STREAM: &'static str = "btwb.exports";

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

impl LandingStore for BtwbExportLandingStore {
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
            FROM btwb_export_landing
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
            FROM btwb_export_landing
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

            let Some(file) = record.provenance().as_file() else {
                return Err(StoreError::Corrupt {
                    detail: format!(
                        "{} was served by a feed, where a folder was expected",
                        record.provenance()
                    ),
                });
            };

            let fetched_at = record.fetched_at().to_string();
            let source_record_id = record.source_record_id().as_str();
            let path = file.path().as_str();
            let modified_at = file.modified_at().to_string();
            let payload = record.payload().as_bytes();
            let digest = record.digest();
            let digest = digest.as_bytes().as_slice();
            let revision = record.revision();
            let revision = revision.as_bytes().as_slice();

            sqlx::query!(
                r#"
                INSERT INTO btwb_export_landing (
                    fetched_at, source_record_id, path, modified_at,
                    payload, payload_digest, revision_digest,
                    run_id, serve_ordinal
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                fetched_at,
                source_record_id,
                path,
                modified_at,
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
        let row = sqlx::query!(r#"SELECT COUNT(*) AS "total!: i64" FROM btwb_export_landing"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;

        Ok(RecordCount::from(count_from_storage(Some(row.total))?))
    }
}

#[cfg(test)]
mod tests {
    use super::{BtwbExportLandingStore, LandingStream};

    /// The constant every run's identity is derived from must name a stream.
    #[test]
    fn the_declared_stream_is_a_stream() {
        let stream =
            LandingStream::try_from(BtwbExportLandingStore::STREAM).expect("a stream name");
        assert_eq!(stream.to_string(), "btwb.exports");
    }
}
