//! The normalised layer for `myfitnesspal.exports`: the weigh-ins
//! MyFitnessPal's exports derive (#242), and the reader that hands the
//! translator every export landed.
//!
//! The weigh-ins go in the source-independent `weigh_in` table beside the Body
//! Scan's and the spreadsheets', told apart by `stream`, and only this
//! stream's rows are replaced. Where each was written, and when it was typed,
//! hang off its row in `myfitnesspal_weigh_in`.

use application::{AccountReader, NormalisedEntityStore, Reached, Reading, StoreError};
use domain::{
    body::ManualWeighIn,
    landing::{
        FetchedAt, FilePath, FileProvenance, InvalidStream, LandedRecord, LandingRecord,
        LandingRecordId, LandingStream, ModifiedAt, RawPayload, SourceRecordId,
    },
    normalised::{NormalisationRunId, WorkoutCount},
};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::{
    MyFitnessPalExportLandingStore, corrupt, count_from_storage, normalisation_run_for_storage,
    store_error,
};
use crate::myfitnesspal::MyFitnessPalExports;

/// Raw, read-only, for MyFitnessPal's exports: one account holding
/// every export landed, oldest first.
#[derive(Debug, Clone)]
pub struct MyFitnessPalExportAccountReader {
    pool: SqlitePool,
    stream: LandingStream,
}

impl MyFitnessPalExportAccountReader {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(MyFitnessPalExportLandingStore::STREAM)?,
        })
    }
}

impl AccountReader for MyFitnessPalExportAccountReader {
    /// Every export. A file's identity is the digest of its bytes, so no two
    /// landed files share one and nothing is superseded.
    type Account = MyFitnessPalExports;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn accounts(
        &self,
        reading: &Reading,
    ) -> Result<Reached<MyFitnessPalExports>, StoreError> {
        let marks = super::marks::Marks::of(reading)?;
        let rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   fetched_at AS "fetched_at!: String",
                   source_record_id AS "source_record_id!: String",
                   path AS "path!: String",
                   modified_at AS "modified_at!: String",
                   payload AS "payload!: Vec<u8>"
            FROM myfitnesspal_export_landing
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
        Ok(Reached {
            up_to: super::marks::up_to(&records),
            previously: marks.map(|marks| MyFitnessPalExports::gather(marks.held(&records))),
            now: MyFitnessPalExports::gather(records),
        })
    }
}

/// Writes the weigh-ins MyFitnessPal's exports derive.
#[derive(Debug, Clone)]
pub struct SqliteMyFitnessPalStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteMyFitnessPalStore {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(MyFitnessPalExportLandingStore::STREAM)?,
        })
    }
}

impl NormalisedEntityStore for SqliteMyFitnessPalStore {
    type Entity = ManualWeighIn;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn replace(
        &self,
        run: NormalisationRunId,
        weigh_ins: Vec<ManualWeighIn>,
    ) -> Result<WorkoutCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let stream = self.stream.to_string();
        let written = weigh_ins.len();

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;
        super::canonical_weigh_in::clear(&mut tx).await?;
        sqlx::query!("DELETE FROM myfitnesspal_weigh_in")
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        sqlx::query!("DELETE FROM weigh_in WHERE stream = ?", stream)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;

        write_weigh_ins(&mut tx, run_id, &stream, &weigh_ins).await?;
        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(written))
    }

    async fn revise(
        &self,
        run: NormalisationRunId,
        removed: Vec<LandingRecordId>,
        weigh_ins: Vec<ManualWeighIn>,
    ) -> Result<WorkoutCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let stream = self.stream.to_string();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;
        let removed = super::ids_json(removed.iter().map(|id| id.as_i64()));
        let doomed = sqlx::query_scalar!(
            r#"
            SELECT weigh_in AS "weigh_in!: i64" FROM myfitnesspal_weigh_in
             WHERE landing_record_id IN (SELECT value FROM json_each(?))
            "#,
            removed
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?;
        let doomed = super::ids_json(doomed);
        // The canonical layer first, but only the days these are on (#610).
        super::canonical_weigh_in::changed_weigh_ins(&mut tx, &doomed).await?;
        for statement in [
            "DELETE FROM myfitnesspal_weigh_in WHERE weigh_in IN (SELECT value FROM json_each(?))",
            "DELETE FROM weigh_in WHERE id IN (SELECT value FROM json_each(?))",
        ] {
            sqlx::query(statement)
                .bind(&doomed)
                .execute(&mut *tx)
                .await
                .map_err(|error| store_error(&error))?;
        }
        write_weigh_ins(&mut tx, run_id, &stream, &weigh_ins).await?;
        super::canonical_weigh_in::changed_by_run(&mut tx, &stream, run_id).await?;
        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(weigh_ins.len()))
    }

    async fn count(&self) -> Result<WorkoutCount, StoreError> {
        let row = sqlx::query!(r#"SELECT count(*) AS "count!: i64" FROM myfitnesspal_weigh_in"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(count_from_storage(Some(row.count))?))
    }
}

/// MyFitnessPal's weigh-ins, each beside the row every source writes to.
async fn write_weigh_ins(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    stream: &str,
    weigh_ins: &[ManualWeighIn],
) -> Result<(), StoreError> {
    for weigh_in in weigh_ins {
        let Some(entered_at) = weigh_in.entered_at() else {
            return Err(StoreError::Corrupt {
                detail: format!(
                    "a MyFitnessPal weigh-in on {} states no time it was entered",
                    weigh_in.on()
                ),
            });
        };
        let entered_at = entered_at.to_string();
        let on_day = weigh_in.on().to_string();
        let mass = i64::try_from(weigh_in.mass().as_grams()).map_err(|error| corrupt(&error))?;
        let cell = weigh_in.written_in();
        let landed_as = cell.landed_as.as_i64();
        let sheet = cell.sheet.as_str();
        let reference = cell.cell.as_str();

        let row = sqlx::query!(
            r#"
            INSERT INTO weigh_in (stream, on_day, mass_grams, run_id)
            VALUES (?, ?, ?, ?)
            RETURNING id AS "id!: i64"
            "#,
            stream,
            on_day,
            mass,
            run_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;

        sqlx::query!(
            r#"
            INSERT INTO myfitnesspal_weigh_in (
                weigh_in, landing_record_id, sheet, cell, entered_at_utc
            )
            VALUES (?, ?, ?, ?, ?)
            "#,
            row.id,
            landed_as,
            sheet,
            reference,
            entered_at,
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    }
    Ok(())
}
