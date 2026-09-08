//! The cycling mesocycles of an authored plan (§ 12).
//!
//! Written once and kept, superseded by the plan's `authored_at` exactly as the
//! gym's is.
//! Nothing regenerates it: the zone plan came off a source that need not be
//! reachable again, and re-fetching forty classes to answer what Wednesday's
//! ride is would make § 36 a promise this command could not keep.
//!
//! **Six tables and no template column.** A cycling mesocycle is microcycles of
//! rides; there is no primary lift, no anchor and no gating role, so there is
//! nothing for a `CHECK` on a template to admit or refuse. It is not a fifth
//! `gym_mesocycle.template` because the two disciplines' mesocycles are
//! different things, not two versions of one (decision 0026).
//!
//! **Identity is the plan's** (issue #86). A mesocycle has no name and no
//! authoring time of its own: it is the *n*th cycling mesocycle of a plan, and
//! what it does carry is the published programme it was taken from.
//!
//! **Reading rebuilds through the domain's constructors**, as the gym's store
//! does: a row edited by hand should be caught rather than trusted, and the one
//! rule the parts cannot enforce between them — every microcycle holding every
//! session the weekday map rides — is re-checked on the way out.

use std::collections::BTreeMap;

use application::{CyclingMesocycleStore, StoreError};
use domain::{
    cycling::{
        CyclingMesocycle, CyclingMesocycleId, CyclingMicrocycle, CyclingWeekdays, Interval,
        PlannedRide, PowerZone, Ride, RideVenue, SessionPosition,
    },
    measure::PositiveDuration,
    plan::{Occupies, PlanName},
    provider::{ExternalProgramme, ProgrammeName, Provider},
    sequence::NonEmpty,
};
use jiff::civil::Date;
use sqlx::SqlitePool;

use super::{
    corrupt,
    gym_mesocycle::{weekday_key, weekday_of},
    store_error,
};

/// The authored cycling side, in SQLite.
pub struct SqliteCyclingMesocycleStore {
    pool: SqlitePool,
}

impl SqliteCyclingMesocycleStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// Every cycling mesocycle of every plan in force, earliest start first.
///
/// **One query for two readers**, for the reason `gym_mesocycle::in_force` gives:
/// the plan store groups these by plan and the mesocycle store reads them flat.
pub(super) async fn in_force(
    pool: &SqlitePool,
) -> Result<Vec<(i64, PlanName, CyclingMesocycleId, CyclingMesocycle)>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT m.id AS "id!: i64", m.plan AS "plan!: i64",
               pl.name AS "plan_name!: String",
               m.provider AS "provider!: String",
               m.provided_programme AS "provided_programme!: String",
               m.start_date AS "start_date!: String"
        FROM cycling_mesocycle AS m
        JOIN plan AS pl ON pl.id = m.plan
        WHERE pl.authored_at = (
            SELECT MAX(q.authored_at) FROM plan AS q WHERE q.name = pl.name
        )
        ORDER BY m.start_date ASC, m.id ASC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut mesocycles = Vec::with_capacity(rows.len());
    for row in rows {
        let plan = PlanName::try_from(row.plan_name).map_err(|error| corrupt(&error))?;
        let published = ExternalProgramme::new(
            Provider::try_from(row.provider).map_err(|error| corrupt(&error))?,
            ProgrammeName::try_from(row.provided_programme).map_err(|error| corrupt(&error))?,
        );
        let start = row
            .start_date
            .parse::<Date>()
            .map_err(|_| corrupt(&"a start date that is not a date"))?;

        let microcycles = read_microcycles(pool, row.id).await?;
        let microcycles = NonEmpty::new(microcycles)
            .map_err(|_| corrupt(&"a cycling mesocycle with no microcycle in it"))?;
        let weekdays = read_weekdays(pool, row.id).await?;

        let mesocycle = CyclingMesocycle::new(published, start, microcycles, weekdays)
            .map_err(|error| corrupt(&error))?;
        mesocycles.push((row.plan, plan, CyclingMesocycleId::new(row.id), mesocycle));
    }
    Ok(mesocycles)
}

async fn read_weekdays(pool: &SqlitePool, id: i64) -> Result<CyclingWeekdays, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT weekday AS "weekday!: String", session AS "session!: i64"
        FROM cycling_weekday
        WHERE mesocycle = ?
        ORDER BY session
        "#,
        id
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut days = Vec::with_capacity(rows.len());
    for row in rows {
        days.push((weekday_of(&row.weekday)?, position_of(row.session)?));
    }
    CyclingWeekdays::new(days).map_err(|error| corrupt(&error))
}

