//! The normalised layer for `peloton.workouts`, and the account raw is derived
//! from.
//!
//! Two adapters in one file for the reason [`super::normalised`] has two: one
//! reads the input and the other writes the derivation, and neither can do the
//! other's job. The reader has no `append`, so a derivation holding one could
//! not mutate raw if it tried.
//!
//! **The reader reads two tables and groups what it finds into sessions.** A
//! ride's start and duration are in one table and its samples in the other,
//! because Peloton serves them from two endpoints; and a session is one, two or
//! three of those rides, because Peloton files a warm-up, a ride and a
//! cool-down separately. Constitution § 3.1 composes both.
//!
//! **The grouping rule is not here.** Which of Peloton's records belong to one
//! session is Peloton knowledge — a class type, a series, a `workout_type` —
//! and it lives in [`crate::peloton::sessions`]. This adapter reads rows and
//! hands them over. What it keeps is the half only it can do: reaching a store,
//! so that what the translator receives is whole and cannot go back for more.
//!
//! **Reading both tables is not a stream reaching into another stream.** The
//! *extraction* adapters are kept apart on purpose — a walk of the graphs must
//! not need a walk of the list to have happened — but a derivation reads raw,
//! and both of these are raw for one source. What it must not do is depend on
//! both being current, which is why a ride with no graph is a refusal rather
//! than an error.

use application::{AccountReader, NormalisedEntityStore, StoreError};
use domain::{
    cycling::{BikePlusRide, Ftp, FtpProvenance, HeartRateSeries, PerformedSession, Watts},
    landing::{
        Endpoint, EventKind, EventProvenance, EventTime, FetchedAt, InvalidStream, LandedRecord,
        LandingRecord, LandingRecordId, LandingStream, RawPayload, SourceRecordId,
    },
    measure::Duration,
    normalised::{NormalisationRunId, WorkoutCount},
};
use jiff::civil::Date;
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::collections::BTreeMap;

use crate::peloton::{LandedRide, SessionAccount, group};

use super::{
    PelotonRideLandingStore, PelotonRideSampleLandingStore, corrupt, count_from_storage,
    normalisation_run_for_storage, store_error,
};

/// Raw, read-only, for Peloton sessions.
#[derive(Debug, Clone)]
pub struct PelotonSessionAccountReader {
    pool: SqlitePool,
    stream: LandingStream,
}

impl PelotonSessionAccountReader {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name. Taken from there rather than restated, so the reader and the
    /// writer cannot come to disagree about which table they are about.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(PelotonRideLandingStore::STREAM)?,
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

impl AccountReader for PelotonSessionAccountReader {
    /// One session: its rides in order, each with the graph that carries its
    /// samples.
    type Account = SessionAccount;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn accounts(&self) -> Result<Vec<SessionAccount>, StoreError> {
        // The graphs first, keyed by the workout they belong to. **The latest
        // landed graph wins** where a workout has more than one: a graph
        // carries no identity of its own, so there is no pairing to preserve —
        // and § 10's rule that the later of two servings supersedes is a
        // canonical-layer rule the derivation may act on where doing so needs
        // nothing it cannot see. It cannot arise in the operator's account, and
        // the alternative is refusing a ride over a duplicate of a payload that
        // is byte-identical.
        let samples_stream = LandingStream::try_from(super::PelotonRideSampleLandingStore::STREAM)
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
            FROM peloton_ride_sample_landing
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
            FROM peloton_ride_landing
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut rides = Vec::with_capacity(rows.len());
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
            rides.push(LandedRide { ride, samples });
        }

        Ok(group(rides))
    }
}

/// The normalised layer for Peloton sessions.
#[derive(Debug, Clone)]
pub struct SqliteCyclingSessionStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteCyclingSessionStore {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(PelotonRideLandingStore::STREAM)?,
        })
    }
}

impl NormalisedEntityStore for SqliteCyclingSessionStore {
    type Entity = PerformedSession;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn replace(
        &self,
        run: NormalisationRunId,
        sessions: Vec<PerformedSession>,
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
        // **Rebuilt with the sessions it is derived from**, in their
        // transaction, so it cannot be left standing against a layer that no
        // longer says what it was computed from (issue #56).
        sqlx::query!("DELETE FROM ftp")
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        sqlx::query!("DELETE FROM cycling_session")
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;

        let written = sessions.len();
        for session in sessions {
            write_session(&mut tx, run_id, &session).await?;
        }

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(written))
    }

    async fn count(&self) -> Result<WorkoutCount, StoreError> {
        let row = sqlx::query!(r#"SELECT COUNT(*) AS "total!: i64" FROM cycling_session"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;
        count_from_storage(Some(row.total)).map(WorkoutCount::from)
    }
}

/// One session and its rides, inside the caller's transaction.
async fn write_session(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    session: &PerformedSession,
) -> Result<(), StoreError> {
    let rides = session.rides();
    // The first ride's landing record. A session is ours and the source names
    // none, so its key is the earliest record it composes.
    let session_id = rides
        .first()
        .ok_or_else(|| StoreError::Corrupt {
            detail: "a session with no rides".to_owned(),
        })?
        .landed_as()
        .ride
        .as_i64();
    let kind = session.kind();

    sqlx::query!(
        r#"
        INSERT INTO cycling_session (landing_record_id, kind, run_id)
        VALUES (?, ?, ?)
        "#,
        session_id,
        kind,
        run_id,
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;

    write_ftp(tx, run_id, session, session_id).await?;

    for (ride, role) in rides.iter().zip(roles_of(session)) {
        write_ride(tx, run_id, ride, session_id, role).await?;
    }
    Ok(())
}

/// What each of a session's rides was for, in the session's own order.
///
/// Read off the variant rather than stored on the ride, because the variant is
/// what knows: a `Test`'s first ride is its warm-up by construction.
fn roles_of(session: &PerformedSession) -> Vec<&'static str> {
    match session {
        PerformedSession::Ride { cool_down, .. } => {
            let mut roles = vec!["main"];
            if cool_down.is_some() {
                roles.push("cool-down");
            }
            roles
        }
        PerformedSession::Test { cool_down, .. } => {
            let mut roles = vec!["warm-up", "effort"];
            if cool_down.is_some() {
                roles.push("cool-down");
            }
            roles
        }
    }
}

