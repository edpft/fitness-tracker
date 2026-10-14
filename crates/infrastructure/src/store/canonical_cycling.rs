//! The canonical layer for cycling sessions (#408), and the normalised layer as
//! it reads it.
//!
//! **Everything downstream reads rides here**: which of the microcycle's
//! sessions were ridden, and which classes a holding microcycle must not
//! repeat. Both read Peloton's normalised rows until #408.
//!
//! **The series are copied, not referenced**, for the reason the gym's heart
//! rate is: § II.4 makes this the layer the analytical layer reads, and a
//! series reachable only through `bike_plus_ride_sample` would make every
//! reader of a canonical session reach past it into one source's tables.

use std::collections::{BTreeMap, BTreeSet};

use application::{
    CanonicalCyclingSessionStore, NormalisedCyclingSessionReader, PerformedCyclingSessions,
    RiddenSession, RiddenSessionLog, RiddenVenues, StoreError,
};
use domain::{
    canonical::{Attributed, NormalisedSessionId, SessionCount},
    cycling::{
        Cadence, CanonicalBikePlusRide, CanonicalCyclingSession, CanonicalRideRecord, Resistance,
        RideSample, RideVenue, Speed, Watts,
    },
    measure::{BeatsPerMinute, Duration, HeartRateSample, HeartRateSeries, Metres},
    normalised::{OperatorZone, StartedAt},
    sequence::NonEmpty,
};
use jiff::{Timestamp, civil::Date};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::{corrupt, count_for_storage, count_from_storage, store_error};

/// Every normalised cycling session, in the canonical shape.
#[derive(Debug, Clone)]
pub struct SqliteNormalisedCyclingSessionReader {
    pool: SqlitePool,
}

impl SqliteNormalisedCyclingSessionReader {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// One normalised ride's own row, before it is a canonical ride.
struct RideRow {
    session: i64,
    kind: String,
    key: i64,
    started_at_utc: String,
    zone: String,
    venue_reference: Option<String>,
    venue_called: Option<String>,
    duration_seconds: i64,
    distance_millimetres: i64,
    average_power_watts: i64,
    heart_rate_declared_missing_seconds: Option<i64>,
}

/// The samples and heart rate of every ride, keyed as [`RideRow::key`] is.
struct Series {
    samples: BTreeMap<i64, Vec<RideSample>>,
    heart_rate: BTreeMap<i64, Vec<HeartRateSample>>,
}

impl NormalisedCyclingSessionReader for SqliteNormalisedCyclingSessionReader {
    async fn all(&self) -> Result<Vec<CanonicalCyclingSession>, StoreError> {
        // A normalised session's id is its first ride's landing record, which
        // is what `cycling_session` is keyed on; a ride is keyed on its own.
        let rides = sqlx::query_as!(
            RideRow,
            r#"
            SELECT s.landing_record_id AS "session!: i64",
                   s.kind AS "kind!: String",
                   r.landing_record_id AS "key!: i64",
                   r.started_at_utc AS "started_at_utc!: String",
                   r.zone AS "zone!: String",
                   r.venue_reference, r.venue_called,
                   r.duration_seconds, r.distance_millimetres, r.average_power_watts,
                   r.heart_rate_declared_missing_seconds
              FROM cycling_session AS s
              JOIN bike_plus_ride AS r ON r.session = s.landing_record_id
             ORDER BY s.landing_record_id, r.started_at_utc, r.landing_record_id
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error(&e))?;

        // **One row per ride, its series as a JSON array**, for the reason the
        // writes are (see `write_samples`): a row per sample was most of the
        // time a rebuild took. Ordered in Rust, because `json_group_array`
        // promises no order.
        let samples = sqlx::query!(
            r#"
            SELECT ride AS "key!: i64",
                   json_group_array(json_array(
                       at_seconds, power_watts, cadence_rpm,
                       resistance_percentage, speed_millimetres_per_hour
                   )) AS "rows!: String"
              FROM bike_plus_ride_sample
             GROUP BY ride
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error(&e))?;
        let mut series = Series {
            samples: BTreeMap::new(),
            heart_rate: BTreeMap::new(),
        };
        for row in samples {
            let values: Vec<[i64; 5]> = serde_json::from_str(&row.rows).map_err(|e| corrupt(&e))?;
            let mut ride = values
                .into_iter()
                .map(|[at, power, cadence, resistance, speed]| {
                    sample(at, power, cadence, resistance, speed)
                })
                .collect::<Result<Vec<_>, _>>()?;
            ride.sort_by_key(|sample| sample.at);
            series.samples.insert(row.key, ride);
        }

        let heart_rate = sqlx::query!(
            r#"
            SELECT ride AS "key!: i64",
                   json_group_array(json_array(at_seconds, beats_per_minute)) AS "rows!: String"
              FROM bike_plus_ride_heart_rate
             GROUP BY ride
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error(&e))?;
        for row in heart_rate {
            let values: Vec<[i64; 2]> = serde_json::from_str(&row.rows).map_err(|e| corrupt(&e))?;
            let mut beats = values
                .into_iter()
                .map(|[at, beats]| beat(at, beats))
                .collect::<Result<Vec<_>, _>>()?;
            beats.sort_by_key(|beat| beat.at);
            series.heart_rate.insert(row.key, beats);
        }

        let mut sessions = sessions_of(rides, &mut series)?;
        sessions.sort_by_key(|session| session.started_at().instant());
        Ok(sessions)
    }
}

/// The canonical cycling layer, in SQLite.
#[derive(Debug, Clone)]
pub struct SqliteCanonicalCyclingSessionStore {
    pool: SqlitePool,
}

impl SqliteCanonicalCyclingSessionStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl CanonicalCyclingSessionStore for SqliteCanonicalCyclingSessionStore {
    async fn replace(
        &self,
        sessions: Vec<CanonicalCyclingSession>,
    ) -> Result<SessionCount, StoreError> {
        let mut tx = self.pool.begin().await.map_err(|e| store_error(&e))?;
        clear(&mut tx).await?;
        let written = sessions.len();
        for session in &sessions {
            write_session(&mut tx, session).await?;
        }
        tx.commit().await.map_err(|e| store_error(&e))?;
        Ok(SessionCount::from(written))
    }

