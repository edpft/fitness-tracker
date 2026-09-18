//! The normalised layer for `garmin.hrv`, and the nights raw is derived from.
//!
//! Two adapters, for the reason [`super::withings_normalised`] has them: the
//! account reader has no `append`, so a derivation holding one could not write
//! to raw if it tried, and the store writes the derivation.
//!
//! **Which record is the night that stands is not decided here.** That is Garmin
//! knowledge and lives in [`crate::garmin::account`]; this reads rows.
//!
//! **`morning_of` is stored, though it is derived** from the window's end
//! (§ 5 permits it: § II fixes what a derivation is a function of, not how it is
//! kept, and `replace` re-derives the whole layer every run). Recovering it in
//! SQL would mean doing zone arithmetic in SQL, and the morning is how a night
//! is asked for.

use application::{AccountReader, NormalisedEntityStore, StoreError};
use domain::{
    body::{HrvReading, HrvStatus, OvernightHrv},
    landing::{
        Endpoint, EventKind, EventProvenance, EventTime, FetchedAt, InvalidStream, LandedRecord,
        LandingRecord, LandingRecordId, LandingStream, Provenance, RawPayload, SourceRecordId,
    },
    normalised::{NormalisationRunId, WorkoutCount},
};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::garmin::{NightAccount, nights};

use super::{
    GarminHrvLandingStore, corrupt, count_from_storage, normalisation_run_for_storage, store_error,
};

/// Raw, read-only, for Garmin's nights.
#[derive(Debug, Clone)]
pub struct GarminHrvAccountReader {
    pool: SqlitePool,
    stream: LandingStream,
}

impl GarminHrvAccountReader {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(GarminHrvLandingStore::STREAM)?,
        })
    }
}

impl AccountReader for GarminHrvAccountReader {
    type Account = NightAccount;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn accounts(&self) -> Result<Vec<NightAccount>, StoreError> {
        // Oldest first, by the store's own sequence — which is the order the
        // source served them, because raw is append-only.
        let rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   endpoint AS "endpoint!: String",
                   fetched_at AS "fetched_at!: String",
                   source_record_id AS "source_record_id!: String",
                   event_kind AS "event_kind!: String",
                   event_time AS "event_time: String",
                   payload AS "payload!: Vec<u8>"
            FROM garmin_hrv_landing
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let occurred_at = row
                .event_time
                .as_deref()
                .map(EventTime::try_from)
                .transpose()
                .map_err(|error| corrupt(&error))?;
            let provenance = EventProvenance::new(
                Endpoint::try_from(row.endpoint.as_str()).map_err(|error| corrupt(&error))?,
                EventKind::try_from(row.event_kind.as_str()).map_err(|error| corrupt(&error))?,
                occurred_at,
            );
            let record = LandingRecord::land(
                self.stream.clone(),
                FetchedAt::try_from(row.fetched_at.as_str()).map_err(|error| corrupt(&error))?,
                SourceRecordId::try_from(row.source_record_id.as_str())
                    .map_err(|error| corrupt(&error))?,
                provenance.into(),
                RawPayload::try_from(row.payload).map_err(|error| corrupt(&error))?,
            );
            records.push(LandedRecord::new(
                LandingRecordId::try_from(row.id).map_err(|error| corrupt(&error))?,
                record,
            ));
        }

        Ok(nights(records))
    }
}

/// The normalised layer for overnight HRV.
#[derive(Debug, Clone)]
pub struct SqliteOvernightHrvStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteOvernightHrvStore {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(GarminHrvLandingStore::STREAM)?,
        })
    }
}

/// A variability figure on its way into the store.
fn milliseconds(value: u32) -> i64 {
    i64::from(value)
}

impl NormalisedEntityStore for SqliteOvernightHrvStore {
    type Entity = OvernightHrv;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn replace(
        &self,
        run: NormalisationRunId,
        nights: Vec<OvernightHrv>,
    ) -> Result<WorkoutCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        for statement in [
            "DELETE FROM overnight_hrv_reading",
            "DELETE FROM overnight_hrv",
        ] {
            sqlx::query(statement)
                .execute(&mut *tx)
                .await
                .map_err(|error| store_error(&error))?;
        }

        let written = nights.len();
        for night in nights {
            write_night(&mut tx, run_id, &night).await?;
        }

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(written))
    }

    async fn count(&self) -> Result<WorkoutCount, StoreError> {
        let row = sqlx::query!(r#"SELECT COUNT(*) AS "total!: i64" FROM overnight_hrv"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;
        count_from_storage(Some(row.total)).map(WorkoutCount::from)
    }
}

async fn write_night(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    night: &OvernightHrv,
) -> Result<(), StoreError> {
    let id = night.landed_as().as_i64();
    let source_record_id = night.source_record_id().as_str().to_owned();
    let morning_of = night.morning_of().to_string();
    let measured = night.measured();
    let from = measured.from().instant().to_string();
    let until = measured.until().instant().to_string();
    let zone = measured.from().zone().id().to_owned();
    let last_night = night.last_night();
    let average = milliseconds(last_night.average.as_milliseconds());
    let high = milliseconds(last_night.five_minute_high.as_milliseconds());
    let weekly = night.weekly();
    let weekly_average = milliseconds(weekly.average.as_milliseconds());
    let status = HrvStatus::as_str(weekly.status);
    let low_upper = milliseconds(weekly.baseline.low_upper.as_milliseconds());
    let balanced_low = milliseconds(weekly.baseline.balanced_low.as_milliseconds());
    let balanced_upper = milliseconds(weekly.baseline.balanced_upper.as_milliseconds());
    let Provenance::Event(event) = night.provenance();
    let endpoint = event.endpoint().as_str().to_owned();
    let event_kind = event.kind().as_str().to_owned();
    let event_time = event.occurred_at().map(|at| at.as_timestamp().to_string());

    sqlx::query!(
        r#"
        INSERT INTO overnight_hrv (
            landing_record_id, source_record_id, morning_of, measured_from_utc,
            measured_until_utc, zone, last_night_average_ms,
            last_night_five_minute_high_ms, weekly_average_ms, status,
            baseline_low_upper_ms, baseline_balanced_low_ms, baseline_balanced_upper_ms,
            endpoint, event_kind, event_time, run_id
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        id,
        source_record_id,
        morning_of,
        from,
        until,
        zone,
        average,
        high,
        weekly_average,
        status,
        low_upper,
        balanced_low,
        balanced_upper,
        endpoint,
        event_kind,
        event_time,
        run_id,
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;

    for reading in last_night
        .readings
        .iter()
        .flat_map(domain::sequence::NonEmpty::iter)
    {
        write_reading(tx, id, reading).await?;
    }
    Ok(())
}

async fn write_reading(
    tx: &mut Transaction<'_, Sqlite>,
    night: i64,
    reading: &HrvReading,
) -> Result<(), StoreError> {
    let taken_at = reading.taken_at.instant().to_string();
    let zone = reading.taken_at.zone().id().to_owned();
    let value = milliseconds(reading.value.as_milliseconds());
    sqlx::query!(
        r#"
        INSERT INTO overnight_hrv_reading (night, taken_at_utc, zone, value_ms)
        VALUES (?, ?, ?, ?)
        "#,
        night,
        taken_at,
        zone,
        value,
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    Ok(())
}
