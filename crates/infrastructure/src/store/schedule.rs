//! The operator's week, and the holidays that depart from it (§ 12).
//!
//! **Authored data, so nothing regenerates it.** A week is a fact about a life
//! rather than something derivable from the record: the record shows when the
//! operator *did* train, which is not the same as when they could have.
//!
//! **Two shapes read as one `Diary`.** Schedules and patches are stored apart
//! because a departure is a fact about dates rather than about which ordinary
//! week was in force when it was recorded. Only `Diary` relates them, and it does so by
//! date — so this module assembles both and resolves nothing.
//!
//! **A week is superseded by a later one existing.** There is no flag and no end
//! date: `Diary::on` takes the last schedule whose date has arrived. An end
//! column would be a second place for the same fact and the two could disagree,
//! which is the reasoning the generation parameters beside this are stored under.

use std::collections::BTreeSet;

use application::{DiaryAuthor, DiaryStore, StoreError};
use domain::{
    gym::OperatorZone,
    schedule::{Diary, PartOfDay, Patch, Schedule, Slot},
};
use jiff::{Timestamp, civil::Date};
use sqlx::SqlitePool;

use super::{
    corrupt,
    programme::{weekday_key, weekday_of},
    store_error,
};

/// The diary, in SQLite.
pub struct SqliteDiaryStore {
    pool: SqlitePool,
}

impl SqliteDiaryStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn date_of(text: &str) -> Result<Date, StoreError> {
    text.parse()
        .map_err(|_| corrupt(&format!("{text:?} is not a date")))
}

fn part_of(text: &str) -> Result<PartOfDay, StoreError> {
    PartOfDay::try_from(text.to_owned()).map_err(|error| corrupt(&error))
}

fn zone_of(text: &str) -> Result<OperatorZone, StoreError> {
    OperatorZone::try_from(text.to_owned()).map_err(|error| corrupt(&error))
}

fn slot_of(weekday: &str, part: &str) -> Result<Slot, StoreError> {
    Ok(Slot::new(weekday_of(weekday)?, part_of(part)?))
}

impl DiaryStore for SqliteDiaryStore {
    async fn diary(&self) -> Result<Diary, StoreError> {
        let weeks = sqlx::query!(
            r"
            SELECT id, from_date, zone
            FROM schedule
            ORDER BY from_date
            "
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut schedules = Vec::with_capacity(weeks.len());
        for week in weeks {
            let slots = sqlx::query!(
                r"
                SELECT weekday, part
                FROM schedule_slot
                WHERE schedule = ?
                ",
                week.id
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;

            let slots = slots
                .iter()
                .map(|row| slot_of(&row.weekday, &row.part))
                .collect::<Result<BTreeSet<_>, _>>()?;

            schedules.push(Schedule::new(
                date_of(&week.from_date)?,
                zone_of(&week.zone)?,
                slots,
            ));
        }

        let booked = sqlx::query!(
            r"
            SELECT id, start_date, days, zone, states_slots, reason
            FROM schedule_patch
            ORDER BY start_date
            "
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut patches = Vec::with_capacity(booked.len());
        for patch in booked {
            let days = u8::try_from(patch.days)
                .ok()
                .and_then(std::num::NonZeroU8::new)
                .ok_or_else(|| corrupt(&"a patch covering no days"))?;

            // Absent is "the ordinary week stands"; present-but-empty is "no
            // room at all". Both are zero rows, so the flag is what tells them
            // apart — see migration 0020.
            let slots = if patch.states_slots == 0 {
                None
            } else {
                let rows = sqlx::query!(
                    r"
                    SELECT weekday, part
                    FROM schedule_patch_slot
                    WHERE patch = ?
                    ",
                    patch.id
                )
                .fetch_all(&self.pool)
                .await
                .map_err(|error| store_error(&error))?;

                Some(
                    rows.iter()
                        .map(|row| slot_of(&row.weekday, &row.part))
                        .collect::<Result<BTreeSet<_>, _>>()?,
                )
            };

            let zone = patch.zone.as_deref().map(zone_of).transpose()?;

            patches.push(Patch::new(
                date_of(&patch.start_date)?,
                days,
                zone,
                slots,
                patch.reason,
            ));
        }

        Ok(Diary::new(schedules, patches))
    }
}

impl DiaryAuthor for SqliteDiaryStore {
    async fn record_week(&self, schedule: &Schedule) -> Result<(), StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        let authored_at = Timestamp::now().to_string();
        let from = schedule.from().to_string();
        let zone = schedule.zone().id().to_owned();

        // Re-stating the week in force from a date corrects it rather than
        // adding a second one that starts the same day, which `Diary` could not
        // order. Succession is a *later* date, not another row on the same one.
        let id = sqlx::query!(
            r"
            INSERT INTO schedule (authored_at, from_date, zone)
            VALUES (?, ?, ?)
            ON CONFLICT (from_date) DO UPDATE
                SET authored_at = excluded.authored_at,
                    zone        = excluded.zone
            RETURNING id
            ",
            authored_at,
            from,
            zone
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?
        .id;

        sqlx::query!("DELETE FROM schedule_slot WHERE schedule = ?", id)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;

        for slot in schedule.slots() {
            let weekday = weekday_key(slot.weekday);
            let part = slot.part.as_str();
            sqlx::query!(
                r"
                INSERT INTO schedule_slot (schedule, weekday, part)
                VALUES (?, ?, ?)
                ",
                id,
                weekday,
                part
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        tx.commit().await.map_err(|error| store_error(&error))
    }

    async fn record_patch(&self, patch: &Patch) -> Result<(), StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        let authored_at = Timestamp::now().to_string();
        let start = patch.start().to_string();
        let days = i64::from(patch.days().get());
        let zone = patch.zone().map(|zone| zone.id().to_owned());
        let states_slots = i64::from(patch.slots().is_some());
        let reason = patch.reason().to_owned();

        let id = sqlx::query!(
            r"
            INSERT INTO schedule_patch (
                authored_at, start_date, days, zone, states_slots, reason
            )
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT (start_date) DO UPDATE
                SET authored_at  = excluded.authored_at,
                    days         = excluded.days,
                    zone         = excluded.zone,
                    states_slots = excluded.states_slots,
                    reason       = excluded.reason
            RETURNING id
            ",
            authored_at,
            start,
            days,
            zone,
            states_slots,
            reason
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?
        .id;

        sqlx::query!("DELETE FROM schedule_patch_slot WHERE patch = ?", id)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;

        for slot in patch.slots().into_iter().flatten() {
            let weekday = weekday_key(slot.weekday);
            let part = slot.part.as_str();
            sqlx::query!(
                r"
                INSERT INTO schedule_patch_slot (patch, weekday, part)
                VALUES (?, ?, ?)
                ",
                id,
                weekday,
                part
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        tx.commit().await.map_err(|error| store_error(&error))
    }
}
