//! Which devices Garmin's activities name, and which days a watch was worn, in
//! the normalised layer and read back for the register (#427, #569).
//!
//! **Written by `garmin.activities` and `garmin.daily_heart_rate`**, each
//! beside what it already derived: an activity's gym session and its devices
//! are one derivation, as a day's resting heart rate and its wear are.

use std::collections::{BTreeMap, BTreeSet};

use application::{NormalisedEntityStore, RecordedDevices, StoreError, WearRecord};
use domain::{
    device::{DeviceIdentifier, DeviceName, Measurand, Role, Sighting, Wear},
    gym::MeasuredGymSession,
    landing::{InvalidStream, LandingStream},
    normalised::{NormalisationRunId, OperatorZone, StartedAt, WorkoutCount},
};
use jiff::{Timestamp, civil::Date};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::garmin::{ActivityDevices, GarminActivityEntity, devices::joined};

use super::{
    GarminActivityLandingStore, corrupt, count_from_storage, garmin_gym::write_session,
    normalisation_run_for_storage, normalised::clear_stream, store_error,
};

/// Writes what Garmin's activities derive: the gym sessions, in the tables
/// [`super::SqliteMeasuredGymSessionStore`] writes, and every activity's
/// devices.
#[derive(Debug, Clone)]
pub struct SqliteGarminActivityStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteGarminActivityStore {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(GarminActivityLandingStore::STREAM)?,
        })
    }
}

impl NormalisedEntityStore for SqliteGarminActivityStore {
    type Entity = GarminActivityEntity;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    /// **Counts gym sessions**, as it did before it wrote devices: what the
    /// stream is reported as holding is unchanged.
    async fn replace(
        &self,
        run: NormalisationRunId,
        entities: Vec<GarminActivityEntity>,
    ) -> Result<WorkoutCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let stream = self.stream.to_string();

