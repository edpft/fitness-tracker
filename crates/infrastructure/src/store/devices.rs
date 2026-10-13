//! The devices the operator uses (#427): what Garmin's recordings say about
//! them, and what he has said.
//!
//! **Only his statements are written.** The baseline is translated from raw
//! each time it is asked for (§ 7), so it moves forward as recordings land and
//! there is nothing to keep in step with them.
//!
//! **A statement is kept when a later one supersedes it** (§ 13): "the one that
//! stands" is the latest of its kind, which is the register's to work out, and
//! this table is append-only in practice.

use std::{collections::BTreeSet, num::NonZeroU64};

use application::{DeviceStatements, RecordedDevices, StoreError};
use domain::{
    device::{
        AddedDevice, DeviceIdentifier, DeviceName, DeviceStatement, LastUsed, Measurand, Sighting,
        Stated, StatedDevice, Statement,
    },
    landing::LandingStream,
    normalised::OperatorZone,
};
use jiff::{Timestamp, civil::Date};
use sqlx::SqlitePool;

use crate::{
    garmin::{account::activities, devices::sightings},
    peloton::paired,
    withings::devices as withings,
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

        Ok(activities(landed, Vec::new(), recordings)
            .iter()
            .flat_map(|account| sightings(account, &self.zone))
            .collect())
    }
}

/// What Withings' landed measurements say about the scales that took them.
#[derive(Debug, Clone)]
pub struct WithingsRecordedDevices {
    reader: super::WithingsWeighInAccountReader,
    zone: OperatorZone,
}

impl WithingsRecordedDevices {
    /// # Errors
    ///
    /// [`domain::landing::InvalidStream`] if the landing store's stream
    /// constant is not a stream name.
    pub fn new(
        pool: SqlitePool,
        zone: OperatorZone,
    ) -> Result<Self, domain::landing::InvalidStream> {
        Ok(Self {
            reader: super::WithingsWeighInAccountReader::new(pool)?,
            zone,
        })
    }
}

