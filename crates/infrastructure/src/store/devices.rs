//! What each source's record says about the devices it used (#427).
//!
//! **Nothing is written here.** The register is translated from raw each time
//! it is asked for (§ 7), so it moves forward as recordings land and there is
//! nothing to keep in step with them.

use std::collections::BTreeMap;

use application::{RecordedDevices, StoreError, WearRecord};
use domain::{
    device::{Sighting, Wear},
    landing::LandingStream,
    normalised::OperatorZone,
};
use jiff::civil::Date;
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::{
    garmin::{
        account::activities,
        devices::{joined, sightings},
    },
    peloton::paired,
};

use super::{
    GarminActivityFileLandingStore, GarminActivityLandingStore, PelotonAccountLandingStore,
    corrupt, garmin_gym::Row, store_error,
};

/// What Garmin's landed recordings say about the hardware that made them.
#[derive(Debug, Clone)]
pub struct GarminRecordedDevices {
    pool: SqlitePool,
    zone: OperatorZone,
}

impl GarminRecordedDevices {
    /// `zone` places each recording on a day, as it does Garmin's gym sessions.
    pub const fn new(pool: SqlitePool, zone: OperatorZone) -> Self {
        Self { pool, zone }
    }
}

impl RecordedDevices for GarminRecordedDevices {
    async fn sightings(&self) -> Result<Vec<Sighting>, StoreError> {
        let activity_stream = LandingStream::try_from(GarminActivityLandingStore::STREAM)
            .map_err(|error| corrupt(&error))?;
        let file_stream = LandingStream::try_from(GarminActivityFileLandingStore::STREAM)
            .map_err(|error| corrupt(&error))?;

        let activity_rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   endpoint AS "endpoint!: String",
                   fetched_at AS "fetched_at!: String",
                   source_record_id AS "source_record_id!: String",
                   event_kind AS "event_kind!: String",
                   event_time AS "event_time: String",
                   payload AS "payload!: Vec<u8>"
            FROM garmin_activity_landing
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        let mut landed = Vec::with_capacity(activity_rows.len());
        for row in activity_rows {
            landed.push(
                Row {
                    id: row.id,
                    endpoint: row.endpoint,
                    fetched_at: row.fetched_at,
                    source_record_id: row.source_record_id,
                    event_kind: row.event_kind,
                    event_time: row.event_time,
                    payload: row.payload,
                }
                .into_record(&activity_stream)?,
            );
        }

        let file_rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   endpoint AS "endpoint!: String",
                   fetched_at AS "fetched_at!: String",
                   source_record_id AS "source_record_id!: String",
                   event_kind AS "event_kind!: String",
                   event_time AS "event_time: String",
                   payload AS "payload!: Vec<u8>"
            FROM garmin_activity_file_landing
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        let mut recordings = Vec::with_capacity(file_rows.len());
        for row in file_rows {
            recordings.push(
                Row {
                    id: row.id,
                    endpoint: row.endpoint,
                    fetched_at: row.fetched_at,
                    source_record_id: row.source_record_id,
                    event_kind: row.event_kind,
                    event_time: row.event_time,
                    payload: row.payload,
                }
                .into_record(&file_stream)?,
            );
        }

        Ok(joined(
            activities(landed, Vec::new(), recordings)
                .iter()
                .flat_map(|account| sightings(account, &self.zone))
                .collect(),
        ))
    }
}

/// What Peloton's landed accounts say about the monitors the Bike+ was paired
/// with, each as of when it was fetched.
#[derive(Debug, Clone)]
pub struct PelotonRecordedDevices {
    pool: SqlitePool,
    zone: OperatorZone,
}

impl PelotonRecordedDevices {
    pub const fn new(pool: SqlitePool, zone: OperatorZone) -> Self {
        Self { pool, zone }
    }
}

impl RecordedDevices for PelotonRecordedDevices {
    async fn sightings(&self) -> Result<Vec<Sighting>, StoreError> {
        let stream = LandingStream::try_from(PelotonAccountLandingStore::STREAM)
            .map_err(|error| corrupt(&error))?;
        let rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   endpoint AS "endpoint!: String",
                   fetched_at AS "fetched_at!: String",
                   source_record_id AS "source_record_id!: String",
                   event_kind AS "event_kind!: String",
                   event_time AS "event_time: String",
                   payload AS "payload!: Vec<u8>"
            FROM peloton_account_landing
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        let mut found = Vec::new();
        for row in rows {
            let landed = Row {
                id: row.id,
                endpoint: row.endpoint,
                fetched_at: row.fetched_at,
                source_record_id: row.source_record_id,
                event_kind: row.event_kind,
                event_time: row.event_time,
                payload: row.payload,
            }
            .into_record(&stream)?;
            found.extend(paired::sightings(&landed, &self.zone));
        }
        Ok(found)
    }
}

/// Which days Garmin's daily heart rate shows a wrist device worn.
#[derive(Debug, Clone)]
pub struct GarminWear {
    pool: SqlitePool,
}

impl GarminWear {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// What a day's answer says about wear. Everything else stays in the payload.
#[derive(Debug, Deserialize)]
struct Day {
    #[serde(default, rename = "restingHeartRate")]
    resting: Option<serde_json::Value>,
    #[serde(default, rename = "heartRateValues")]
    readings: Option<Vec<(Option<i64>, Option<i64>)>>,
}

impl Day {
    /// Worn if the day states a resting heart rate or any reading at all.
    fn worn(&self) -> bool {
        self.resting
            .as_ref()
            .is_some_and(|resting| !resting.is_null())
            || self
                .readings
                .as_ref()
                .is_some_and(|readings| readings.iter().any(|(_, bpm)| bpm.is_some()))
    }
}

impl WearRecord for GarminWear {
    async fn wear(&self) -> Result<Wear, StoreError> {
        // The latest serving of each day stands: today's answer grows.
        let rows = sqlx::query!(
            r#"
            SELECT source_record_id AS "day!: String", payload AS "payload!: Vec<u8>"
            FROM garmin_daily_heart_rate_landing AS landed
            WHERE id = (
                SELECT MAX(id) FROM garmin_daily_heart_rate_landing
                WHERE source_record_id = landed.source_record_id
            )
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut days = BTreeMap::new();
        for row in rows {
            let date: Date = row.day.parse().map_err(|error| corrupt(&error))?;
            // A day that will not read is no evidence either way.
            if let Ok(day) = serde_json::from_slice::<Day>(&row.payload) {
                days.insert(date, day.worn());
            }
        }
        Ok(Wear::new(days))
    }
}
