//! The canonical layer for gym sessions: one row per visit, and one row per
//! part of it naming the normalised session that part comes from (§ II.4).
//!
//! **Two tables of its own rather than a column on `gym_session`.** A
//! normalised session belongs to one source and a canonical one belongs to
//! none, so a `canonical` column on the normalised table would be the join
//! stored on the wrong side of it — and there would be nowhere to say that a
//! watch supplies a session's heart rate but not its exercises.
//!
//! **Replaced whole, and nothing here is append-only.** A derivation is never
//! mutated in place (§ II), and a rebuild of the normalised layer reassigns
//! every `gym_session.id` these rows point at, so the canonical layer is
//! rebuilt after it rather than patched.

use application::{CanonicalGymSessionStore, StoreError};
use domain::{
    canonical::{NormalisedSessionId, Occurred, SessionCount},
    gym::{CanonicalGymSession, Part},
    normalised::{OperatorZone, StartedAt},
    sequence::NonEmpty,
};
use jiff::civil::Date;
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::{corrupt, count_for_storage, count_from_storage, store_error};

/// What a part is called in the store. The same two words the domain's
/// [`Part::as_str`] uses, so a row reads as the type does.
const EXERCISES: &str = "exercises";
const HEART_RATE: &str = "heart rate";

#[derive(Debug, Clone)]
pub struct SqliteCanonicalGymSessionStore {
    pool: SqlitePool,
}

impl SqliteCanonicalGymSessionStore {
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl CanonicalGymSessionStore for SqliteCanonicalGymSessionStore {
    async fn replace(
        &self,
        sessions: Vec<CanonicalGymSession>,
    ) -> Result<SessionCount, StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        clear(&mut tx).await?;

        let written = sessions.len();
        for session in &sessions {
            write_session(&mut tx, session).await?;
        }

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(SessionCount::from(written))
    }

    async fn all(&self) -> Result<Vec<CanonicalGymSession>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT s.id           AS "id!: i64",
                   s.started_at_utc AS "started_at_utc: String",
                   s.zone         AS "zone: String",
                   s.on_day       AS "on_day: String",
                   p.part         AS "part!: String",
                   p.normalised   AS "normalised!: i64"
            FROM canonical_gym_session AS s
            JOIN canonical_gym_session_part AS p ON p.session = s.id
            ORDER BY COALESCE(date(datetime(s.started_at_utc)), s.on_day),
                     s.started_at_utc, s.id, p.position
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut sessions: Vec<(i64, Occurred, Vec<Part>)> = Vec::new();
        for row in rows {
            let part = part_from_row(&row.part, row.normalised)?;
            match sessions.last_mut() {
                Some((id, _, parts)) if *id == row.id => parts.push(part),
                _ => {
                    let occurred = occurred_from_row(
                        row.started_at_utc.as_deref(),
                        row.zone.as_deref(),
                        row.on_day.as_deref(),
                    )?;
                    sessions.push((row.id, occurred, vec![part]));
                }
            }
        }

        sessions
            .into_iter()
            .map(|(id, occurred, parts)| {
                let parts = NonEmpty::new(parts).map_err(|_| StoreError::Corrupt {
                    detail: format!("canonical gym session {id} stands on nothing"),
                })?;
                Ok(CanonicalGymSession::new(occurred, parts))
            })
            .collect()
    }

    async fn count(&self) -> Result<SessionCount, StoreError> {
        let row = sqlx::query!(r#"SELECT COUNT(*) AS "count: i64" FROM canonical_gym_session"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;

        Ok(SessionCount::from(count_from_storage(Some(row.count))?))
    }
}

/// Parts first, because they point at the sessions.
async fn clear(tx: &mut Transaction<'_, Sqlite>) -> Result<(), StoreError> {
    sqlx::query!("DELETE FROM canonical_gym_session_part")
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    sqlx::query!("DELETE FROM canonical_gym_session")
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    Ok(())
}

async fn write_session(
    tx: &mut Transaction<'_, Sqlite>,
    session: &CanonicalGymSession,
) -> Result<(), StoreError> {
    let (started_at, zone, on_day) = match session.occurred() {
        Occurred::At(started_at) => (
            Some(started_at.instant().to_string()),
            Some(started_at.zone().id().to_owned()),
            None,
        ),
        Occurred::On(day) => (None, None, Some(day.to_string())),
    };

    let row = sqlx::query!(
        r#"
        INSERT INTO canonical_gym_session (started_at_utc, zone, on_day)
        VALUES (?, ?, ?)
        RETURNING id AS "id!: i64"
        "#,
        started_at,
        zone,
        on_day
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;

    for (position, part) in session.parts().iter().enumerate() {
        let position = count_for_storage(position)?;
        let name = part.as_str();
        let normalised = part.from().as_i64();

        sqlx::query!(
            r#"
            INSERT INTO canonical_gym_session_part (session, position, part, normalised)
            VALUES (?, ?, ?, ?)
            "#,
            row.id,
            position,
            name,
            normalised
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    }

    Ok(())
}

fn part_from_row(name: &str, normalised: i64) -> Result<Part, StoreError> {
    let session = NormalisedSessionId::try_from(normalised).map_err(|error| corrupt(&error))?;
    match name {
        EXERCISES => Ok(Part::Exercises(session)),
        HEART_RATE => Ok(Part::HeartRate(session)),
        other => Err(StoreError::Corrupt {
            detail: format!("{other:?} is not a part of a gym session"),
        }),
    }
}

/// The stored instant and zone, or the stored day. The table's checks make
/// exactly one of the two present, so a row with neither is a file this
/// program did not write.
fn occurred_from_row(
    started_at_utc: Option<&str>,
    zone: Option<&str>,
    on_day: Option<&str>,
) -> Result<Occurred, StoreError> {
    match (started_at_utc, zone, on_day) {
        (Some(instant), Some(zone), None) => {
            let instant: jiff::Timestamp = instant.parse().map_err(|_| StoreError::Corrupt {
                detail: format!("{instant:?} is not an instant"),
            })?;
            let zone = OperatorZone::try_from(zone).map_err(|error| corrupt(&error))?;
            Ok(Occurred::At(StartedAt::new(instant, zone)))
        }
        (None, None, Some(day)) => {
            let day = day.parse::<Date>().map_err(|_| StoreError::Corrupt {
                detail: format!("{day:?} is not a date"),
            })?;
            Ok(Occurred::On(day))
        }
        _ => Err(StoreError::Corrupt {
            detail: "a canonical gym session with neither an instant nor a day".to_owned(),
        }),
    }
}
