//! The normalised layer for `garmin.daily_heart_rate`, and the days raw is
//! derived from (#254), and both daily figures read back (§ 4).
//!
//! Two adapters, for the reason [`super::garmin_normalised`] has them: the
//! account reader has no `append`, and the store writes the derivation. Which
//! serving of a day stands is Garmin knowledge, in [`crate::garmin::account`].

use application::{
    AccountReader, NormalisedEntityStore, OvernightHrvHistory, RestingHeartRateHistory, StoreError,
};
use domain::{
    analytical::per_device::Reading,
    body::RestingHeartRate,
    landing::{InvalidStream, LandingStream},
    measure::{BeatsPerMinute, HeartRateVariability},
    normalised::{NormalisationRunId, WorkoutCount},
};
use jiff::civil::Date;
use sqlx::SqlitePool;

use crate::garmin::{DayAccount, days};

use super::{
    GarminDailyHeartRateLandingStore, corrupt, count_from_storage, garmin_gym::Row,
    normalisation_run_for_storage, store_error,
};

/// Raw, read-only, for the wrist's days.
#[derive(Debug, Clone)]
pub struct GarminDailyHeartRateAccountReader {
    pool: SqlitePool,
    stream: LandingStream,
}

impl GarminDailyHeartRateAccountReader {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(GarminDailyHeartRateLandingStore::STREAM)?,
        })
    }
}

impl AccountReader for GarminDailyHeartRateAccountReader {
    type Account = DayAccount;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn accounts(&self) -> Result<Vec<DayAccount>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   endpoint AS "endpoint!: String",
                   fetched_at AS "fetched_at!: String",
                   source_record_id AS "source_record_id!: String",
                   event_kind AS "event_kind!: String",
                   event_time AS "event_time: String",
                   payload AS "payload!: Vec<u8>"
            FROM garmin_daily_heart_rate_landing
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            records.push(
                Row {
                    id: row.id,
                    endpoint: row.endpoint,
                    fetched_at: row.fetched_at,
                    source_record_id: row.source_record_id,
                    event_kind: row.event_kind,
                    event_time: row.event_time,
                    payload: row.payload,
                }
                .into_record(&self.stream)?,
            );
        }
        Ok(days(records))
    }
}

/// The normalised layer for resting heart rate.
#[derive(Debug, Clone)]
pub struct SqliteRestingHeartRateStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteRestingHeartRateStore {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(GarminDailyHeartRateLandingStore::STREAM)?,
        })
    }
}

impl NormalisedEntityStore for SqliteRestingHeartRateStore {
    type Entity = RestingHeartRate;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn replace(
        &self,
        run: NormalisationRunId,
        days: Vec<RestingHeartRate>,
    ) -> Result<WorkoutCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        sqlx::query!("DELETE FROM resting_heart_rate")
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;

        let written = days.len();
        for day in days {
            let id = day.landed_as().as_i64();
            let source_record_id = day.source_record_id().as_str().to_owned();
            let date = day.day().to_string();
            let rate = i64::from(day.rate().as_u32());
            let event = super::served_by_a_feed(day.provenance())?;
            let endpoint = event.endpoint().as_str().to_owned();
            let event_kind = event.kind().as_str().to_owned();
            let event_time = event.occurred_at().map(|at| at.as_timestamp().to_string());
            sqlx::query!(
                r#"
                INSERT INTO resting_heart_rate (
                    landing_record_id, source_record_id, day, rate_bpm,
                    endpoint, event_kind, event_time, run_id
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#,
                id,
                source_record_id,
                date,
                rate,
                endpoint,
                event_kind,
                event_time,
                run_id,
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(written))
    }

    async fn count(&self) -> Result<WorkoutCount, StoreError> {
        let row = sqlx::query!(r#"SELECT COUNT(*) AS "total!: i64" FROM resting_heart_rate"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;
        count_from_storage(Some(row.total)).map(WorkoutCount::from)
    }
}

/// Every day's resting heart rate, read back for the analytical layer.
#[derive(Debug, Clone)]
pub struct SqliteRestingHeartRateHistory {
    pool: SqlitePool,
}

impl SqliteRestingHeartRateHistory {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl RestingHeartRateHistory for SqliteRestingHeartRateHistory {
    async fn days(&self) -> Result<Vec<Reading<BeatsPerMinute>>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT day AS "day!: String", rate_bpm AS "rate!: i64"
            FROM resting_heart_rate
            ORDER BY day ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        rows.into_iter()
            .map(|row| {
                let on: Date = row.day.parse().map_err(|error| corrupt(&error))?;
                let beats = u32::try_from(row.rate).map_err(|error| corrupt(&error))?;
                let value = BeatsPerMinute::new(beats).map_err(|error| corrupt(&error))?;
                Ok(Reading { on, value })
            })
            .collect()
    }
}

/// Every night's overnight HRV, read back for the analytical layer: last
/// night's average, by the morning it is reported under.
#[derive(Debug, Clone)]
pub struct SqliteOvernightHrvHistory {
    pool: SqlitePool,
}

impl SqliteOvernightHrvHistory {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl OvernightHrvHistory for SqliteOvernightHrvHistory {
    async fn nights(&self) -> Result<Vec<Reading<HeartRateVariability>>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT morning_of AS "morning_of!: String",
                   last_night_average_ms AS "average!: i64"
            FROM overnight_hrv
            ORDER BY morning_of ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        rows.into_iter()
            .map(|row| {
                let on: Date = row.morning_of.parse().map_err(|error| corrupt(&error))?;
                let milliseconds = u32::try_from(row.average).map_err(|error| corrupt(&error))?;
                let value = HeartRateVariability::from_milliseconds(milliseconds)
                    .map_err(|error| corrupt(&error))?;
                Ok(Reading { on, value })
            })
            .collect()
    }
}
