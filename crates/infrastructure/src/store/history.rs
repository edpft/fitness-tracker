//! The performed record, as prescription reads it.
//!
//! **This is where § 10 is applied.** The spec defers the canonical layer on the
//! grounds that one source needs no reconciliation, and that is sound about
//! *matching* and incomplete about *supersession*: two landing records sharing a
//! source record id are one source contradicting itself, and the later
//! supersedes. A projection reading both would prescribe from a performance the
//! source has withdrawn — silently wrong rather than visibly broken.
//!
//! So every query here filters to the latest-served record per source id. One
//! `WHERE` clause, using the `serve_ordinal` raw already carries for exactly this
//! purpose. No such pair exists in the corpus today, which is why it is cheap now
//! and expensive to find later.
//!
//! **What this deliberately does not do is resolve fragmentation.** One training
//! session spread across four landing records stays four workouts. That is
//! harmless for "the most recent performance of this exercise" and would not be
//! harmless for a session count, a frequency or a streak — § 10's counting rule.
//! The first figure that needs it right is the trigger to build the canonical
//! layer properly.
//!
//! Reading only, and only working sets. There is no write half: prescription may
//! read the performed layer and never the reverse (§ 11), and a type with no
//! `write` is how that stops being a promise about the code.

use std::collections::BTreeMap;

use application::{ExerciseHistory, LastPerformance, Performance, PerformedSetSummary, StoreError};
use domain::{
    gym::{Load, Performed, RepCount, SignedKg, exercise::RepsExercise},
    landing::LandingRecordId,
};
use jiff::civil::Date;
use sqlx::SqlitePool;

use super::store_error;

// § 10's currency clause appears in full in both queries below rather than in a
// shared constant. That is forced rather than chosen: `sqlx::query!` verifies SQL
// against the schema at compile time and will not accept an interpolated string,
// and offline verification is worth more here than one fewer copy. The two copies
// must stay identical — a workout is current when no later-served landing record
// shares its source record id.

/// The performed record, read for prescription.
#[derive(Debug, Clone)]
pub struct SqliteExerciseHistory {
    pool: SqlitePool,
}

impl SqliteExerciseHistory {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// One row of a set, as SQLite hands it back.
struct SetRow {
    load_kind: String,
    load_grams: i64,
    outcome: String,
    reps: Option<i64>,
}

/// Rebuild a set summary from its flat row.
///
/// The measure columns are the sum type projected; which one matters follows
/// from the exercise, and a failed attempt populates none of them.
fn summary_of(row: &SetRow) -> Result<PerformedSetSummary, StoreError> {
    let load = match row.load_kind.as_str() {
        "absolute" => {
            let grams = u64::try_from(row.load_grams).map_err(|_| StoreError::Corrupt {
                detail: "an absolute load stored as a negative mass".to_owned(),
            })?;
            Load::Absolute(domain::gym::Kg::from_grams(grams))
        }
        "relative" => Load::Relative(SignedKg::from_grams(row.load_grams)),
        other => {
            return Err(StoreError::Corrupt {
                detail: format!("{other:?} is not a load kind"),
            });
        }
    };

    let outcome = match row.outcome.as_str() {
        "failed" => Performed::Failed,
        "completed" => {
            let reps = row.reps.ok_or_else(|| StoreError::Corrupt {
                detail: "a completed set of repetitions with no count".to_owned(),
            })?;
            let count = u32::try_from(reps).map_err(|_| StoreError::Corrupt {
                detail: "a repetition count the domain cannot hold".to_owned(),
            })?;
            Performed::Completed(RepCount::new(count).map_err(|error| StoreError::Corrupt {
                detail: error.to_string(),
            })?)
        }
        other => {
            return Err(StoreError::Corrupt {
                detail: format!("{other:?} is not a set outcome"),
            });
        }
    };

    Ok(PerformedSetSummary { load, outcome })
}

/// The date part of a stored UTC instant, in the zone it was recorded against.
///
/// The zone is on the row, so this resolves through it rather than assuming the
/// stored instant's UTC date is the day trained (§ II.3). An evening session in
/// British Summer Time is the case that breaks the naive reading.
fn day_of(started_at_utc: &str, zone: &str) -> Result<Date, StoreError> {
    let instant: jiff::Timestamp = started_at_utc.parse().map_err(|_| StoreError::Corrupt {
        detail: format!("{started_at_utc:?} is not an instant"),
    })?;
    let tz = jiff::tz::TimeZone::get(zone).map_err(|_| StoreError::Corrupt {
        detail: format!("{zone:?} is not a zone this build knows"),
    })?;
    Ok(instant.to_zoned(tz).date())
}

impl ExerciseHistory for SqliteExerciseHistory {
    async fn last_performances(
        &self,
        exercises: &[RepsExercise],
    ) -> Result<BTreeMap<RepsExercise, LastPerformance>, StoreError> {
        let mut answers = BTreeMap::new();
        for exercise in exercises {
            // Every exercise asked about gets an answer, and a named one. An
            // absent key would make the caller's `get` return `None` for both
            // "never performed" and "never asked", which is the conflation
            // `LastPerformance` exists to prevent.
            let performances = self.performances(*exercise).await?;
            let answer = performances
                .into_iter()
                .next_back()
                .map_or(LastPerformance::NeverPerformed, LastPerformance::Performed);
            answers.insert(*exercise, answer);
        }
        Ok(answers)
    }

