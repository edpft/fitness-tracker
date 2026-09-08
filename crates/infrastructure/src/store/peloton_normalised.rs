//! The normalised layer for `peloton.workouts`, and the account raw is derived
//! from.
//!
//! Two adapters in one file for the reason [`super::normalised`] has two: one
//! reads the input and the other writes the derivation, and neither can do the
//! other's job. The reader has no `append`, so a derivation holding one could
//! not mutate raw if it tried.
//!
//! **The reader reads two tables**, which is the whole of what constitution
//! 3.1.0 allowed. A ride's start and duration are in `peloton_workout_landing`
//! and its samples are in `peloton_workout_sample_landing`, because Peloton
//! serves them from two endpoints; the graph names no workout, states no time,
//! no zone and no device, and neither response is an entity alone. Composing
//! them here rather than in the translator is deliberate: this is the only
//! thing at this end that can reach a store, so what the translator receives is
//! whole and cannot go back for more.
//!
//! **Reading both tables is not a stream reaching into another stream.** The
//! *extraction* adapters are kept apart on purpose — a walk of the graphs must
//! not need a walk of the list to have happened — but a derivation reads raw,
//! and both of these are raw for one source. What it must not do is depend on
//! both being current, which is why a ride with no graph is a refusal rather
//! than an error.

use application::{AccountReader, NormalisedEntityStore, StoreError};
use domain::{
    cycling::{BikePlusRide, HeartRateSeries},
    landing::{
        Endpoint, EventKind, EventProvenance, EventTime, FetchedAt, InvalidStream, LandedRecord,
        LandingRecord, LandingRecordId, LandingStream, RawPayload, SourceRecordId,
    },
    measure::Duration,
    normalised::{NormalisationRunId, NormalisedEntity, WorkoutCount},
};
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::collections::BTreeMap;

use crate::peloton::RideAccount;

use super::{
    PelotonWorkoutLandingStore, corrupt, count_from_storage, normalisation_run_for_storage,
    store_error,
};

/// Raw, read-only, for Peloton rides — both halves of one.
#[derive(Debug, Clone)]
pub struct PelotonRideAccountReader {
    pool: SqlitePool,
    stream: LandingStream,
}

impl PelotonRideAccountReader {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name. Taken from there rather than restated, so the reader and the
    /// writer cannot come to disagree about which table they are about.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(PelotonWorkoutLandingStore::STREAM)?,
        })
    }
}

/// One row of either landing table, before it is a [`LandedRecord`].
struct Row {
    id: i64,
    endpoint: String,
    fetched_at: String,
    source_record_id: String,
    event_kind: String,
    event_time: Option<String>,
    payload: Vec<u8>,
}

impl Row {
    fn into_record(self, stream: &LandingStream) -> Result<LandedRecord, StoreError> {
        let occurred_at = self
            .event_time
            .as_deref()
            .map(EventTime::try_from)
            .transpose()
            .map_err(|error| corrupt(&error))?;

        let provenance = EventProvenance::new(
            Endpoint::try_from(self.endpoint.as_str()).map_err(|error| corrupt(&error))?,
            EventKind::try_from(self.event_kind.as_str()).map_err(|error| corrupt(&error))?,
            occurred_at,
        );

        let record = LandingRecord::land(
            stream.clone(),
            FetchedAt::try_from(self.fetched_at.as_str()).map_err(|error| corrupt(&error))?,
            SourceRecordId::try_from(self.source_record_id.as_str())
                .map_err(|error| corrupt(&error))?,
            provenance.into(),
            RawPayload::try_from(self.payload).map_err(|error| corrupt(&error))?,
        );

        Ok(LandedRecord::new(
            LandingRecordId::try_from(self.id).map_err(|error| corrupt(&error))?,
            record,
        ))
    }
}

impl AccountReader for PelotonRideAccountReader {
    /// A workout record and, where one has landed, its graph.
    type Account = RideAccount;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn accounts(&self) -> Result<Vec<RideAccount>, StoreError> {
        // The graphs first, keyed by the workout they belong to. **The latest
        // landed graph wins** where a workout has more than one: a graph
        // carries no identity of its own, so there is no pairing to preserve —
        // and § 10's rule that the later of two servings supersedes is a
        // canonical-layer rule the derivation may act on where doing so needs
        // nothing it cannot see. It cannot arise in the operator's account, and
        // the alternative is refusing a ride over a duplicate of a payload that
        // is byte-identical.
        let samples_stream =
            LandingStream::try_from(super::PelotonWorkoutSampleLandingStore::STREAM)
                .map_err(|error| corrupt(&error))?;

        let graph_rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   endpoint AS "endpoint!: String",
                   fetched_at AS "fetched_at!: String",
                   source_record_id AS "source_record_id!: String",
                   event_kind AS "event_kind!: String",
                   event_time AS "event_time: String",
                   payload AS "payload!: Vec<u8>"
            FROM peloton_workout_sample_landing
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut graphs: BTreeMap<String, LandedRecord> = BTreeMap::new();
        for row in graph_rows {
            let key = row.source_record_id.clone();
            let record = Row {
                id: row.id,
                endpoint: row.endpoint,
                fetched_at: row.fetched_at,
                source_record_id: row.source_record_id,
                event_kind: row.event_kind,
                event_time: row.event_time,
                payload: row.payload,
            }
            .into_record(&samples_stream)?;
            graphs.insert(key, record);
        }

        // Then the workouts, oldest first, by the store's own sequence — which
        // is the order the source served them, because raw is append-only.
        let rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   endpoint AS "endpoint!: String",
                   fetched_at AS "fetched_at!: String",
                   source_record_id AS "source_record_id!: String",
                   event_kind AS "event_kind!: String",
                   event_time AS "event_time: String",
                   payload AS "payload!: Vec<u8>"
            FROM peloton_workout_landing
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut accounts = Vec::with_capacity(rows.len());
        for row in rows {
            let samples = graphs.get(&row.source_record_id).cloned();
            let ride = Row {
                id: row.id,
                endpoint: row.endpoint,
                fetched_at: row.fetched_at,
                source_record_id: row.source_record_id,
                event_kind: row.event_kind,
                event_time: row.event_time,
                payload: row.payload,
            }
            .into_record(&self.stream)?;
            accounts.push(RideAccount { ride, samples });
        }

        Ok(accounts)
    }
}

