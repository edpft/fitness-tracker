//! The `weigh_in` table: a body mass from any source (#273).
//!
//! **One table, every source.** Body mass is source-independent (§ 6), so a
//! Body Scan step and a figure the operator typed into a spreadsheet are rows
//! of the same shape: a day or an instant, a mass, and the source. Each
//! source's derivation replaces only its own rows, which the `stream` column
//! tells apart. What a source recorded beyond the mass hangs off its row: a
//! Body Scan's composition in `body_scan_weigh_in`, and a spreadsheet's cell in
//! `manual_weigh_in`.
//!
//! Three adapters here, for the reason [`super::withings_normalised`] gives for
//! two: the spreadsheet reader has no `append`, the manual store writes the
//! derivation, and the history reads the mass back for relative strength.

use application::{AccountReader, NormalisedEntityStore, StoreError, WeighInHistory};
use domain::{
    analytical::Weighed,
    body::ManualWeighIn,
    landing::{
        FetchedAt, FilePath, FileProvenance, InvalidStream, LandedRecord, LandingRecord,
        LandingRecordId, LandingStream, ModifiedAt, RawPayload, SourceRecordId,
    },
    measure::Kg,
    normalised::{NormalisationRunId, OperatorZone, StartedAt, WorkoutCount},
};
use sqlx::SqlitePool;

use super::{
    SpreadsheetFileLandingStore, corrupt, count_from_storage, normalisation_run_for_storage,
    store_error,
};

/// Raw, read-only, for the historical spreadsheets. One account per file.
#[derive(Debug, Clone)]
pub struct SpreadsheetFileAccountReader {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SpreadsheetFileAccountReader {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(SpreadsheetFileLandingStore::STREAM)?,
        })
    }
}

impl AccountReader for SpreadsheetFileAccountReader {
    /// A file on its own. Its identity is the digest of its bytes, so no two
    /// landed files share one and nothing is superseded.
    type Account = LandedRecord;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn accounts(&self) -> Result<Vec<LandedRecord>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   fetched_at AS "fetched_at!: String",
                   source_record_id AS "source_record_id!: String",
                   path AS "path!: String",
                   modified_at AS "modified_at!: String",
                   payload AS "payload!: Vec<u8>"
            FROM spreadsheet_file_landing
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        rows.into_iter()
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
            .collect()
    }
}

/// The normalised layer for manual weigh-ins.
#[derive(Debug, Clone)]
pub struct SqliteManualWeighInStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteManualWeighInStore {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(SpreadsheetFileLandingStore::STREAM)?,
        })
    }
}

impl NormalisedEntityStore for SqliteManualWeighInStore {
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
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        sqlx::query!("DELETE FROM manual_weigh_in")
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        sqlx::query!("DELETE FROM weigh_in WHERE stream = ?", stream)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;

        let written = weigh_ins.len();
        for weigh_in in weigh_ins {
            let on_day = weigh_in.on().to_string();
            let mass =
                i64::try_from(weigh_in.mass().as_grams()).map_err(|error| corrupt(&error))?;
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
            .fetch_one(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;

            sqlx::query!(
                r#"
                INSERT INTO manual_weigh_in (weigh_in, landing_record_id, sheet, cell)
                VALUES (?, ?, ?, ?)
                "#,
                row.id,
                landed_as,
                sheet,
                reference,
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(written))
    }

    async fn count(&self) -> Result<WorkoutCount, StoreError> {
        let row = sqlx::query!(r#"SELECT COUNT(*) AS "total!: i64" FROM manual_weigh_in"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;
        count_from_storage(Some(row.total)).map(WorkoutCount::from)
    }
}

/// Weigh-ins, read back for relative strength.
///
/// **Only those with an instant.** Relative strength reads a session against
/// the weigh-in closest to its start on the same day, and a manual weigh-in has
/// a day and no time. More to the point, the same day sits in up to six
/// spreadsheets, and choosing between the copies is the canonical layer's work,
/// which does not exist yet. Until it does, only the Body Scan's weigh-ins are
/// read here, as they were before.
#[derive(Debug, Clone)]
pub struct SqliteWeighInHistory {
    pool: SqlitePool,
}

impl SqliteWeighInHistory {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl WeighInHistory for SqliteWeighInHistory {
    async fn weigh_ins(&self) -> Result<Vec<Weighed>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT measured_at_utc AS "measured_at!: String",
                   zone AS "zone!: String",
                   mass_grams AS "mass_grams!: i64"
            FROM weigh_in
            WHERE measured_at_utc IS NOT NULL
            ORDER BY measured_at_utc ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        rows.into_iter()
            .map(|row| {
                let instant = row
                    .measured_at
                    .parse::<jiff::Timestamp>()
                    .map_err(|error| corrupt(&error))?;
                let zone = OperatorZone::try_from(row.zone).map_err(|error| corrupt(&error))?;
                let grams = u64::try_from(row.mass_grams).map_err(|error| corrupt(&error))?;
                Ok(Weighed {
                    at: StartedAt::new(instant, zone),
                    mass: Kg::from_grams(grams),
                })
            })
            .collect()
    }
}
