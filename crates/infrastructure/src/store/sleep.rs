//! The normalised layer for `garmin.sleep`, the nights raw is derived from,
//! and each night read back (#255).
//!
//! Three adapters, for the reason [`super::garmin_normalised`] has two: the
//! account reader has no `append`, the store writes the derivation, and the
//! history reads it for the analytical layer. Which serving of a night stands
//! is Garmin knowledge, in [`crate::garmin::account`].

use application::{
    AccountReader, NormalisedEntityStore, Reached, Reading as RawReading, SleepHistory, StoreError,
};
use domain::{
    analytical::{
        per_device::{Reading, Recorded},
        sleep::Night,
    },
    body::Sleep,
    landing::{InvalidStream, LandingRecordId, LandingStream},
    measure::PositiveDuration,
    normalised::{NormalisationRunId, OperatorZone, StartedAt, WorkoutCount},
};
use jiff::civil::Date;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::garmin::{SleepAccount, sleeps};

use super::{
    GarminSleepLandingStore, corrupt, count_from_storage, garmin_gym::Row,
    normalisation_run_for_storage, store_error,
};

/// Raw, read-only, for Garmin's nights of sleep.
#[derive(Debug, Clone)]
pub struct GarminSleepAccountReader {
    pool: SqlitePool,
    stream: LandingStream,
}

impl GarminSleepAccountReader {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(GarminSleepLandingStore::STREAM)?,
        })
    }
}

impl AccountReader for GarminSleepAccountReader {
    type Account = SleepAccount;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn accounts(&self, reading: &RawReading) -> Result<Reached<SleepAccount>, StoreError> {
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
            FROM garmin_sleep_landing
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
        Ok(Reached {
            up_to: super::marks::up_to(records.iter()),
            previously: marks.map(|marks| sleeps(marks.held(&records))),
            now: sleeps(records),
        })
    }
}

/// The normalised layer for a night's sleep.
#[derive(Debug, Clone)]
pub struct SqliteSleepStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteSleepStore {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(GarminSleepLandingStore::STREAM)?,
        })
    }
}

impl NormalisedEntityStore for SqliteSleepStore {
    type Entity = Sleep;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn replace(
        &self,
        run: NormalisationRunId,
        nights: Vec<Sleep>,
    ) -> Result<WorkoutCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;
        sqlx::query!("DELETE FROM sleep")
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        let written = nights.len();
        for night in nights {
            write_night(&mut tx, run_id, &night).await?;
        }
        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(written))
    }

    async fn revise(
        &self,
        run: NormalisationRunId,
        removed: Vec<LandingRecordId>,
        nights: Vec<Sleep>,
    ) -> Result<WorkoutCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let removed = super::ids_json(removed.iter().map(|id| id.as_i64()));
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;
        sqlx::query!(
            "DELETE FROM sleep WHERE landing_record_id IN (SELECT value FROM json_each(?))",
            removed
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?;
        let written = nights.len();
        for night in nights {
            write_night(&mut tx, run_id, &night).await?;
        }
        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(written))
    }

    async fn count(&self) -> Result<WorkoutCount, StoreError> {
        let row = sqlx::query!(r#"SELECT COUNT(*) AS "total!: i64" FROM sleep"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;
        count_from_storage(Some(row.total)).map(WorkoutCount::from)
    }
}

async fn write_night(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    night: &Sleep,
) -> Result<(), StoreError> {
    let id = night.landed_as().as_i64();
    let source_record_id = night.source_record_id().as_str().to_owned();
    let morning_of = night.morning_of().to_string();
    let window = night.window();
    let fell_asleep = window.fell_asleep().instant().to_string();
    let woke = window.woke().instant().to_string();
    let zone = window.fell_asleep().zone().id().to_owned();
    let asleep = i64::try_from(night.asleep().as_seconds()).map_err(|error| corrupt(&error))?;
    let recorded_by = night.recorded_by().map(ToString::to_string);
    let event = super::served_by_a_feed(night.provenance())?;
    let endpoint = event.endpoint().as_str().to_owned();
    let event_kind = event.kind().as_str().to_owned();
    let event_time = event.occurred_at().map(|at| at.as_timestamp().to_string());
    sqlx::query!(
        r#"
        INSERT INTO sleep (
            landing_record_id, source_record_id, morning_of, fell_asleep_utc,
            woke_utc, zone, asleep_seconds, recorded_by,
            endpoint, event_kind, event_time, run_id
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        id,
        source_record_id,
        morning_of,
        fell_asleep,
        woke,
        zone,
        asleep,
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

/// Every night's sleep, read back for the analytical layer.
#[derive(Debug, Clone)]
pub struct SqliteSleepHistory {
    pool: SqlitePool,
}

impl SqliteSleepHistory {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl SleepHistory for SqliteSleepHistory {
    async fn nights(&self) -> Result<Vec<Recorded<Night>>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT morning_of AS "morning_of!: String",
                   fell_asleep_utc AS "fell_asleep!: String",
                   woke_utc AS "woke!: String",
                   zone AS "zone!: String",
                   asleep_seconds AS "asleep!: i64",
                   recorded_by
            FROM sleep
            ORDER BY morning_of ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        rows.into_iter()
            .map(|row| {
                let on: Date = row.morning_of.parse().map_err(|error| corrupt(&error))?;
                let zone = OperatorZone::try_from(row.zone).map_err(|error| corrupt(&error))?;
                let wall_clock = |stored: &str| -> Result<jiff::civil::DateTime, StoreError> {
                    let instant: jiff::Timestamp =
                        stored.parse().map_err(|error| corrupt(&error))?;
                    Ok(StartedAt::new(instant, zone.clone())
                        .wall_clock()
                        .datetime())
                };
                let seconds = u64::try_from(row.asleep).map_err(|error| corrupt(&error))?;
                let asleep =
                    PositiveDuration::from_seconds(seconds).map_err(|error| corrupt(&error))?;
                let by = row
                    .recorded_by
                    .map(|identifier| identifier.parse().map_err(|error| corrupt(&error)))
                    .transpose()?;
                Ok(Recorded {
                    reading: Reading {
                        on,
                        value: Night {
                            asleep,
                            fell_asleep: wall_clock(&row.fell_asleep)?,
                            woke: wall_clock(&row.woke)?,
                        },
                    },
                    by,
                })
            })
            .collect()
    }
}