    async fn count(&self) -> Result<SessionCount, StoreError> {
        let count =
            sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!" FROM canonical_cycling_session"#)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| store_error(&e))?;
        Ok(SessionCount::from(count_from_storage(Some(count))?))
    }
}

/// One canonical ride's own row, each field beside the normalised session it
/// was taken from.
struct CanonicalRideRow {
    session: i64,
    kind: String,
    position: i64,
    started_at_utc: String,
    zone: String,
    started_at_from: i64,
    venue_reference: String,
    venue_called: String,
    venue_from: i64,
    duration_seconds: i64,
    duration_from: i64,
    distance_millimetres: i64,
    distance_from: i64,
    average_power_watts: i64,
    average_power_from: i64,
    samples_from: i64,
    heart_rate_from: Option<i64>,
    heart_rate_declared_missing_seconds: Option<i64>,
    heart_rate_declared_missing_from: Option<i64>,
}

impl PerformedCyclingSessions for SqliteCanonicalCyclingSessionStore {
    async fn all(&self) -> Result<Vec<CanonicalCyclingSession>, StoreError> {
        let rides = sqlx::query_as!(
            CanonicalRideRow,
            r#"
            SELECT s.id AS "session!: i64", s.kind AS "kind!: String",
                   r.position AS "position!: i64",
                   r.started_at_utc AS "started_at_utc!: String", r.zone AS "zone!: String",
                   r.started_at_normalised_session AS "started_at_from!: i64",
                   r.venue_reference AS "venue_reference!: String",
                   r.venue_called AS "venue_called!: String",
                   r.venue_normalised_session AS "venue_from!: i64",
                   r.duration_seconds AS "duration_seconds!: i64",
                   r.duration_normalised_session AS "duration_from!: i64",
                   r.distance_millimetres AS "distance_millimetres!: i64",
                   r.distance_normalised_session AS "distance_from!: i64",
                   r.average_power_watts AS "average_power_watts!: i64",
                   r.average_power_normalised_session AS "average_power_from!: i64",
                   r.samples_normalised_session AS "samples_from!: i64",
                   r.heart_rate_normalised_session AS "heart_rate_from: i64",
                   r.heart_rate_declared_missing_seconds AS "heart_rate_declared_missing_seconds: i64",
                   r.heart_rate_declared_missing_normalised_session AS "heart_rate_declared_missing_from: i64"
              FROM canonical_cycling_session AS s
              JOIN canonical_bike_plus_ride AS r ON r.session = s.id
             ORDER BY s.id, r.position
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error(&e))?;

        // One row per ride, its series as a JSON array, as the normalised
        // reader does and for its reason.
        let samples = sqlx::query!(
            r#"
            SELECT session AS "session!: i64", ride_position AS "position!: i64",
                   json_group_array(json_array(
                       at_seconds, power_watts, cadence_rpm,
                       resistance_percentage, speed_millimetres_per_hour
                   )) AS "rows!: String"
              FROM canonical_bike_plus_ride_sample
             GROUP BY session, ride_position
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error(&e))?;
        let mut series: BTreeMap<(i64, i64), Vec<RideSample>> = BTreeMap::new();
        for row in samples {
            let values: Vec<[i64; 5]> = serde_json::from_str(&row.rows).map_err(|e| corrupt(&e))?;
            let mut ride = values
                .into_iter()
                .map(|[at, power, cadence, resistance, speed]| {
                    sample(at, power, cadence, resistance, speed)
                })
                .collect::<Result<Vec<_>, _>>()?;
            ride.sort_by_key(|sample| sample.at);
            series.insert((row.session, row.position), ride);
        }

        let heart_rate = sqlx::query!(
            r#"
            SELECT session AS "session!: i64", ride_position AS "position!: i64",
                   json_group_array(json_array(at_seconds, beats_per_minute)) AS "rows!: String"
              FROM canonical_bike_plus_ride_heart_rate
             GROUP BY session, ride_position
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error(&e))?;
        let mut hearts: BTreeMap<(i64, i64), Vec<HeartRateSample>> = BTreeMap::new();
        for row in heart_rate {
            let values: Vec<[i64; 2]> = serde_json::from_str(&row.rows).map_err(|e| corrupt(&e))?;
            let mut beats = values
                .into_iter()
                .map(|[at, beats]| beat(at, beats))
                .collect::<Result<Vec<_>, _>>()?;
            beats.sort_by_key(|beat| beat.at);
            hearts.insert((row.session, row.position), beats);
        }

        let mut grouped: Vec<(i64, String, Vec<CanonicalBikePlusRide>)> = Vec::new();
        for row in rides {
            let session = row.session;
            let kind = row.kind.clone();
            let ride = canonical_ride_of(row, &mut series, &mut hearts)?;
            match grouped.last_mut() {
                Some((last, _, rides)) if *last == session => rides.push(ride),
                _ => grouped.push((session, kind, vec![ride])),
            }
        }
        let mut sessions = grouped
            .into_iter()
            .map(|(_, kind, rides)| session_of(&kind, rides))
            .collect::<Result<Vec<_>, _>>()?;
        sessions.sort_by_key(|session| session.started_at().instant());
        Ok(sessions)
    }
}

fn canonical_ride_of(
    row: CanonicalRideRow,
    series: &mut BTreeMap<(i64, i64), Vec<RideSample>>,
    hearts: &mut BTreeMap<(i64, i64), Vec<HeartRateSample>>,
) -> Result<CanonicalBikePlusRide, StoreError> {
    let key = (row.session, row.position);
    let from = normalised_session_from_storage;
    let instant: Timestamp = row.started_at_utc.parse().map_err(|e| corrupt(&e))?;
    let zone = OperatorZone::try_from(row.zone).map_err(|e| corrupt(&e))?;
    let venue = RideVenue::new(&row.venue_reference, &row.venue_called).map_err(|e| corrupt(&e))?;
    let samples = NonEmpty::new(series.remove(&key).unwrap_or_default()).map_err(|_| {
        StoreError::Corrupt {
            detail: format!(
                "the canonical ride at {} has no samples",
                row.started_at_utc
            ),
        }
    })?;
    let heart_rate = match row.heart_rate_from {
        None => None,
        Some(id) => {
            let beats = NonEmpty::new(hearts.remove(&key).unwrap_or_default()).map_err(|_| {
                StoreError::Corrupt {
                    detail: format!(
                        "the canonical ride at {} names a heart rate it does not hold",
                        row.started_at_utc
                    ),
                }
            })?;
            Some(Attributed::new(HeartRateSeries::new(beats), from(id)?))
        }
    };
    let heart_rate_declared_missing = match (
        row.heart_rate_declared_missing_seconds,
        row.heart_rate_declared_missing_from,
    ) {
        (Some(seconds), Some(id)) => {
            Some(Attributed::new(seconds_from_storage(seconds)?, from(id)?))
        }
        _ => None,
    };

    Ok(CanonicalBikePlusRide::new(CanonicalRideRecord {
        started_at: Attributed::new(StartedAt::new(instant, zone), from(row.started_at_from)?),
        at: Attributed::new(venue, from(row.venue_from)?),
        duration: Attributed::new(
            seconds_from_storage(row.duration_seconds)?,
            from(row.duration_from)?,
        ),
        distance: Attributed::new(
            Metres::from_millimetres(
                u64::try_from(row.distance_millimetres).map_err(|e| corrupt(&e))?,
            ),
            from(row.distance_from)?,
        ),
        average_power: Attributed::new(
            Watts::from_u32(u32::try_from(row.average_power_watts).map_err(|e| corrupt(&e))?),
            from(row.average_power_from)?,
        ),
        samples: Attributed::new(samples, from(row.samples_from)?),
        heart_rate,
        heart_rate_declared_missing,
    }))
}

/// When each session was ridden, and every class it was ridden to.
///
/// A session's day is its first ride's, so one that ran past midnight is filed
/// under the day it started, as a gym session is.
impl RiddenSessionLog for SqliteCanonicalCyclingSessionStore {
    async fn ridden_between(&self, from: Date, to: Date) -> Result<Vec<RiddenSession>, StoreError> {
        // Widened in SQL and narrowed in Rust, because the day depends on the
        // zone on each row.
        let lower = from
            .checked_sub(jiff::Span::new().days(1))
            .map_err(|_| StoreError::Corrupt {
                detail: "a date before the calendar".to_owned(),
            })?
            .to_string();
        let upper = to
            .checked_add(jiff::Span::new().days(2))
            .map_err(|_| StoreError::Corrupt {
                detail: "a date beyond the calendar".to_owned(),
            })?
            .to_string();

        let rows = sqlx::query!(
            r#"
            SELECT first.session        AS "session!: i64",
                   first.started_at_utc AS "started_at_utc!: String",
                   first.zone           AS "zone!: String",
                   part.venue_reference AS "reference!: String",
                   part.venue_called    AS "called!: String"
              FROM canonical_bike_plus_ride AS first
              JOIN canonical_bike_plus_ride AS part ON part.session = first.session
             WHERE first.position = 0
               AND first.started_at_utc >= ? AND first.started_at_utc < ?
             ORDER BY first.started_at_utc, first.session, part.position
            "#,
            lower,
            upper,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error(&e))?;

        let mut sessions: Vec<(i64, RiddenSession)> = Vec::new();
        for row in rows {
            let started = super::moment_of(&row.started_at_utc, &row.zone)?;
            if started.date() < from || started.date() > to {
                continue;
            }
            let venue = RideVenue::new(&row.reference, &row.called).map_err(|e| corrupt(&e))?;
            match sessions.last_mut() {
                Some((session, ridden)) if *session == row.session => {
                    ridden.at.insert(venue);
                }
                _ => sessions.push((
                    row.session,
                    RiddenSession {
                        started,
                        at: BTreeSet::from([venue]),
                    },
                )),
            }
        }
        Ok(sessions.into_iter().map(|(_, ridden)| ridden).collect())
    }
}

/// Every class the canonical layer says was ridden.
impl RiddenVenues for SqliteCanonicalCyclingSessionStore {
    async fn ridden(&self) -> Result<BTreeSet<RideVenue>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT DISTINCT venue_reference AS "reference!: String",
                            venue_called    AS "called!: String"
              FROM canonical_bike_plus_ride
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error(&e))?;