async fn read_microcycles(
    pool: &SqlitePool,
    id: i64,
) -> Result<Vec<CyclingMicrocycle>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT ordinal AS "ordinal!: i64",
               published_ordinal AS "published_ordinal!: i64"
        FROM cycling_microcycle
        WHERE mesocycle = ?
        ORDER BY ordinal
        "#,
        id
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut microcycles = Vec::with_capacity(rows.len());
    for row in rows {
        let number = u32::try_from(row.published_ordinal)
            .map_err(|_| corrupt(&"a published microcycle number the domain cannot hold"))?;
        let rides = read_rides(pool, id, row.ordinal).await?;
        microcycles.push(CyclingMicrocycle::new(rides, number).map_err(|error| corrupt(&error))?);
    }
    Ok(microcycles)
}

async fn read_rides(
    pool: &SqlitePool,
    id: i64,
    microcycle: i64,
) -> Result<BTreeMap<SessionPosition, PlannedRide>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT session AS "session!: i64",
               published_session AS "published_session!: i64",
               warm_up_seconds AS "warm_up_seconds!: i64",
               cool_down_seconds AS "cool_down_seconds: i64",
               effort_seconds AS "effort_seconds: i64"
        FROM cycling_ride
        WHERE mesocycle = ? AND microcycle = ?
        ORDER BY session
        "#,
        id,
        microcycle
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut rides = BTreeMap::new();
    for row in rows {
        let warm_up = duration_of(row.warm_up_seconds)?;
        let cool_down = row.cool_down_seconds.map(duration_of).transpose()?;
        let intervals = read_intervals(pool, id, microcycle, row.session).await?;

        // A ride is intervals or an effort, and never both. The `CHECK` can see
        // one half of that and this is the other: only a reader can count the
        // rows in another table.
        let ride = match (row.effort_seconds, intervals.is_empty()) {
            (Some(seconds), true) => Ride::Effort(duration_of(seconds)?),
            (None, false) => {
                Ride::Intervals(NonEmpty::new(intervals).map_err(|error| corrupt(&error))?)
            }
            (Some(_), false) => {
                return Err(corrupt(&"a ride stored as both an effort and a zone plan"));
            }
            (None, true) => {
                return Err(corrupt(&"a ride that instructs nothing"));
            }
        };

        let published = u32::try_from(row.published_session)
            .map_err(|_| corrupt(&"a published session number the domain cannot hold"))?;
        rides.insert(
            position_of(row.session)?,
            PlannedRide::new(
                domain::cycling::CyclingSession::new(warm_up, ride, cool_down),
                read_venues(pool, id, microcycle, row.session).await?,
                published,
            ),
        );
    }
    Ok(rides)
}

async fn read_intervals(
    pool: &SqlitePool,
    id: i64,
    microcycle: i64,
    session: i64,
) -> Result<Vec<Interval>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT zone AS "zone!: i64", seconds AS "seconds!: i64"
        FROM cycling_interval
        WHERE mesocycle = ? AND microcycle = ? AND session = ?
        ORDER BY position
        "#,
        id,
        microcycle,
        session
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    rows.into_iter()
        .map(|row| {
            let zone = u8::try_from(row.zone)
                .ok()
                .and_then(|number| PowerZone::try_from(number).ok())
                .ok_or_else(|| corrupt(&"an interval at no power zone"))?;
            Ok(Interval::new(zone, duration_of(row.seconds)?))
        })
        .collect()
}

async fn read_venues(
    pool: &SqlitePool,
    id: i64,
    microcycle: i64,
    session: i64,
) -> Result<NonEmpty<RideVenue>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT reference AS "reference!: String", called AS "called!: String"
        FROM cycling_venue
        WHERE mesocycle = ? AND microcycle = ? AND session = ?
        ORDER BY position
        "#,
        id,
        microcycle,
        session
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let venues = rows
        .into_iter()
        .map(|row| RideVenue::new(&row.reference, &row.called).map_err(|error| corrupt(&error)))
        .collect::<Result<Vec<_>, StoreError>>()?;
    NonEmpty::new(venues).map_err(|_| corrupt(&"a ride with nowhere to do it"))
}

fn position_of(session: i64) -> Result<SessionPosition, StoreError> {
    u8::try_from(session)
        .ok()
        .and_then(|number| SessionPosition::new(number).ok())
        .ok_or_else(|| corrupt(&"a session position the domain cannot hold"))
}

fn duration_of(seconds: i64) -> Result<PositiveDuration, StoreError> {
    u64::try_from(seconds)
        .map_err(|_| corrupt(&"a duration stored as a negative number of seconds"))
        .and_then(|seconds| {
            PositiveDuration::from_seconds(seconds).map_err(|error| corrupt(&error))
        })
}

/// Seconds on their way into the store.
///
/// SQLite holds signed 64-bit integers; a duration that will not fit is a
/// programme this store cannot hold, and saying so is better than truncating.
fn seconds_of(duration: PositiveDuration) -> Result<i64, StoreError> {
    i64::try_from(duration.as_seconds())
        .map_err(|_| corrupt(&"a duration too long for the store to hold"))
}

