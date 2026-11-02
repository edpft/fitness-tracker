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
    planner::Arrangement as _,
    planner::{Discipline, TrainingWeek},
    schedule::{
        Alteration, DayPart, Diary, Illness, PartOfDay, Skip, SlotLength, TrainingPattern,
        TrainingSlot,
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

fn length_of(text: &str) -> Result<SlotLength, StoreError> {
    SlotLength::try_from(text).map_err(|error| corrupt(&error))
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
    let Some(pattern) = sqlx::query!(
        r"
        SELECT id, from_date, zone FROM training_pattern
        WHERE from_date <= ?
        ORDER BY from_date DESC
        LIMIT 1
        ",
        on
    )
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error(&error))?
    else {
        return Ok(None);
    };

    // **The store reads free time and the planner arranges it** (#673):
    // which discipline a slot is given is programming, worked out from the
    // whole week, so the pattern is read whole and handed over rather than
    // filtered here.
    let pattern = TrainingPattern::new(
        date_of(&pattern.from_date)?,
        zone_of(&pattern.zone)?,
        pattern_slots(pool, pattern.id).await?,
    );
    Ok(Diary::new(vec![pattern], Vec::new()).training_week(date, discipline))
}

/// One pattern's free slots and how long each is.
async fn pattern_slots(
    pool: &SqlitePool,
    pattern: i64,
) -> Result<BTreeMap<TrainingSlot, SlotLength>, StoreError> {
    let rows = sqlx::query!(
        r"
        SELECT weekday, part, length
        FROM training_slot
        WHERE pattern = ?
        ",
        pattern
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    rows.iter()
        .map(|row| Ok((slot_of(&row.weekday, &row.part)?, length_of(&row.length)?)))
        .collect()
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
            let slots = pattern_slots(&self.pool, week.id).await?;

            patterns.push(TrainingPattern::new(
                date_of(&week.from_date)?,
                zone_of(&week.zone)?,
                slots,
            ));
        }

        Ok(Diary::new(patterns, read_alterations(&self.pool).await?)
            .with_illnesses(read_illnesses(&self.pool).await?)
            .with_skips(read_skips(&self.pool).await?))
    }
}

/// Every illness, earliest first.
async fn read_illnesses(pool: &SqlitePool) -> Result<Vec<Illness>, StoreError> {
    let rows = sqlx::query!(
        r"
        SELECT from_date, ended_on
        FROM illness
        ORDER BY from_date
        "
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    rows.into_iter()
        .map(|row| {
            let illness = Illness::new(date_of(&row.from_date)?);
            match row.ended_on {
                None => Ok(illness),
                Some(ended) => illness
                    .ended(date_of(&ended)?)
                    .map_err(|error| corrupt(&format!("a stored illness: {error}"))),
            }
        })
        .collect()
}

/// Every skip, oldest first, so the latest of two for one window is last.
async fn read_skips(pool: &SqlitePool) -> Result<Vec<Skip>, StoreError> {
    let rows = sqlx::query!(
        r"
        SELECT from_date, from_part, until_date, until_part, reason
        FROM skip
        ORDER BY id
        "
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    rows.into_iter()
        .map(|row| {
            Skip::new(
                DayPart::new(date_of(&row.from_date)?, part_of(&row.from_part)?),
                DayPart::new(date_of(&row.until_date)?, part_of(&row.until_part)?),
                &row.reason,
            )
            .map_err(|error| StoreError::Corrupt {
                detail: format!("a stored skip: {error}"),
            })
        })
        .collect()
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
        SELECT id, start_date, days, zone, states_slots, reason
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

        // Stated slots, which may be none: zero rows either way, so
        // `states_slots` is what tells "none" from "the ordinary week".
        let slots = if alteration.states_slots == 0 {
            None
        } else {
            let rows = sqlx::query!(
                r"
                SELECT weekday, part, length
                FROM alteration_slot
                WHERE alteration = ?
                ",
                alteration.id
            )
            .fetch_all(pool)
            .await
            .map_err(|error| store_error(&error))?;

            Some(
                rows.iter()
                    .map(|row| Ok((slot_of(&row.weekday, &row.part)?, length_of(&row.length)?)))
                    .collect::<Result<BTreeMap<_, _>, StoreError>>()?,
            )
        };

        alterations.push(Alteration::new(
            date_of(&alteration.start_date)?,
            days,
            alteration.zone.as_deref().map(zone_of).transpose()?,
            slots,
            alteration.reason,
        ));
    }
    Ok(alterations)
}

impl DiaryAuthor for SqliteDiaryStore {
    async fn record_pattern(&self, pattern: &TrainingPattern) -> Result<(), StoreError> {
        let mut tx = super::writing(&self.pool)
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

        for (slot, length) in pattern.slots() {
            let weekday = weekday_key(slot.weekday);
            let part = slot.part.as_str();
            let length = length.as_str();
            sqlx::query!(
                r"
                INSERT INTO training_slot (pattern, weekday, part, length)
                VALUES (?, ?, ?, ?)
                ",
                id,
                weekday,
                part,
                length
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        tx.commit().await.map_err(|error| store_error(&error))
    }

    async fn record_alteration(&self, alteration: &Alteration) -> Result<(), StoreError> {
        let mut tx = super::writing(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;

        let authored_at = Timestamp::now().to_string();
        let start = alteration.start().to_string();
        let days = i64::from(alteration.days().get());
        let zone = alteration.zone().map(|zone| zone.id().to_owned());
        let states_slots = i64::from(alteration.slots().is_some());
        let reason = alteration.reason();

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

        for (slot, length) in alteration.slots().into_iter().flatten() {
            let weekday = weekday_key(slot.weekday);
            let part = slot.part.as_str();
            let length = length.as_str();
            sqlx::query!(
                r"
                INSERT INTO alteration_slot (alteration, weekday, part, length)
                VALUES (?, ?, ?, ?)
                ",
                id,
                weekday,
                part,
                length
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        tx.commit().await.map_err(|error| store_error(&error))
    }

    async fn record_illness(&self, illness: &Illness) -> Result<(), StoreError> {
        let authored_at = Timestamp::now().to_string();
        let from = illness.from().to_string();
        let ended = illness.ended_on().map(|ended| ended.to_string());
        sqlx::query!(
            r"
            INSERT INTO illness (authored_at, from_date, ended_on)
            VALUES (?, ?, ?)
            ON CONFLICT (from_date) DO UPDATE
                SET authored_at = excluded.authored_at,
                    ended_on    = excluded.ended_on
            ",
            authored_at,
            from,
            ended
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(())
    }

    async fn record_skip(&self, skip: &Skip) -> Result<(), StoreError> {
        let authored_at = Timestamp::now().to_string();
        let from_date = skip.from().date.to_string();
        let from_part = skip.from().part.as_str();
        let until_date = skip.until().date.to_string();
        let until_part = skip.until().part.as_str();
        let reason = skip.reason();
        sqlx::query!(
            r"
            INSERT INTO skip (authored_at, from_date, from_part, until_date, until_part, reason)
            VALUES (?, ?, ?, ?, ?, ?)
            ",
            authored_at,
            from_date,
            from_part,
            until_date,
            until_part,
            reason
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(())
    }
}