        rows.into_iter()
            .map(|row| RideVenue::new(&row.reference, &row.called).map_err(|e| corrupt(&e)))
            .collect()
    }
}

/// Every row of the canonical cycling layer, children first.
///
/// Called by the Peloton derivation before it replaces its own rows, for the
/// gym's reason (#350): every field here names a `cycling_session` row, so the
/// layer cannot outlive them. `fitness next` rebuilds it straight after.
pub(super) async fn clear(tx: &mut Transaction<'_, Sqlite>) -> Result<(), StoreError> {
    sqlx::query!("DELETE FROM canonical_bike_plus_ride_heart_rate")
        .execute(&mut **tx)
        .await
        .map_err(|e| store_error(&e))?;
    sqlx::query!("DELETE FROM canonical_bike_plus_ride_sample")
        .execute(&mut **tx)
        .await
        .map_err(|e| store_error(&e))?;
    sqlx::query!("DELETE FROM canonical_bike_plus_ride")
        .execute(&mut **tx)
        .await
        .map_err(|e| store_error(&e))?;
    sqlx::query!("DELETE FROM canonical_cycling_session")
        .execute(&mut **tx)
        .await
        .map_err(|e| store_error(&e))?;
    Ok(())
}

async fn write_session(
    tx: &mut Transaction<'_, Sqlite>,
    session: &CanonicalCyclingSession,
) -> Result<(), StoreError> {
    let kind = session.kind();
    let id = sqlx::query_scalar!(
        r#"INSERT INTO canonical_cycling_session (kind) VALUES (?) RETURNING id AS "id!: i64""#,
        kind,
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;

    for (position, (ride, role)) in session
        .rides()
        .into_iter()
        .zip(roles_of(session))
        .enumerate()
    {
        write_ride(tx, id, count_for_storage(position)?, role, ride).await?;
    }
    Ok(())
}

/// What each of a session's rides was for, in the session's own order.
fn roles_of(session: &CanonicalCyclingSession) -> Vec<&'static str> {
    match session {
        CanonicalCyclingSession::Ride { cool_down, .. } => {
            let mut roles = vec!["main"];
            if cool_down.is_some() {
                roles.push("cool-down");
            }
            roles
        }
        CanonicalCyclingSession::Test { cool_down, .. } => {
            let mut roles = vec!["warm-up", "effort"];
            if cool_down.is_some() {
                roles.push("cool-down");
            }
            roles
        }
    }
}