/// The normalised layer for Peloton rides.
#[derive(Debug, Clone)]
pub struct SqliteBikePlusRideStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteBikePlusRideStore {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(PelotonWorkoutLandingStore::STREAM)?,
        })
    }
}

impl NormalisedEntityStore for SqliteBikePlusRideStore {
    type Entity = BikePlusRide;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn replace(
        &self,
        run: NormalisationRunId,
        rides: Vec<BikePlusRide>,
    ) -> Result<WorkoutCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        // One transaction, and a replacement rather than an update: § II says a
        // derivation is never mutated in place, and a derivation that failed
        // part-way must leave the previous one standing. Children first, so
        // nothing is orphaned between statements.
        sqlx::query!("DELETE FROM bike_plus_ride_heart_rate")
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        sqlx::query!("DELETE FROM bike_plus_ride_sample")
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        sqlx::query!("DELETE FROM bike_plus_ride")
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;

        let written = rides.len();
        for ride in rides {
            write_ride(&mut tx, run_id, &ride).await?;
        }

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(written))
    }

    async fn count(&self) -> Result<WorkoutCount, StoreError> {
        let row = sqlx::query!(r#"SELECT COUNT(*) AS "total!: i64" FROM bike_plus_ride"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;
        count_from_storage(Some(row.total)).map(WorkoutCount::from)
    }
}

/// One ride and its two series, inside the caller's transaction.
///
/// Its own function so the replacement above reads as what it is — empty, then
/// write each — rather than as a hundred lines of column lists.
async fn write_ride(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    ride: &BikePlusRide,
) -> Result<(), StoreError> {
    let landed = ride.landed_as();
    let ride_id = landed.ride.as_i64();
    let samples_id = landed.samples.as_i64();

    let started_at = ride.started_at().instant().to_string();
    let zone = ride.started_at().zone().id().to_owned();
    let source_record_id = ride.source_record_id().as_str().to_owned();
    let duration = seconds_for_storage(ride.duration())?;
    let distance =
        i64::try_from(ride.distance().as_millimetres()).map_err(|_| StoreError::Corrupt {
            detail: "a distance larger than the store can hold".to_owned(),
        })?;
    let declared_missing = ride
        .heart_rate()
        .and_then(HeartRateSeries::declared_missing)
        .map(seconds_for_storage)
        .transpose()?;

    let domain::landing::Provenance::Event(event) = ride.provenance();
    let endpoint = event.endpoint().as_str().to_owned();
    let event_kind = event.kind().as_str().to_owned();
    let event_time = event.occurred_at().map(|at| at.as_timestamp().to_string());

    sqlx::query!(
        r#"
        INSERT INTO bike_plus_ride (
            landing_record_id, samples_record_id, source_record_id,
            started_at_utc, zone, duration_seconds, distance_millimetres,
            heart_rate_declared_missing_seconds,
            endpoint, event_kind, event_time, run_id
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        ride_id,
        samples_id,
        source_record_id,
        started_at,
        zone,
        duration,
        distance,
        declared_missing,
        endpoint,
        event_kind,
        event_time,
        run_id,
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;

    for sample in ride.samples().iter() {
        let at = seconds_for_storage(sample.at)?;
        let power = i64::from(sample.power.as_u32());
        let cadence = i64::from(sample.cadence.as_revolutions_per_minute());
        let resistance = i64::from(sample.resistance.as_percentage());
        let speed = i64::try_from(sample.speed.as_millimetres_per_hour()).map_err(|_| {
            StoreError::Corrupt {
                detail: "a speed larger than the store can hold".to_owned(),
            }
        })?;

        sqlx::query!(
            r#"
            INSERT INTO bike_plus_ride_sample (
                ride, at_seconds, power_watts, cadence_rpm,
                resistance_percentage, speed_millimetres_per_hour
            )
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
            ride_id,
            at,
            power,
            cadence,
            resistance,
            speed,
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    }

    if let Some(series) = ride.heart_rate() {
        for sample in series.samples().iter() {
            let at = seconds_for_storage(sample.at)?;
            let beats = i64::from(sample.beats_per_minute.as_u32());

            sqlx::query!(
                r#"
                INSERT INTO bike_plus_ride_heart_rate (ride, at_seconds, beats_per_minute)
                VALUES (?, ?, ?)
                "#,
                ride_id,
                at,
                beats,
            )
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error(&error))?;
        }
    }

    Ok(())
}

/// A duration as the store holds it.
fn seconds_for_storage(duration: Duration) -> Result<i64, StoreError> {
    i64::try_from(duration.as_seconds()).map_err(|_| StoreError::Corrupt {
        detail: "a duration larger than the store can hold".to_owned(),
    })
}
