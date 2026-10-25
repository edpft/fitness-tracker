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
    analytical::per_device::{Reading, Recorded},
    body::RestingHeartRate,
    device::DeviceIdentifier,
    landing::{InvalidStream, LandingStream},
    measure::{BeatsPerMinute, HeartRateVariability},
    normalised::{NormalisationRunId, WorkoutCount},
};
use jiff::civil::Date;
use sqlx::SqlitePool;

use crate::garmin::{DayAccount, GarminDayEntity, WearDay, days};

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
        let mut sleeps = super::garmin_normalised::sleeps(&self.pool).await?;
        Ok(days(records)
            .into_iter()
            .map(|day| {
                let sleep = sleeps.remove(day.day().source_record_id().as_str());
                day.slept(sleep)
            })
            .collect())
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
        replace(&self.pool, run, &days, &[]).await
    }

    async fn count(&self) -> Result<WorkoutCount, StoreError> {
        count(&self.pool).await
    }
}

/// The normalised layer for the wrist's day: resting heart rate, and whether a
/// watch was worn (#569).
#[derive(Debug, Clone)]
pub struct SqliteDailyHeartRateStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteDailyHeartRateStore {
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

impl NormalisedEntityStore for SqliteDailyHeartRateStore {
    type Entity = GarminDayEntity;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    /// **Counts resting heart rates**, as it did before it wrote wear: what
    /// the stream is reported as holding is unchanged.
    async fn replace(
        &self,
        run: NormalisationRunId,
        entities: Vec<GarminDayEntity>,
    ) -> Result<WorkoutCount, StoreError> {
        let mut days = Vec::new();
        let mut wear = Vec::new();
        for entity in entities {
            match entity {
                GarminDayEntity::RestingHeartRate(day) => days.push(day),
                GarminDayEntity::Wear(day) => wear.push(day),
            }
        }
        replace(&self.pool, run, &days, &wear).await
    }

    async fn count(&self) -> Result<WorkoutCount, StoreError> {
        count(&self.pool).await
    }
}

async fn replace(
    pool: &SqlitePool,
    run: NormalisationRunId,
    days: &[RestingHeartRate],
    wear: &[WearDay],
) -> Result<WorkoutCount, StoreError> {
    let run_id = normalisation_run_for_storage(run)?;
    let mut tx = pool.begin().await.map_err(|error| store_error(&error))?;

    sqlx::query!("DELETE FROM resting_heart_rate")
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?;
    sqlx::query!("DELETE FROM wear_day")
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?;

    for day in days {
        let id = day.landed_as().as_i64();
        let source_record_id = day.source_record_id().as_str().to_owned();
        let date = day.day().to_string();
        let rate = i64::from(day.rate().as_u32());
        let event = super::served_by_a_feed(day.provenance())?;
        let endpoint = event.endpoint().as_str().to_owned();
        let event_kind = event.kind().as_str().to_owned();
        let event_time = event.occurred_at().map(|at| at.as_timestamp().to_string());
        let recorded_by = day.recorded_by().map(ToString::to_string);
        sqlx::query!(
            r#"
            INSERT INTO resting_heart_rate (
                landing_record_id, source_record_id, day, rate_bpm,
                endpoint, event_kind, event_time, run_id, recorded_by
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            id,
            source_record_id,
            date,
            rate,
            endpoint,
            event_kind,
            event_time,
            run_id,
            recorded_by,
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?;
    }

    for day in wear {
        let id = day.landed_as.as_i64();
        let source_record_id = day.source_record_id.as_str().to_owned();
        let date = day.day.to_string();
        let worn = i64::from(day.worn);
        sqlx::query!(
            r#"
            INSERT INTO wear_day (landing_record_id, source_record_id, day, worn, run_id)
            VALUES (?, ?, ?, ?, ?)
            "#,
            id,
            source_record_id,
            date,
            worn,
            run_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?;
    }

    tx.commit().await.map_err(|error| store_error(&error))?;
    Ok(WorkoutCount::from(days.len()))
}

async fn count(pool: &SqlitePool) -> Result<WorkoutCount, StoreError> {
    let row = sqlx::query!(r#"SELECT COUNT(*) AS "total!: i64" FROM resting_heart_rate"#)
        .fetch_one(pool)
        .await
        .map_err(|error| store_error(&error))?;
    count_from_storage(Some(row.total)).map(WorkoutCount::from)
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
    async fn days(&self) -> Result<Vec<Recorded<BeatsPerMinute>>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT day AS "day!: String", rate_bpm AS "rate!: i64", recorded_by
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
                Ok(Recorded {
                    reading: Reading { on, value },
                    by: recorded_by(row.recorded_by)?,
                })
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
    async fn nights(&self) -> Result<Vec<Recorded<HeartRateVariability>>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT morning_of AS "morning_of!: String",
                   last_night_average_ms AS "average!: i64",
                   recorded_by
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
                Ok(Recorded {
                    reading: Reading { on, value },
                    by: recorded_by(row.recorded_by)?,
                })
            })
            .collect()
    }
}

/// The device a row names, where it names one.
fn recorded_by(stored: Option<String>) -> Result<Option<DeviceIdentifier>, StoreError> {
    stored
        .map(|identifier| identifier.parse().map_err(|error| corrupt(&error)))
        .transpose()
}