        let mut sessions: Vec<MeasuredGymSession> = Vec::new();
        let mut devices: Vec<ActivityDevices> = Vec::new();
        for entity in entities {
            match entity {
                GarminActivityEntity::GymSession(session) => sessions.push(*session),
                GarminActivityEntity::Devices(found) => devices.push(*found),
            }
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;
        clear_stream(&mut tx, &stream).await?;
        for session in &sessions {
            write_session(&mut tx, run_id, &stream, session).await?;
        }
        sqlx::query!("DELETE FROM device_sighting WHERE stream = ?", stream)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        for found in &devices {
            write_sightings(&mut tx, run_id, &stream, found).await?;
        }
        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(sessions.len()))
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

/// One activity's devices, in the order its recording names them.
async fn write_sightings(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    stream: &str,
    found: &ActivityDevices,
) -> Result<(), StoreError> {
    let landing_record_id = found.landed_as.as_i64();
    let source_record_id = found.source_record_id.as_str().to_owned();
    for sighting in found.sightings.iter() {
        let device = sighting.device.to_string();
        let name = sighting.name.as_str().to_owned();
        let role = match sighting.role {
            Role::Recorder => "recorder",
            Role::Paired => "paired",
        };
        let worn = i64::from(sighting.worn);
        let at = sighting.at.instant().to_string();
        let zone = sighting.at.zone().id().to_owned();
        let id = sqlx::query_scalar!(
            r#"
            INSERT INTO device_sighting (
                stream, landing_record_id, source_record_id, device, name, role,
                worn, at_utc, zone, run_id
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id AS "id!: i64"
            "#,
            stream,
            landing_record_id,
            source_record_id,
            device,
            name,
            role,
            worn,
            at,
            zone,
            run_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;

        for (position, identifier) in sighting.also.iter().enumerate() {
            let position = i64::try_from(position).map_err(|error| corrupt(&error))?;
            let identifier = identifier.to_string();
            sqlx::query!(
                r#"
                INSERT INTO device_sighting_identifier (sighting, position, identifier)
                VALUES (?, ?, ?)
                "#,
                id,
                position,
                identifier,
            )
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        let measurands = sighting
            .measured
            .iter()
            .map(|what| (what, "measured"))
            .chain(sighting.sent.iter().map(|what| (what, "sent")));
        for (what, how) in measurands {
            let what = what.to_string();
            sqlx::query!(
                r#"
                INSERT INTO device_sighting_measurand (sighting, measurand, how)
                VALUES (?, ?, ?)
                "#,
                id,
                what,
                how,
            )
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error(&error))?;
        }
    }
    Ok(())
}

/// Every sighting `stream` derived, in the order it derived them.
async fn sightings(
    pool: &SqlitePool,
    stream: &str,
    source: &'static str,
) -> Result<Vec<Sighting>, StoreError> {
    let mut snapshot = pool.begin().await.map_err(|error| store_error(&error))?;
    let rows = sqlx::query!(
        r#"
        SELECT id AS "id!: i64", device AS "device!: String", name AS "name!: String",
               role AS "role!: String", worn AS "worn!: i64",
               at_utc AS "at_utc!: String", zone AS "zone!: String"
          FROM device_sighting
         WHERE stream = ?
         ORDER BY id
        "#,
        stream
    )
    .fetch_all(&mut *snapshot)
    .await
    .map_err(|error| store_error(&error))?;

    let identifiers = sqlx::query!(
        r#"
        SELECT i.sighting AS "sighting!: i64", i.identifier AS "identifier!: String"
          FROM device_sighting_identifier AS i
          JOIN device_sighting AS s ON s.id = i.sighting
         WHERE s.stream = ?
         ORDER BY i.sighting, i.position
        "#,
        stream
    )
    .fetch_all(&mut *snapshot)
    .await
    .map_err(|error| store_error(&error))?;
    let mut also: BTreeMap<i64, Vec<DeviceIdentifier>> = BTreeMap::new();
    for row in identifiers {
        also.entry(row.sighting)
            .or_default()
            .push(row.identifier.parse().map_err(|error| corrupt(&error))?);
    }

    let measurands = sqlx::query!(
        r#"
        SELECT m.sighting AS "sighting!: i64", m.measurand AS "measurand!: String",
               m.how AS "how!: String"
          FROM device_sighting_measurand AS m
          JOIN device_sighting AS s ON s.id = m.sighting
         WHERE s.stream = ?
        "#,
        stream
    )
    .fetch_all(&mut *snapshot)
    .await
    .map_err(|error| store_error(&error))?;
    let mut measured: BTreeMap<i64, BTreeSet<Measurand>> = BTreeMap::new();
    let mut sent: BTreeMap<i64, BTreeSet<Measurand>> = BTreeMap::new();
    for row in measurands {
        let what: Measurand = row.measurand.parse().map_err(|error| corrupt(&error))?;
        let into = match row.how.as_str() {
            "measured" => &mut measured,
            "sent" => &mut sent,
            other => return Err(corrupt(&format!("{other:?} is not how a device measures"))),
        };
        into.entry(row.sighting).or_default().insert(what);
    }

    rows.into_iter()
        .map(|row| {
            let instant: Timestamp = row.at_utc.parse().map_err(|error| corrupt(&error))?;
            let zone = OperatorZone::try_from(row.zone).map_err(|error| corrupt(&error))?;
            Ok(Sighting {
                device: row.device.parse().map_err(|error| corrupt(&error))?,
                also: also.remove(&row.id).unwrap_or_default(),
                name: DeviceName::try_from(row.name).map_err(|error| corrupt(&error))?,
                role: match row.role.as_str() {
                    "recorder" => Role::Recorder,
                    "paired" => Role::Paired,
                    other => return Err(corrupt(&format!("{other:?} is not a device's role"))),
                },
                measured: measured.remove(&row.id).unwrap_or_default(),
                sent: sent.remove(&row.id).unwrap_or_default(),
                at: StartedAt::new(instant, zone),
                source,
                worn: row.worn != 0,
            })
        })
        .collect()
}

/// What Garmin's normalised activities say about the hardware that made them.
#[derive(Debug, Clone)]
pub struct GarminRecordedDevices {
    pool: SqlitePool,
}

impl GarminRecordedDevices {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl RecordedDevices for GarminRecordedDevices {
    /// **Joined across recordings here**, where every activity's sightings are
    /// in hand: one strap Garmin wrote two ways is one device, and telling so
    /// needs more than one activity.
    async fn sightings(&self) -> Result<Vec<Sighting>, StoreError> {
        Ok(joined(
            sightings(
                &self.pool,
                GarminActivityLandingStore::STREAM,
                crate::garmin::devices::SOURCE,
            )
            .await?,
        ))
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

impl WearRecord for GarminWear {
    async fn wear(&self) -> Result<Wear, StoreError> {
        let rows =
            sqlx::query!(r#"SELECT day AS "day!: String", worn AS "worn!: i64" FROM wear_day"#)
                .fetch_all(&self.pool)
                .await
                .map_err(|error| store_error(&error))?;
        let mut days = BTreeMap::new();
        for row in rows {
            let date: Date = row.day.parse().map_err(|error| corrupt(&error))?;
            days.insert(date, row.worn != 0);
        }
        Ok(Wear::new(days))
    }
}