async fn write_ride(
    tx: &mut Transaction<'_, Sqlite>,
    session: i64,
    position: i64,
    role: &str,
    ride: &CanonicalBikePlusRide,
) -> Result<(), StoreError> {
    let started_at = ride.started_at().value().instant().to_string();
    let zone = ride.started_at().value().zone().id().to_owned();
    let started_at_from = ride.started_at().normalised_session().as_i64();
    let reference = ride.at().value().reference().to_owned();
    let called = ride.at().value().called().to_owned();
    let venue_from = ride.at().normalised_session().as_i64();
    let duration = seconds_for_storage(ride.duration().copied())?;
    let duration_from = ride.duration().normalised_session().as_i64();
    let distance = i64::try_from(ride.distance().value().as_millimetres())
        .map_err(|_| too_large("a distance"))?;
    let distance_from = ride.distance().normalised_session().as_i64();
    let average_power = i64::from(ride.average_power().value().as_u32());
    let average_power_from = ride.average_power().normalised_session().as_i64();
    let samples_from = ride.samples().normalised_session().as_i64();
    let heart_rate_from = ride
        .heart_rate()
        .map(|series| series.normalised_session().as_i64());
    let missing = ride
        .heart_rate_declared_missing()
        .map(|missing| seconds_for_storage(missing.copied()))
        .transpose()?;
    let missing_from = ride
        .heart_rate_declared_missing()
        .map(|missing| missing.normalised_session().as_i64());

    sqlx::query!(
        r#"
        INSERT INTO canonical_bike_plus_ride (
            session, position, role,
            started_at_utc, zone, started_at_normalised_session,
            venue_reference, venue_called, venue_normalised_session,
            duration_seconds, duration_normalised_session,
            distance_millimetres, distance_normalised_session,
            average_power_watts, average_power_normalised_session,
            samples_normalised_session, heart_rate_normalised_session,
            heart_rate_declared_missing_seconds, heart_rate_declared_missing_normalised_session
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        session,
        position,
        role,
        started_at,
        zone,
        started_at_from,
        reference,
        called,
        venue_from,
        duration,
        duration_from,
        distance,
        distance_from,
        average_power,
        average_power_from,
        samples_from,
        heart_rate_from,
        missing,
        missing_from,
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;

    write_samples(tx, session, position, ride).await?;
    write_heart_rate(tx, session, position, ride).await
}

/// The four per-second series the bike produced, as the canonical ride holds
/// them.
///
/// **One statement per ride, the series handed over as a JSON array.** A row
/// per statement made rebuilding the operator's 376,000 samples and 305,000
/// beats take seventeen seconds, which every run of `next` paid; the values
/// are integers the domain has already bounded, so nothing is lost on the
/// way.
async fn write_samples(
    tx: &mut Transaction<'_, Sqlite>,
    session: i64,
    position: i64,
    ride: &CanonicalBikePlusRide,
) -> Result<(), StoreError> {
    let mut rows = Vec::with_capacity(ride.samples().value().count());
    for sample in ride.samples().value().iter() {
        rows.push(format!(
            "[{},{},{},{},{}]",
            seconds_for_storage(sample.at)?,
            sample.power.as_u32(),
            sample.cadence.as_revolutions_per_minute(),
            sample.resistance.as_percentage(),
            sample.speed.as_millimetres_per_hour(),
        ));
    }
    let rows = format!("[{}]", rows.join(","));
    sqlx::query!(
        r#"
        INSERT INTO canonical_bike_plus_ride_sample (
            session, ride_position, at_seconds, power_watts, cadence_rpm,
            resistance_percentage, speed_millimetres_per_hour
        )
        SELECT ?, ?, value ->> 0, value ->> 1, value ->> 2, value ->> 3, value ->> 4
          FROM json_each(?)
        "#,
        session,
        position,
        rows,
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;
    Ok(())
}

async fn write_heart_rate(
    tx: &mut Transaction<'_, Sqlite>,
    session: i64,
    position: i64,
    ride: &CanonicalBikePlusRide,
) -> Result<(), StoreError> {
    let Some(series) = ride.heart_rate() else {
        return Ok(());
    };
    let mut rows = Vec::with_capacity(series.value().samples().count());
    for sample in series.value().samples().iter() {
        rows.push(format!(
            "[{},{}]",
            seconds_for_storage(sample.at)?,
            sample.beats_per_minute.as_u32(),
        ));
    }
    let rows = format!("[{}]", rows.join(","));
    sqlx::query!(
        r#"
        INSERT INTO canonical_bike_plus_ride_heart_rate (
            session, ride_position, at_seconds, beats_per_minute
        )
        SELECT ?, ?, value ->> 0, value ->> 1
          FROM json_each(?)
        "#,
        session,
        position,
        rows,
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;
    Ok(())
}

/// Rides grouped into sessions, in the order the rows came, every field
/// attributed to the normalised session its row belongs to.
fn sessions_of(
    rows: Vec<RideRow>,
    series: &mut Series,
) -> Result<Vec<CanonicalCyclingSession>, StoreError> {
    let mut grouped: Vec<(i64, String, Vec<CanonicalBikePlusRide>)> = Vec::new();
    for row in rows {
        let session = row.session;
        let kind = row.kind.clone();
        let normalised = normalised_session_from_storage(session)?;
        let ride = ride_of(row, normalised, series)?;
        match grouped.last_mut() {
            Some((last, _, rides)) if *last == session => rides.push(ride),
            _ => grouped.push((session, kind, vec![ride])),
        }
    }
    grouped
        .into_iter()
        .map(|(_, kind, rides)| session_of(&kind, rides))
        .collect()
}

fn session_of(
    kind: &str,
    rides: Vec<CanonicalBikePlusRide>,
) -> Result<CanonicalCyclingSession, StoreError> {
    let mut rides = rides.into_iter();
    let mut next = |part: &str| {
        rides.next().ok_or_else(|| StoreError::Corrupt {
            detail: format!("a cycling session with no {part}"),
        })
    };
    let session = match kind {
        "ride" => CanonicalCyclingSession::Ride {
            main: next("main ride")?,
            cool_down: next("cool-down").ok(),
        },
        "test" => CanonicalCyclingSession::Test {
            warm_up: next("warm-up")?,
            effort: next("effort")?,
            cool_down: next("cool-down").ok(),
        },
        other => {
            return Err(StoreError::Corrupt {
                detail: format!("{other:?} is not a kind of cycling session"),
            });
        }
    };
    if rides.next().is_some() {
        return Err(StoreError::Corrupt {
            detail: format!("a {kind} with more rides than it has parts"),
        });
    }
    Ok(session)
}

fn ride_of(
    row: RideRow,
    id: NormalisedSessionId,
    series: &mut Series,
) -> Result<CanonicalBikePlusRide, StoreError> {
    let instant: Timestamp = row.started_at_utc.parse().map_err(|e| corrupt(&e))?;
    let zone = OperatorZone::try_from(row.zone).map_err(|e| corrupt(&e))?;
    let (Some(reference), Some(called)) = (row.venue_reference, row.venue_called) else {
        return Err(StoreError::Corrupt {
            detail: format!(
                "the ride at {} names no class; normalise peloton.rides again",
                row.started_at_utc
            ),
        });
    };
    let venue = RideVenue::new(&reference, &called).map_err(|e| corrupt(&e))?;
    let samples =
        NonEmpty::new(series.samples.remove(&row.key).unwrap_or_default()).map_err(|_| {
            StoreError::Corrupt {
                detail: format!("the ride at {} has no samples", row.started_at_utc),
            }
        })?;
    let heart_rate = series
        .heart_rate
        .remove(&row.key)
        .and_then(|beats| NonEmpty::new(beats).ok())
        .map(|beats| Attributed::new(HeartRateSeries::new(beats), id));

    Ok(CanonicalBikePlusRide::new(CanonicalRideRecord {
        started_at: Attributed::new(StartedAt::new(instant, zone), id),
        at: Attributed::new(venue, id),
        duration: Attributed::new(seconds_from_storage(row.duration_seconds)?, id),
        distance: Attributed::new(
            Metres::from_millimetres(
                u64::try_from(row.distance_millimetres).map_err(|e| corrupt(&e))?,
            ),
            id,
        ),
        average_power: Attributed::new(
            Watts::from_u32(u32::try_from(row.average_power_watts).map_err(|e| corrupt(&e))?),
            id,
        ),
        samples: Attributed::new(samples, id),
        heart_rate,
        heart_rate_declared_missing: row
            .heart_rate_declared_missing_seconds
            .map(seconds_from_storage)
            .transpose()?
            .map(|missing| Attributed::new(missing, id)),
    }))
}

fn sample(
    at: i64,
    power: i64,
    cadence: i64,
    resistance: i64,
    speed: i64,
) -> Result<RideSample, StoreError> {
    Ok(RideSample {
        at: seconds_from_storage(at)?,
        power: Watts::from_u32(u32::try_from(power).map_err(|e| corrupt(&e))?),
        cadence: Cadence::from_revolutions_per_minute(
            u32::try_from(cadence).map_err(|e| corrupt(&e))?,
        ),
        resistance: Resistance::from_percentage(u8::try_from(resistance).map_err(|e| corrupt(&e))?)
            .map_err(|e| corrupt(&e))?,
        speed: Speed::from_millimetres_per_hour(u64::try_from(speed).map_err(|e| corrupt(&e))?),
    })
}

fn beat(at: i64, beats: i64) -> Result<HeartRateSample, StoreError> {
    Ok(HeartRateSample {
        at: seconds_from_storage(at)?,
        beats_per_minute: BeatsPerMinute::new(u32::try_from(beats).map_err(|e| corrupt(&e))?)
            .map_err(|e| corrupt(&e))?,
    })
}

fn normalised_session_from_storage(id: i64) -> Result<NormalisedSessionId, StoreError> {
    NormalisedSessionId::try_from(id).map_err(|e| corrupt(&e))
}

fn seconds_from_storage(seconds: i64) -> Result<Duration, StoreError> {
    u64::try_from(seconds)
        .map(Duration::from_seconds)
        .map_err(|e| corrupt(&e))
}

fn seconds_for_storage(duration: Duration) -> Result<i64, StoreError> {
    i64::try_from(duration.as_seconds()).map_err(|_| too_large("a duration"))
}

fn too_large(what: &str) -> StoreError {
    StoreError::Corrupt {
        detail: format!("{what} larger than the store can hold"),
    }
}
