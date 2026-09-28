//! The normalised layer for `btwb.exports`: the gym sessions Beyond The White
//! Board's exports derive (#285), and the reader that hands the translator
//! every export landed.
//!
//! The sessions go in the source-independent gym tables beside Hevy's and the
//! spreadsheets', told apart by `stream`, and only this stream's rows are
//! replaced.

use application::{AccountReader, NormalisedEntityStore, StoreError};
use domain::{
    gym::ManualGymSession,
    landing::{
        FetchedAt, FilePath, FileProvenance, InvalidStream, LandedRecord, LandingRecord,
        LandingRecordId, LandingStream, ModifiedAt, RawPayload, SourceRecordId,
    },
    normalised::{NormalisationRunId, WorkoutCount},
};
use sqlx::SqlitePool;

use super::{
    BtwbExportLandingStore, corrupt, count_from_storage, manual_session::write_session,
    normalisation_run_for_storage, normalised::clear_stream, store_error,
};
use crate::btwb::Exports;

/// Raw, read-only, for Beyond The White Board's exports: one account holding
/// every export landed, oldest first.
#[derive(Debug, Clone)]
pub struct BtwbExportAccountReader {
    pool: SqlitePool,
    stream: LandingStream,
}

impl BtwbExportAccountReader {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(BtwbExportLandingStore::STREAM)?,
        })
    }
}

impl AccountReader for BtwbExportAccountReader {
    /// Every export. A file's identity is the digest of its bytes, so no two
    /// landed files share one and nothing is superseded.
    type Account = Exports;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn accounts(&self) -> Result<Vec<Exports>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   fetched_at AS "fetched_at!: String",
                   source_record_id AS "source_record_id!: String",
                   path AS "path!: String",
                   modified_at AS "modified_at!: String",
                   payload AS "payload!: Vec<u8>"
            FROM btwb_export_landing
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let records = rows
            .into_iter()
            .map(|row| {
                let provenance = FileProvenance::new(
                    FilePath::try_from(row.path).map_err(|error| corrupt(&error))?,
                    ModifiedAt::try_from(row.modified_at.as_str())
                        .map_err(|error| corrupt(&error))?,
                );
                let record = LandingRecord::land(
                    self.stream.clone(),
                    FetchedAt::try_from(row.fetched_at.as_str())
                        .map_err(|error| corrupt(&error))?,
                    SourceRecordId::try_from(row.source_record_id.as_str())
                        .map_err(|error| corrupt(&error))?,
                    provenance.into(),
                    RawPayload::try_from(row.payload).map_err(|error| corrupt(&error))?,
                );
                Ok(LandedRecord::new(
                    LandingRecordId::try_from(row.id).map_err(|error| corrupt(&error))?,
                    record,
                ))
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        Ok(Exports::gather(records))
    }
}

/// Writes the sessions Beyond The White Board's exports derive.
#[derive(Debug, Clone)]
pub struct SqliteBtwbStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteBtwbStore {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(BtwbExportLandingStore::STREAM)?,
        })
    }
}

impl NormalisedEntityStore for SqliteBtwbStore {
    type Entity = ManualGymSession;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn replace(
        &self,
        run: NormalisationRunId,
        sessions: Vec<ManualGymSession>,
    ) -> Result<WorkoutCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let stream = self.stream.to_string();
        let written = sessions.len();

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;
        clear_stream(&mut tx, &stream).await?;
        for session in &sessions {
            write_session(&mut tx, run_id, &stream, session).await?;
        }
        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(written))
    }

    async fn count(&self) -> Result<WorkoutCount, StoreError> {
        let stream = self.stream.to_string();
        let row = sqlx::query!(
            r#"SELECT count(*) AS "count!: i64" FROM gym_session WHERE stream = ?"#,
            stream
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(count_from_storage(Some(row.count))?))
    }
}
