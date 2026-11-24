//! Every session of training done, read back for the analytical layer (#517).
//!
//! **Each kind from the layer that holds it**: a gym session and a cycling
//! session from their canonical layers, where several sources' accounts of
//! one visit or one ride are already one, and a padel session from the
//! normalised, which only the watch records, with the operator's change of
//! when it started and ended over it (#894).
//!
//! **A gym session no source timed adds no hours**: a sheet and the gym's log
//! state no duration, so the sessions before a watch or Hevy recorded one are
//! not counted. Every session since 2024-12-20 states one.

use application::{StoreError, TrainingDone};
use domain::{
    analytical::training_done::{Kind, Session},
    measure::PositiveDuration,
    normalised::{OperatorZone, StartedAt},
};
use jiff::civil::Date;
use sqlx::SqlitePool;

use super::{corrupt, store_error};

/// The read side of training done.
#[derive(Debug, Clone)]
pub struct SqliteTrainingDone {
    pool: SqlitePool,
}

impl SqliteTrainingDone {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// The day an instant falls on in the zone it was performed in (§ II.3).
fn day(started_at: &str, zone: String) -> Result<Date, StoreError> {
    let instant: jiff::Timestamp = started_at.parse().map_err(|error| corrupt(&error))?;
    let zone = OperatorZone::try_from(zone).map_err(|error| corrupt(&error))?;
    Ok(StartedAt::new(instant, zone).wall_clock().date())
}

fn length(seconds: i64) -> Result<PositiveDuration, StoreError> {
    let seconds = u64::try_from(seconds).map_err(|error| corrupt(&error))?;
    PositiveDuration::from_seconds(seconds).map_err(|error| corrupt(&error))
}

impl TrainingDone for SqliteTrainingDone {
    async fn sessions(&self) -> Result<Vec<Session>, StoreError> {
        let mut sessions = Vec::new();

        let gym = sqlx::query!(
            r#"
            SELECT started_at_utc, zone, on_day,
                   duration_seconds AS "duration!: i64"
            FROM canonical_gym_session
            WHERE duration_seconds IS NOT NULL
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        for row in gym {
            let on = match (row.started_at_utc, row.zone, row.on_day) {
                (Some(started_at), Some(zone), _) => day(&started_at, zone)?,
                (_, _, Some(on_day)) => on_day.parse().map_err(|error| corrupt(&error))?,
                _ => {
                    return Err(corrupt(
                        &"a canonical gym session with neither an instant nor a day",
                    ));
                }
            };
            sessions.push(Session {
                on,
                kind: Kind::Gym,
                length: length(row.duration)?,
            });
        }

        let rides = sqlx::query!(
            r#"
            SELECT started_at_utc AS "started_at!: String",
                   zone AS "zone!: String",
                   duration_seconds AS "duration!: i64"
            FROM canonical_bike_plus_ride
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        for row in rides {
            sessions.push(Session {
                on: day(&row.started_at, row.zone)?,
                kind: Kind::Cycling,
                length: length(row.duration)?,
            });
        }

        for played in super::SqlitePadelSessions::new(self.pool.clone())
            .every()
            .await?
        {
            sessions.push(Session {
                on: played.started_at().wall_clock().date(),
                kind: Kind::Padel,
                length: played.duration(),
            });
        }

        sessions.sort_by_key(|session| (session.on, session.kind));
        Ok(sessions)
    }
}

/// The canonical gym sessions and rides, and the padel sessions (#809).
impl application::Input for SqliteTrainingDone {
    fn reads() -> application::Reads {
        application::Reads::of([application::Node::Canonical(
            application::CanonicalLayer::Gym,
        )])
        .and(application::Reads::of([application::Node::Canonical(
            application::CanonicalLayer::Cycling,
        )]))
        .and(<super::SqlitePadelSessions as application::Input>::reads())
    }
}

impl application::NotRaw for SqliteTrainingDone {}
