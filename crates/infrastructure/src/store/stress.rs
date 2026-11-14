//! The normalised layer for `garmin.daily_summary`, the days raw is derived
//! from, and each day's stress read back (#517, #777).
//!
//! Three adapters, for the reason [`super::sleep`] has them. Which serving of
//! a day stands is Garmin knowledge, in [`crate::garmin::account`].

use application::{
    AccountReader, NormalisedEntityStore, Reached, Reading as RawReading, StoreError, StressHistory,
};
use domain::{
    analytical::{
        per_device::{Reading, Recorded},
        stress::Measured,
    },
    body::{Stress, StressLevel},
    landing::{InvalidStream, LandedRecord, LandingRecordId, LandingStream},
    measure::PositiveDuration,
    normalised::{NormalisationRunId, WorkoutCount},
};
use jiff::civil::Date;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::garmin::{DayAccount, days};

use super::{
    GarminDailySummaryLandingStore, corrupt, count_from_storage, garmin_gym::Row,
    normalisation_run_for_storage, store_error,
};

/// Raw, read-only, for Garmin's daily summaries.
#[derive(Debug, Clone)]
pub struct GarminDailySummaryAccountReader {
    pool: SqlitePool,
    stream: LandingStream,
}

impl GarminDailySummaryAccountReader {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(GarminDailySummaryLandingStore::STREAM)?,
        })
    }
}

impl AccountReader for GarminDailySummaryAccountReader {
    type Account = DayAccount;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    /// **With Garmin's sleep record for each date**, which names the watch
    /// (#569), as a day's resting heart rate has it.
    async fn accounts(&self, reading: &RawReading) -> Result<Reached<DayAccount>, StoreError> {
        let marks = super::marks::Marks::of(reading)?;
        let rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   endpoint AS "endpoint!: String",
                   fetched_at AS "fetched_at!: String",
                   source_record_id AS "source_record_id!: String",
                   event_kind AS "event_kind!: String",
                   event_time AS "event_time: String",
                   payload AS "payload!: Vec<u8>"
            FROM garmin_daily_summary_landing
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
        let sleeps = super::garmin_normalised::sleeps(&self.pool).await?;
        let slept = |records: Vec<LandedRecord>, sleeps: &[LandedRecord]| {
            let mut sleeps = super::garmin_normalised::by_date(sleeps);
            days(records)
                .into_iter()
                .map(|day| {
                    let sleep = sleeps.remove(day.day().source_record_id().as_str());
                    day.slept(sleep)
                })
                .collect()
        };
        Ok(Reached {
            up_to: super::marks::up_to(records.iter().chain(&sleeps)),
            previously: marks.map(|marks| slept(marks.held(&records), &marks.held(&sleeps))),
            now: slept(records, &sleeps),
        })
    }
}

/// The normalised layer for a day's stress.
#[derive(Debug, Clone)]
pub struct SqliteStressStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteStressStore {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(GarminDailySummaryLandingStore::STREAM)?,
        })
    }
}

impl NormalisedEntityStore for SqliteStressStore {
    type Entity = Stress;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn replace(
        &self,
        run: NormalisationRunId,
        days: Vec<Stress>,
    ) -> Result<WorkoutCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let mut tx = super::writing(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;
        sqlx::query!("DELETE FROM stress")
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        let written = days.len();
        for day in days {
            write_day(&mut tx, run_id, &day).await?;
        }
        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(written))
    }

    async fn revise(
        &self,
        run: NormalisationRunId,
        removed: Vec<LandingRecordId>,
        days: Vec<Stress>,
    ) -> Result<WorkoutCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let removed = super::ids_json(removed.iter().map(|id| id.as_i64()));
        let mut tx = super::writing(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;
        sqlx::query!(
            "DELETE FROM stress WHERE landing_record_id IN (SELECT value FROM json_each(?))",
            removed
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?;
        let written = days.len();
        for day in days {
            write_day(&mut tx, run_id, &day).await?;
        }
        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(written))
    }

    async fn count(&self) -> Result<WorkoutCount, StoreError> {
        let row = sqlx::query!(r#"SELECT COUNT(*) AS "total!: i64" FROM stress"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;
        count_from_storage(Some(row.total)).map(WorkoutCount::from)
    }
}

async fn write_day(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    day: &Stress,
) -> Result<(), StoreError> {
    let id = day.landed_as().as_i64();
    let source_record_id = day.source_record_id().as_str().to_owned();
    let date = day.day().to_string();
    let average = i64::from(day.average().as_u8());
    let measured = i64::try_from(day.measured().as_seconds()).map_err(|error| corrupt(&error))?;
    let recorded_by = day.recorded_by().map(ToString::to_string);
    let event = super::served_by_a_feed(day.provenance())?;
    let endpoint = event.endpoint().as_str().to_owned();
    let event_kind = event.kind().as_str().to_owned();
    let event_time = event.occurred_at().map(|at| at.as_timestamp().to_string());
    sqlx::query!(
        r#"
        INSERT INTO stress (
            landing_record_id, source_record_id, day, average_level,
            measured_seconds, recorded_by, endpoint, event_kind, event_time, run_id
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        id,
        source_record_id,
        date,
        average,
        measured,
        recorded_by,
        endpoint,
        event_kind,
        event_time,
        run_id,
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    Ok(())
}

/// Every day's stress, read back for the analytical layer.
#[derive(Debug, Clone)]
pub struct SqliteStressHistory {
    pool: SqlitePool,
}

impl SqliteStressHistory {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl StressHistory for SqliteStressHistory {
    async fn days(&self) -> Result<Vec<Recorded<Measured>>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT day AS "day!: String",
                   average_level AS "average!: i64",
                   measured_seconds AS "measured!: i64",
                   recorded_by
            FROM stress
            ORDER BY day ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        rows.into_iter()
            .map(|row| {
                let on: Date = row.day.parse().map_err(|error| corrupt(&error))?;
                let average = StressLevel::new(row.average).map_err(|error| corrupt(&error))?;
                let seconds = u64::try_from(row.measured).map_err(|error| corrupt(&error))?;
                let over =
                    PositiveDuration::from_seconds(seconds).map_err(|error| corrupt(&error))?;
                let by = row
                    .recorded_by
                    .map(|identifier| identifier.parse().map_err(|error| corrupt(&error)))
                    .transpose()?;
                Ok(Recorded {
                    reading: Reading {
                        on,
                        value: Measured { average, over },
                    },
                    by,
                })
            })
            .collect()
    }
}

/// The days landed, and the sleep record naming each day's watch (#809).
impl application::Input for GarminDailySummaryAccountReader {
    fn reads() -> application::Reads {
        super::landed(&[
            super::GarminDailySummaryLandingStore::STREAM,
            super::GarminSleepLandingStore::STREAM,
        ])
    }
}

/// `stress` (#809).
impl application::Input for SqliteStressHistory {
    fn reads() -> application::Reads {
        super::tables(&[super::table::STRESS])
    }
}

/// Garmin's daily stress (#809).
impl application::Output for SqliteStressStore {
    fn writes() -> std::collections::BTreeSet<application::Table> {
        super::writes(&[super::table::STRESS])
    }
}