impl CyclingMesocycleStore for SqliteCyclingMesocycleStore {
    async fn on(
        &self,
        date: Date,
    ) -> Result<Option<(CyclingMesocycleId, PlanName, CyclingMesocycle)>, StoreError> {
        Ok(in_force(&self.pool)
            .await?
            .into_iter()
            .find(|(_, _, _, mesocycle)| mesocycle.span().covers(date))
            .map(|(_, plan, id, mesocycle)| (id, plan, mesocycle)))
    }

    async fn following(
        &self,
        date: Date,
    ) -> Result<Option<(CyclingMesocycleId, PlanName, CyclingMesocycle)>, StoreError> {
        // Ordered by start, so the first one beginning after the date is the
        // next in the sequence.
        Ok(in_force(&self.pool)
            .await?
            .into_iter()
            .find(|(_, _, _, mesocycle)| mesocycle.start() > date)
            .map(|(_, plan, id, mesocycle)| (id, plan, mesocycle)))
    }
}

/// Write one cycling mesocycle of a plan, inside the plan's own transaction.
///
/// **Not a port method**, for the reason `gym_mesocycle::write` gives: the plan
/// is what is authored, and a half-written one is not a state the store should
/// be able to hold.
pub(super) async fn write(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    plan: i64,
    ordinal: i64,
    mesocycle: &CyclingMesocycle,
) -> Result<CyclingMesocycleId, StoreError> {
    let provider = mesocycle.programme().provider().to_string();
    let programme = mesocycle.programme().name().to_string();
    let start = mesocycle.start().to_string();

    let id = sqlx::query!(
        r#"
        INSERT INTO cycling_mesocycle (
            plan, ordinal, provider, provided_programme, start_date
        )
        VALUES (?, ?, ?, ?, ?)
        RETURNING id AS "id!: i64"
        "#,
        plan,
        ordinal,
        provider,
        programme,
        start
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?
    .id;

    for (weekday, position) in mesocycle.weekdays().days() {
        let key = weekday_key(*weekday);
        let session = i64::from(position.as_u8());
        sqlx::query!(
            r"
            INSERT INTO cycling_weekday (mesocycle, weekday, session)
            VALUES (?, ?, ?)
            ",
            id,
            key,
            session
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    }

    for (at, microcycle) in mesocycle.microcycles().iter().enumerate() {
        let week = i64::try_from(at + 1)
            .map_err(|_| corrupt(&"more microcycles than the store can number"))?;
        let published_ordinal = i64::from(microcycle.published_ordinal());
        sqlx::query!(
            r"
            INSERT INTO cycling_microcycle (mesocycle, ordinal, published_ordinal)
            VALUES (?, ?, ?)
            ",
            id,
            week,
            published_ordinal
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;

        for (position, planned) in microcycle.rides() {
            write_ride(tx, id, week, *position, planned).await?;
        }
    }

    Ok(CyclingMesocycleId::new(id))
}

async fn write_ride(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    mesocycle: i64,
    microcycle: i64,
    position: SessionPosition,
    planned: &PlannedRide,
) -> Result<(), StoreError> {
    let session = i64::from(position.as_u8());
    let ride = planned.session();
    let warm_up = seconds_of(ride.warm_up())?;
    let cool_down = ride.cool_down().map(seconds_of).transpose()?;
    let effort = match ride.ride() {
        Ride::Effort(duration) => Some(seconds_of(*duration)?),
        Ride::Intervals(_) => None,
    };

    let published = i64::from(planned.published_session());
    sqlx::query!(
        r"
        INSERT INTO cycling_ride (
            mesocycle, microcycle, session, published_session,
            warm_up_seconds, cool_down_seconds, effort_seconds
        )
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ",
        mesocycle,
        microcycle,
        session,
        published,
        warm_up,
        cool_down,
        effort
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;

    for (at, venue) in planned.at().iter().enumerate() {
        let index =
            i64::try_from(at).map_err(|_| corrupt(&"more places than the store can number"))?;
        let reference = venue.reference();
        let called = venue.called();
        sqlx::query!(
            r"
            INSERT INTO cycling_venue (
                mesocycle, microcycle, session, position, reference, called
            )
            VALUES (?, ?, ?, ?, ?, ?)
            ",
            mesocycle,
            microcycle,
            session,
            index,
            reference,
            called
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    }

    if let Ride::Intervals(intervals) = ride.ride() {
        for (at, interval) in intervals.iter().enumerate() {
            let index = i64::try_from(at)
                .map_err(|_| corrupt(&"more intervals than the store can number"))?;
            let zone = i64::from(interval.zone().as_u8());
            let seconds = seconds_of(interval.duration())?;
            sqlx::query!(
                r"
                INSERT INTO cycling_interval (
                    mesocycle, microcycle, session, position, zone, seconds
                )
                VALUES (?, ?, ?, ?, ?, ?)
                ",
                mesocycle,
                microcycle,
                session,
                index,
                zone,
                seconds
            )
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error(&error))?;
        }
    }

    Ok(())
}