/// One ride and its two series, inside the caller's transaction.
///
/// Its own function so the writes above read as what they are — empty, then
/// write each — rather than as a hundred lines of column lists.
/// The FTP a test session measured, where it measured one.
///
/// **§ 13's interpretive parameter, derived rather than asked for.** The value
/// is the effort's stated average power times 0.95, dated to the day the test
/// was ridden, and it is written here so that it is rebuilt in the same
/// transaction as the sessions it is a function of.
///
/// An ordinary ride measures nothing and writes nothing: the variant says so,
/// so nothing here recognises a test from a class, a title or a duration.
async fn write_ftp(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    session: &PerformedSession,
    session_id: i64,
) -> Result<(), StoreError> {
    let Some(measured) = session.measured_ftp() else {
        return Ok(());
    };
    let ftp = measured.map_err(|error| StoreError::Corrupt {
        detail: error.to_string(),
    })?;

    let effect_from = ftp.from().to_string();
    let watts = i64::from(ftp.watts().as_u32());
    let provenance = ftp.provenance().as_str().to_owned();

    sqlx::query!(
        r#"
        INSERT INTO ftp (effect_from, watts, provenance, measured_by, run_id)
        VALUES (?, ?, ?, ?, ?)
        "#,
        effect_from,
        watts,
        provenance,
        session_id,
        run_id,
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;

    Ok(())
}

async fn write_ride(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    ride: &BikePlusRide,
    session_id: i64,
    role: &'static str,
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
    let average_power = i64::from(ride.average_power().as_u32());
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
            average_power_watts, heart_rate_declared_missing_seconds,
            endpoint, event_kind, event_time, run_id, session, role
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        ride_id,
        samples_id,
        source_record_id,
        started_at,
        zone,
        duration,
        distance,
        average_power,
        declared_missing,
        endpoint,
        event_kind,
        event_time,
        run_id,
        session_id,
        role,
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

/// The FTP series, as the cycling derivation left it.
///
/// **A reader over a table nothing else writes**: every row is a function of a
/// test session, written by [`SqliteCyclingSessionStore`] in the same
/// transaction, so this can never be asked about a value the record no longer
/// supports.
#[derive(Debug, Clone)]
pub struct SqliteFtpHistory {
    pool: SqlitePool,
}

impl SqliteFtpHistory {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl application::FtpHistory for SqliteFtpHistory {
    async fn in_force_on(&self, date: Date) -> Result<Option<Ftp>, StoreError> {
        // The latest value dated on or before the day. Dates are ISO-8601, so
        // the store's string ordering is the calendar's.
        let on = date.to_string();
        let row = sqlx::query!(
            r#"
            SELECT effect_from AS "effect_from!: String",
                   watts       AS "watts!: i64",
                   provenance  AS "provenance!: String"
              FROM ftp
             WHERE effect_from <= ?
             ORDER BY effect_from DESC
             LIMIT 1
            "#,
            on,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let corrupt = |detail: String| StoreError::Corrupt { detail };
        let watts = u32::try_from(row.watts)
            .map(Watts::from_u32)
            .map_err(|_| corrupt(format!("an FTP of {} watts", row.watts)))?;
        let from: Date = row
            .effect_from
            .parse()
            .map_err(|_| corrupt(format!("{:?} does not date an FTP", row.effect_from)))?;
        let provenance = match row.provenance.as_str() {
            "tested" => FtpProvenance::Tested,
            "estimated" => FtpProvenance::Estimated,
            "asserted" => FtpProvenance::Asserted,
            other => return Err(corrupt(format!("{other:?} does not name a provenance"))),
        };

        Ftp::new(watts, from, provenance)
            .map(Some)
            .map_err(|error| corrupt(error.to_string()))
    }
}

/// Both of Peloton's landing tables, counted as one.
///
/// **What a Peloton derivation actually reads.** A cycling session composes
/// rides from one table and their sample graphs from the other, so "how far
/// behind is the normalised layer" is a question about both — and answering it
/// from the ride table alone would report a derivation that has read 903
/// records as having read 477, and never show it falling behind.
#[derive(Debug, Clone)]
pub struct PelotonRawExtent {
    rides: PelotonRideLandingStore,
    samples: PelotonRideSampleLandingStore,
}

impl PelotonRawExtent {
    pub const fn new(
        rides: PelotonRideLandingStore,
        samples: PelotonRideSampleLandingStore,
    ) -> Self {
        Self { rides, samples }
    }
}

impl application::RawExtent for PelotonRawExtent {
    async fn records(&self) -> Result<domain::landing::RecordCount, StoreError> {
        let rides = application::LandingStore::count(&self.rides).await?;
        let samples = application::LandingStore::count(&self.samples).await?;
        Ok(domain::landing::RecordCount::from(
            rides.as_usize().saturating_add(samples.as_usize()),
        ))
    }
}
