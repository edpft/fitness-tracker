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
    normalised::OperatorZone,
    schedule::{
        Absence, Allocation, Alteration, Diary, Discipline, GymClosure, PartOfDay, Relative,
        SessionRole, TrainingPattern, TrainingSlot, TrainingWeek,
    },
};
use jiff::{Timestamp, civil::Date};
use sqlx::SqlitePool;

use super::{
    corrupt,
    gym_mesocycle::{weekday_key, weekday_of},
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

fn relative_of(text: &str) -> Result<Relative, StoreError> {
    Relative::try_from(text.to_owned()).map_err(|error| corrupt(&error))
}

fn role_of(intensity: &str, volume: &str) -> Result<SessionRole, StoreError> {
    Ok(SessionRole::new(
        relative_of(intensity)?,
        relative_of(volume)?,
    ))
}

fn allocation_of(
    discipline: &str,
    intensity: &str,
    volume: &str,
) -> Result<Allocation, StoreError> {
    Ok(Allocation::new(
        discipline_of(discipline)?,
        role_of(intensity, volume)?,
    ))
}

/// One discipline's ordinary week as of a date, for a calendar to be built
/// against.
///
/// **The pattern in force, with no alteration applied**, which is the rule
/// [`Diary::training_week`] states: a holiday covering a block's start date
/// would otherwise decide the shape of every week after it.
///
/// A query of its own rather than a whole [`Diary`], because a mesocycle is
/// read back one row at a time and assembling every pattern and every
/// alteration to answer about one date is work nobody asked for.
///
/// # Errors
///
/// [`StoreError`] if the store is unavailable or holds a week the domain
/// refuses.
pub(super) async fn training_week(
    pool: &SqlitePool,
    date: Date,
    discipline: Discipline,
) -> Result<Option<TrainingWeek>, StoreError> {
    let on = date.to_string();
    let key = discipline.as_str();
    let rows = sqlx::query!(
        r"
        SELECT weekday, part, intensity, volume
        FROM training_slot
        WHERE discipline = ?
          AND pattern = (
              SELECT id FROM training_pattern
              WHERE from_date <= ?
              ORDER BY from_date DESC
              LIMIT 1
          )
        ORDER BY part
        ",
        key,
        on
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut days = Vec::with_capacity(rows.len());
    for row in rows {
        // The part of day is read so the ordering matches `TrainingSlot`'s:
        // where a discipline holds two slots on one weekday, the earlier one's
        // role is the one the week keeps.
        let _ = part_of(&row.part)?;
        days.push((
            weekday_of(&row.weekday)?,
            role_of(&row.intensity, &row.volume)?,
        ));
    }
    Ok(TrainingWeek::new(days).ok())
}

/// A holiday's reason, which the schema requires it to have and forbids an
/// illness. Only a row written round the check could be missing one.
fn reason_of(text: Option<String>) -> Result<String, StoreError> {
    text.ok_or_else(|| corrupt(&"a holiday with no reason"))
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
                SELECT weekday, part, discipline, intensity, volume
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
                        allocation_of(&row.discipline, &row.intensity, &row.volume)?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>, StoreError>>()?;

            patterns.push(TrainingPattern::new(
                date_of(&week.from_date)?,
                zone_of(&week.zone)?,
                slots,
            ));
        }

        Ok(Diary::new(patterns, read_alterations(&self.pool).await?)
            .with_closures(read_closures(&self.pool).await?))
    }
}

