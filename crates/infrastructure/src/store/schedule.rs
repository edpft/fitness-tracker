//! When the operator has room to train, and what departs from it (§ 12).
//!
//! **Authored data, so nothing regenerates it.** A pattern is a fact about a life
//! rather than something derivable from the record: the record shows when the
//! operator *did* train, which is not the same as when they could have.
//!
//! **Two shapes read as one `Diary`.** Schedules and alterations are stored apart
//! because a departure is a fact about dates rather than about which ordinary
//! week was in force when it was recorded. Only `Diary` relates them, and it does so by
//! date — so this module assembles both and resolves nothing.
//!
//! **A pattern is superseded by a later one existing.** There is no flag and no end
//! date: `Diary::on` takes the last schedule whose date has arrived. An end
//! column would be a second place for the same fact and the two could disagree,
//! which is the reasoning the generation parameters beside this are stored under.

use std::collections::BTreeMap;

use application::{DiaryAuthor, DiaryStore, StoreError};
use domain::{
    gym::OperatorZone,
    schedule::{Alteration, Diary, Discipline, PartOfDay, TrainingPattern, TrainingSlot},
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

fn slot_of(weekday: &str, part: &str) -> Result<TrainingSlot, StoreError> {
    Ok(TrainingSlot::new(weekday_of(weekday)?, part_of(part)?))
}

fn discipline_of(text: &str) -> Result<Discipline, StoreError> {
    Discipline::try_from(text.to_owned()).map_err(|error| corrupt(&error))
}

impl DiaryStore for SqliteDiaryStore {
    async fn diary(&self) -> Result<Diary, StoreError> {
        let weeks = sqlx::query!(
            r"
            SELECT id, from_date, zone
            FROM training_pattern
            ORDER BY from_date
            "
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut patterns = Vec::with_capacity(weeks.len());
        for week in weeks {
            let slots = sqlx::query!(
                r"
                SELECT weekday, part, discipline
                FROM training_slot
                WHERE pattern = ?
                ",
                week.id
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;

            let slots = slots
                .iter()
                .map(|row| {
                    Ok((
                        slot_of(&row.weekday, &row.part)?,
                        discipline_of(&row.discipline)?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>, StoreError>>()?;

            patterns.push(TrainingPattern::new(
                date_of(&week.from_date)?,
                zone_of(&week.zone)?,
                slots,
            ));
        }

        let booked = sqlx::query!(
            r"
            SELECT id, start_date, days, zone, states_slots, reason
            FROM alteration
            ORDER BY start_date
            "
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut alterations = Vec::with_capacity(booked.len());
        for alteration in booked {
            let days = u8::try_from(alteration.days)
                .ok()
                .and_then(std::num::NonZeroU8::new)
                .ok_or_else(|| corrupt(&"an alteration covering no days"))?;

            // Absent is "the ordinary pattern stands"; present-but-empty is "no
            // room at all". Both are zero rows, so the flag is what tells them
            // apart — see migration 0020.
            let slots = if alteration.states_slots == 0 {
                None
            } else {
                let rows = sqlx::query!(
                    r"
                    SELECT weekday, part, discipline
                    FROM alteration_slot
                    WHERE alteration = ?
                    ",
                    alteration.id
                )
                .fetch_all(&self.pool)
                .await
                .map_err(|error| store_error(&error))?;

                Some(
                    rows.iter()
                        .map(|row| {
                            Ok((
                                slot_of(&row.weekday, &row.part)?,
                                discipline_of(&row.discipline)?,
                            ))
                        })
                        .collect::<Result<BTreeMap<_, _>, StoreError>>()?,
                )
            };

            let zone = alteration.zone.as_deref().map(zone_of).transpose()?;

            alterations.push(Alteration::new(
                date_of(&alteration.start_date)?,
                days,
                zone,
                slots,
                alteration.reason,
            ));
        }

        Ok(Diary::new(patterns, alterations))
    }
}

impl DiaryAuthor for SqliteDiaryStore {
    async fn record_pattern(&self, pattern: &TrainingPattern) -> Result<(), StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        let authored_at = Timestamp::now().to_string();
        let from = pattern.from().to_string();
        let zone = pattern.zone().id().to_owned();

        // Re-stating the week in force from a date corrects it rather than
        // adding a second one that starts the same day, which `Diary` could not
        // order. Succession is a *later* date, not another row on the same one.
        let id = sqlx::query!(
            r"
            INSERT INTO training_pattern (authored_at, from_date, zone)
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

        sqlx::query!("DELETE FROM training_slot WHERE pattern = ?", id)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;

        for (slot, discipline) in pattern.slots() {
            let weekday = weekday_key(slot.weekday);
            let part = slot.part.as_str();
            let discipline = discipline.as_str();
            sqlx::query!(
                r"
                INSERT INTO training_slot (pattern, weekday, part, discipline)
                VALUES (?, ?, ?, ?)
                ",
                id,
                weekday,
                part,
                discipline
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        tx.commit().await.map_err(|error| store_error(&error))
    }

    async fn record_alteration(&self, alteration: &Alteration) -> Result<(), StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        let authored_at = Timestamp::now().to_string();
        let start = alteration.start().to_string();
        let days = i64::from(alteration.days().get());
        let zone = alteration.zone().map(|zone| zone.id().to_owned());
        let states_slots = i64::from(alteration.slots().is_some());
        let reason = alteration.reason().to_owned();

        let id = sqlx::query!(
            r"
            INSERT INTO alteration (
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

        sqlx::query!("DELETE FROM alteration_slot WHERE alteration = ?", id)
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;

        for (slot, discipline) in alteration.slots().into_iter().flatten() {
            let weekday = weekday_key(slot.weekday);
            let part = slot.part.as_str();
            let discipline = discipline.as_str();
            sqlx::query!(
                r"
                INSERT INTO alteration_slot (alteration, weekday, part, discipline)
                VALUES (?, ?, ?, ?)
                ",
                id,
                weekday,
                part,
                discipline
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        tx.commit().await.map_err(|error| store_error(&error))
    }
}
