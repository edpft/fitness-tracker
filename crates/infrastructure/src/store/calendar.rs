//! The school and public holidays as their publishers last served them (#809).
//!
//! **The body as served, read when it is read**, so what the holidays are is
//! decided in one place, the adapter that reads a feed, whether it was served
//! a moment ago or a term ago.

use application::{
    Calendar, CalendarStore, HolidayCalendar, Kept, Served, SourceError, StoreError,
};
use domain::schedule::Holidays;
use jiff::Timestamp;
use sqlx::SqlitePool;

use super::store_error;
use crate::calendar::{public::public_holidays, school::school_holidays};

/// Each calendar, kept.
#[derive(Debug, Clone)]
pub struct SqliteCalendarStore {
    pool: SqlitePool,
}

impl SqliteCalendarStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl CalendarStore for SqliteCalendarStore {
    async fn kept(&self, calendar: Calendar) -> Result<Option<Kept>, StoreError> {
        let name = calendar.as_str();
        let row = sqlx::query!(
            r#"
            SELECT served AS "served!: String",
                   etag,
                   last_modified,
                   asked_at AS "asked_at!: String"
              FROM calendar
             WHERE name = ?
            "#,
            name
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        row.map(|row| {
            Ok(Kept {
                served: Served {
                    body: row.served,
                    etag: row.etag,
                    last_modified: row.last_modified,
                },
                asked: row.asked_at.parse().map_err(|_| StoreError::Corrupt {
                    detail: format!(
                        "the {name} calendar was asked at {:?}, which is not a moment",
                        row.asked_at
                    ),
                })?,
            })
        })
        .transpose()
    }

    async fn keep(
        &self,
        calendar: Calendar,
        served: &Served,
        asked: Timestamp,
    ) -> Result<(), StoreError> {
        let name = calendar.as_str();
        let asked = asked.to_string();
        sqlx::query!(
            r#"
            INSERT INTO calendar (name, served, etag, last_modified, asked_at)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT (name) DO UPDATE
               SET served = excluded.served,
                   etag = excluded.etag,
                   last_modified = excluded.last_modified,
                   asked_at = excluded.asked_at
            "#,
            name,
            served.body,
            served.etag,
            served.last_modified,
            asked
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(())
    }

    async fn unchanged(&self, calendar: Calendar, asked: Timestamp) -> Result<(), StoreError> {
        let name = calendar.as_str();
        let asked = asked.to_string();
        sqlx::query!(
            "UPDATE calendar SET asked_at = ? WHERE name = ?",
            asked,
            name
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(())
    }
}

/// Both calendars, as last served.
///
/// **None served yet is no data, not no holidays**: a store no run has asked
/// a publisher for answers as a calendar that cannot be read does.
impl HolidayCalendar for SqliteCalendarStore {
    async fn holidays(&self) -> Result<Holidays, SourceError> {
        let body = |calendar: Calendar| async move {
            self.kept(calendar)
                .await
                .map_err(|error| SourceError::Unavailable {
                    detail: error.to_string(),
                })?
                .map(|kept| kept.served.body)
                .ok_or_else(|| SourceError::Unavailable {
                    detail: format!(
                        "the {} calendar has not been fetched yet",
                        calendar.as_str()
                    ),
                })
        };
        let school = school_holidays(&body(Calendar::School).await?)?;
        let public = public_holidays(&body(Calendar::Public).await?)?;
        Ok(school.and(public))
    }
}

/// What a page reads it through reads the calendars (#809).
impl application::Input for SqliteCalendarStore {
    fn reads() -> application::Reads {
        Calendar::ALL
            .map(|calendar| {
                application::Node::Generation(application::Generation::Calendar(calendar))
            })
            .into_iter()
            .collect()
    }
}