/// Every alteration, with the slots each one states.
///
/// **Split out of `diary` so that function stays inside the line budget**, and
/// they are two questions anyway: the ordinary weeks and what departs from
/// them are stored apart because a departure is a fact about dates rather than
/// about which week was in force when it was recorded.
async fn read_alterations(pool: &SqlitePool) -> Result<Vec<Alteration>, StoreError> {
    let booked = sqlx::query!(
        r"
        SELECT id, start_date, days, absence, zone, states_slots, reason
        FROM alteration
        ORDER BY start_date
        "
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut alterations = Vec::with_capacity(booked.len());
    for alteration in booked {
        let days = u8::try_from(alteration.days)
            .ok()
            .and_then(std::num::NonZeroU8::new)
            .ok_or_else(|| corrupt(&"an alteration covering no days"))?;

        let absence = match alteration.absence.as_str() {
            "illness" => Absence::Illness,
            "holiday" if alteration.states_slots == 0 => Absence::Holiday {
                zone: alteration.zone.as_deref().map(zone_of).transpose()?,
                slots: None,
                reason: reason_of(alteration.reason)?,
            },
            // Stated slots, which may be none: zero rows either way, so
            // `states_slots` is what tells "none" from "the ordinary week".
            "holiday" => {
                let rows = sqlx::query!(
                    r"
                    SELECT weekday, part, discipline, intensity, volume
                    FROM alteration_slot
                    WHERE alteration = ?
                    ",
                    alteration.id
                )
                .fetch_all(pool)
                .await
                .map_err(|error| store_error(&error))?;

                Absence::Holiday {
                    zone: alteration.zone.as_deref().map(zone_of).transpose()?,
                    reason: reason_of(alteration.reason)?,
                    slots: Some(
                        rows.iter()
                            .map(|row| {
                                Ok((
                                    slot_of(&row.weekday, &row.part)?,
                                    allocation_of(&row.discipline, &row.intensity, &row.volume)?,
                                ))
                            })
                            .collect::<Result<BTreeMap<_, _>, StoreError>>()?,
                    ),
                }
            }
            other => return Err(corrupt(&format!("an absence of kind {other:?}"))),
        };

        alterations.push(Alteration::new(
            date_of(&alteration.start_date)?,
            days,
            absence,
        ));
    }
    Ok(alterations)
}

/// Every run of days the gym is shut.
async fn read_closures(pool: &SqlitePool) -> Result<Vec<GymClosure>, StoreError> {
    let rows = sqlx::query!(
        r"
        SELECT start_date, days, reason
        FROM gym_closure
        ORDER BY start_date
        "
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    rows.into_iter()
        .map(|row| {
            let days = u8::try_from(row.days)
                .ok()
                .and_then(std::num::NonZeroU8::new)
                .ok_or_else(|| corrupt(&"a gym closure covering no days"))?;
            Ok(GymClosure::new(date_of(&row.start_date)?, days, row.reason))
        })
        .collect()
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

        for (slot, allocation) in pattern.slots() {
            let weekday = weekday_key(slot.weekday);
            let part = slot.part.as_str();
            let discipline = allocation.discipline.as_str();
            let intensity = allocation.role.intensity().as_str();
            let volume = allocation.role.volume().as_str();
            sqlx::query!(
                r"
                INSERT INTO training_slot (pattern, weekday, part, discipline, intensity, volume)
                VALUES (?, ?, ?, ?, ?, ?)
                ",
                id,
                weekday,
                part,
                discipline,
                intensity,
                volume
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
        let absence = alteration.absence().as_str();
        let zone = alteration.zone().map(|zone| zone.id().to_owned());
        let states_slots = i64::from(alteration.slots().is_some());
        let reason = alteration.reason().map(str::to_owned);

        let id = sqlx::query!(
            r"
            INSERT INTO alteration (
                authored_at, start_date, days, absence, zone, states_slots, reason
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT (start_date) DO UPDATE
                SET authored_at = excluded.authored_at,
                    days        = excluded.days,
                    absence     = excluded.absence,
                    zone         = excluded.zone,
                    states_slots = excluded.states_slots,
                    reason       = excluded.reason
            RETURNING id
            ",
            authored_at,
            start,
            days,
            absence,
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

        for (slot, allocation) in alteration.slots().into_iter().flatten() {
            let weekday = weekday_key(slot.weekday);
            let part = slot.part.as_str();
            let discipline = allocation.discipline.as_str();
            let intensity = allocation.role.intensity().as_str();
            let volume = allocation.role.volume().as_str();
            sqlx::query!(
                r"
                INSERT INTO alteration_slot (alteration, weekday, part, discipline, intensity, volume)
                VALUES (?, ?, ?, ?, ?, ?)
                ",
                id,
                weekday,
                part,
                discipline,
                intensity,
                volume
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        tx.commit().await.map_err(|error| store_error(&error))
    }

    async fn record_closure(&self, closure: &GymClosure) -> Result<(), StoreError> {
        let authored_at = Timestamp::now().to_string();
        let start = closure.start().to_string();
        let days = i64::from(closure.days().get());
        let reason = closure.reason();

        sqlx::query!(
            r"
            INSERT INTO gym_closure (authored_at, start_date, days, reason)
            VALUES (?, ?, ?, ?)
            ON CONFLICT (start_date) DO UPDATE
                SET authored_at = excluded.authored_at,
                    days        = excluded.days,
                    reason      = excluded.reason
            ",
            authored_at,
            start,
            days,
            reason
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(())
    }
}