impl RecordedDevices for WithingsRecordedDevices {
    async fn sightings(&self) -> Result<Vec<Sighting>, StoreError> {
        use application::AccountReader as _;

        Ok(self
            .reader
            .accounts()
            .await?
            .iter()
            .flat_map(|account| withings::sightings(account, &self.zone))
            .collect())
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

/// What the operator has said about his devices.
#[derive(Debug, Clone)]
pub struct SqliteDeviceStatementStore {
    pool: SqlitePool,
}

impl SqliteDeviceStatementStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// Every column a statement could fill, as one statement fills them.
#[derive(Default)]
struct Columns {
    kind: &'static str,
    device: Option<String>,
    other_device: Option<String>,
    first_used: Option<String>,
    last_used: Option<String>,
    name: Option<String>,
    measures: Option<String>,
}

fn columns(statement: &DeviceStatement) -> Columns {
    match statement {
        DeviceStatement::FirstUsed { device, on } => Columns {
            kind: "first-used",
            device: Some(device.to_string()),
            first_used: Some(on.to_string()),
            ..Columns::default()
        },
        DeviceStatement::LastUsed { device, last } => Columns {
            kind: "last-used",
            device: Some(device.to_string()),
            last_used: Some(last_used_for_storage(*last)),
            ..Columns::default()
        },
        DeviceStatement::Named { device, name } => Columns {
            kind: "named",
            device: Some(device.to_string()),
            name: Some(name.to_string()),
            ..Columns::default()
        },
        DeviceStatement::Sends { device, what } => Columns {
            kind: "sends",
            device: Some(device.to_string()),
            measures: Some(measures_for_storage(what)),
            ..Columns::default()
        },
        DeviceStatement::SameDevice { device, other } => Columns {
            kind: "same-device",
            device: Some(device.to_string()),
            other_device: Some(other.to_string()),
            ..Columns::default()
        },
    }
}

fn measures_for_storage(measures: &BTreeSet<Measurand>) -> String {
    measures
        .iter()
        .map(|measurand| measurand.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

fn measures_from_storage(stored: Option<&str>) -> Result<BTreeSet<Measurand>, StoreError> {
    stored
        .ok_or_else(|| corrupt(&"a statement missing what a device measures"))?
        .split(',')
        .filter(|key| !key.is_empty())
        .map(str::parse::<Measurand>)
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(|error| corrupt(&error))
}

/// The form [`LastUsed`] parses back from, which is not the form it prints.
fn last_used_for_storage(last: LastUsed) -> String {
    match last {
        LastUsed::On(date) => date.to_string(),
        LastUsed::StillInUse => "still-in-use".to_owned(),
    }
}

impl DeviceStatements for SqliteDeviceStatementStore {
    async fn statements(&self) -> Result<Vec<Stated>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   stated_at AS "stated_at!: String",
                   kind AS "kind!: String",
                   device,
                   other_device,
                   first_used,
                   last_used,
                   name,
                   measures
            FROM device_statement
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        rows.into_iter()
            .map(|row| {
                stated(&StoredStatement {
                    id: row.id,
                    stated_at: row.stated_at,
                    kind: row.kind,
                    device: row.device,
                    other_device: row.other_device,
                    first_used: row.first_used,
                    last_used: row.last_used,
                    name: row.name,
                    measures: row.measures,
                })
            })
            .collect()
    }

    async fn state(&self, at: Timestamp, statement: &DeviceStatement) -> Result<(), StoreError> {
        let columns = columns(statement);
        let stated_at = at.to_string();
        sqlx::query!(
            "INSERT INTO device_statement
                 (stated_at, kind, device, other_device, first_used, last_used, name, measures)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            stated_at,
            columns.kind,
            columns.device,
            columns.other_device,
            columns.first_used,
            columns.last_used,
            columns.name,
            columns.measures,
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(())
    }

    async fn add(&self, at: Timestamp, added: &AddedDevice) -> Result<StatedDevice, StoreError> {
        let stated_at = at.to_string();
        let name = added.name().to_string();
        let first_used = added.first_used().to_string();
        let last_used = last_used_for_storage(added.last_used());
        let measures = measures_for_storage(added.measures());
        let id = sqlx::query_scalar!(
            r#"INSERT INTO device_statement
                 (stated_at, kind, first_used, last_used, name, measures)
             VALUES (?, 'added', ?, ?, ?, ?)
             RETURNING id AS "id!: i64""#,
            stated_at,
            first_used,
            last_used,
            name,
            measures,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        u64::try_from(id)
            .ok()
            .and_then(NonZeroU64::new)
            .map(StatedDevice::new)
            .ok_or_else(|| corrupt(&"a statement numbered zero or below"))
    }
}

/// One row of `device_statement`, as read.
struct StoredStatement {
    id: i64,
    stated_at: String,
    kind: String,
    device: Option<String>,
    other_device: Option<String>,
    first_used: Option<String>,
    last_used: Option<String>,
    name: Option<String>,
    measures: Option<String>,
}

/// A statement, from the row that keeps it.
fn stated(row: &StoredStatement) -> Result<Stated, StoreError> {
    let missing = || corrupt(&format!("a {} statement missing a value", row.kind));
    let device = || -> Result<DeviceIdentifier, StoreError> {
        row.device
            .as_deref()
            .ok_or_else(missing)?
            .parse()
            .map_err(|error| corrupt(&error))
    };
    let date = |text: Option<&str>| -> Result<Date, StoreError> {
        text.ok_or_else(missing)?
            .parse()
            .map_err(|error| corrupt(&error))
    };
    let last = || -> Result<LastUsed, StoreError> {
        row.last_used
            .as_deref()
            .ok_or_else(missing)?
            .parse()
            .map_err(|error| corrupt(&error))
    };
    let name = || -> Result<DeviceName, StoreError> {
        row.name
            .as_deref()
            .ok_or_else(missing)?
            .parse()
            .map_err(|error| corrupt(&error))
    };
    let statement = match row.kind.as_str() {
        "added" => {
            let number = u64::try_from(row.id)
                .ok()
                .and_then(NonZeroU64::new)
                .ok_or_else(|| corrupt(&"a statement numbered zero or below"))?;
            let measures = measures_from_storage(row.measures.as_deref())?;
            Statement::Added {
                device: StatedDevice::new(number),
                added: AddedDevice::new(
                    name()?,
                    measures,
                    date(row.first_used.as_deref())?,
                    last()?,
                )
                .map_err(|error| corrupt(&error))?,
            }
        }
        "first-used" => Statement::About(DeviceStatement::FirstUsed {
            device: device()?,
            on: date(row.first_used.as_deref())?,
        }),
        "last-used" => Statement::About(DeviceStatement::LastUsed {
            device: device()?,
            last: last()?,
        }),
        "named" => Statement::About(DeviceStatement::Named {
            device: device()?,
            name: name()?,
        }),
        "sends" => Statement::About(DeviceStatement::Sends {
            device: device()?,
            what: measures_from_storage(row.measures.as_deref())?,
        }),
        "same-device" => Statement::About(DeviceStatement::SameDevice {
            device: device()?,
            other: row
                .other_device
                .as_deref()
                .ok_or_else(missing)?
                .parse()
                .map_err(|error| corrupt(&error))?,
        }),
        other => return Err(corrupt(&format!("a statement of kind {other:?}"))),
    };
    Ok(Stated {
        at: row
            .stated_at
            .parse::<Timestamp>()
            .map_err(|error| corrupt(&error))?,
        statement,
    })
}