    async fn performances(&self, exercise: RepsExercise) -> Result<Vec<Performance>, StoreError> {
        let key = exercise.as_str();
        let rows = sqlx::query!(
            r#"
            SELECT w.started_at_utc AS "on_utc!: String", w.zone AS "zone!: String",
                   w.landing_record_id AS "landed_as!: i64",
                   s.load_kind AS "load_kind!: String", s.load_grams AS "load_grams!: i64",
                   s.outcome AS "outcome!: String", s.reps AS "reps: i64"
            FROM gym_workout AS w
            JOIN performed_exercise AS e ON e.workout = w.landing_record_id
            JOIN performed_set AS s
              ON s.workout = w.landing_record_id
             AND s.item_position = e.item_position
             AND s.exercise_position = e.position
            WHERE e.exercise = ?
              AND s.set_kind = 'working'
              AND NOT EXISTS (
                    SELECT 1
                    FROM gym_workout AS superseding
                    JOIN hevy_workout_landing AS later
                        ON later.id = superseding.landing_record_id
                    JOIN hevy_workout_landing AS this
                        ON this.id = w.landing_record_id
                    WHERE superseding.source_record_id = w.source_record_id
                      AND later.serve_ordinal > this.serve_ordinal
              )
            ORDER BY w.started_at_utc ASC, e.item_position ASC, s.position ASC
            "#,
            key
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        // Group into one `Performance` per workout, preserving set order.
        let mut performances: Vec<Performance> = Vec::new();
        for row in rows {
            let day = day_of(&row.on_utc, &row.zone)?;
            let summary = summary_of(&SetRow {
                load_kind: row.load_kind,
                load_grams: row.load_grams,
                outcome: row.outcome,
                reps: row.reps,
            })?;

            let id =
                LandingRecordId::try_from(row.landed_as).map_err(|error| StoreError::Corrupt {
                    detail: error.to_string(),
                })?;
            match performances.last_mut() {
                Some(last) if last.landed_as == id => last.sets.push(summary),
                _ => performances.push(Performance {
                    on: day,
                    landed_as: id,
                    sets: vec![summary],
                }),
            }
        }
        Ok(performances)
    }

    async fn newest_performance(&self) -> Result<Option<Date>, StoreError> {
        let row = sqlx::query!(
            r#"
            SELECT w.started_at_utc AS "on_utc!: String", w.zone AS "zone!: String"
            FROM gym_workout AS w
            WHERE NOT EXISTS (
                    SELECT 1
                    FROM gym_workout AS superseding
                    JOIN hevy_workout_landing AS later
                        ON later.id = superseding.landing_record_id
                    JOIN hevy_workout_landing AS this
                        ON this.id = w.landing_record_id
                    WHERE superseding.source_record_id = w.source_record_id
                      AND later.serve_ordinal > this.serve_ordinal
            )
            ORDER BY w.started_at_utc DESC
            LIMIT 1
            "#
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        match row {
            Some(row) => Ok(Some(day_of(&row.on_utc, &row.zone)?)),
            None => Ok(None),
        }
    }
}

// `PerformedWorkoutReader` is deliberately not here yet.
//
// Projecting a performance into a prescription shape needs the whole
// `GymWorkout` rebuilt from its five tables, and the store has only ever written
// them. That assembly belongs with the round-trip work (SC-010) rather than with
// the MVP, which needs per-exercise history and nothing more. Adding a
// half-built reader now would be a second, weaker way to read the same tables.
