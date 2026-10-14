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
//! One adapter here and the writer's half of another: the spreadsheet reader
//! has no `append`, and [`super::spreadsheet_normalised`] writes the
//! derivation, weigh-ins and gym sessions together, through the functions
//! below. Body weight is read back from the canonical layer
//! ([`super::canonical_weigh_in`]), where copies of one day are one weigh-in.

use application::{AccountReader, StoreError};
use domain::{
    body::{LastSaved, ManualWeighIn},
    landing::{
        FetchedAt, FilePath, FileProvenance, InvalidStream, LandedRecord, LandingRecord,
        LandingRecordId, LandingStream, ModifiedAt, RawPayload, SourceRecordId,
    },
};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::{SpreadsheetFileLandingStore, corrupt, count_from_storage, store_error};
use crate::spreadsheets::Workbook;

/// Raw, read-only, for the historical spreadsheets. One account per workbook,
/// holding every copy of it that landed.
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
    /// Every copy of one workbook. A file's identity is the digest of its
    /// bytes, so no two landed files share one and nothing is superseded.
    type Account = Workbook;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn accounts(&self) -> Result<Vec<Workbook>, StoreError> {
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
        Ok(Workbook::gather(records))
    }
}

/// Every manual weigh-in, in place of the ones this stream wrote before.
///
/// Called by the spreadsheets' store inside its own transaction, beside the
/// gym sessions the same files derive.
pub(super) async fn replace_manual_weigh_ins(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    stream: &str,
    weigh_ins: &[ManualWeighIn],
) -> Result<(), StoreError> {
    super::canonical_weigh_in::clear(tx).await?;
    sqlx::query!("DELETE FROM manual_weigh_in")
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    sqlx::query!("DELETE FROM weigh_in WHERE stream = ?", stream)
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;

    for weigh_in in weigh_ins {
        let on_day = weigh_in.on().to_string();
        let mass = i64::try_from(weigh_in.mass().as_grams()).map_err(|error| corrupt(&error))?;
        let cell = weigh_in.written_in();
        let landed_as = cell.landed_as.as_i64();
        let sheet = cell.sheet.as_str();
        let reference = cell.cell.as_str();
        let (last_saved_at, last_saved_by) = match weigh_in.last_saved() {
            Some(LastSaved::ByTheDocument(at)) => (Some(at.to_string()), Some("document")),
            Some(LastSaved::ByTheFileSystem(at)) => (Some(at.to_string()), Some("file system")),
            None => (None, None),
        };

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
            INSERT INTO manual_weigh_in (
                weigh_in, landing_record_id, sheet, cell, last_saved_at_utc, last_saved_by
            )
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
            row.id,
            landed_as,
            sheet,
            reference,
            last_saved_at,
            last_saved_by,
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    }
    Ok(())
}

/// How many manual weigh-ins the spreadsheets derived.
pub(super) async fn count_manual_weigh_ins(pool: &SqlitePool) -> Result<usize, StoreError> {
    let row = sqlx::query!(r#"SELECT COUNT(*) AS "total!: i64" FROM manual_weigh_in"#)
        .fetch_one(pool)
        .await
        .map_err(|error| store_error(&error))?;
    count_from_storage(Some(row.total))
}
