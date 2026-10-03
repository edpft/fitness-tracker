//! The performed record, as prescription reads it.
//!
//! **A projection of the canonical layer** (§ 5, #350). One visit is one
//! performance here however many sources recorded it, so the progression and the
//! ladder read the whole record rather than the stretch the source in use
//! covers — 389 sessions from 2016 rather than 143 from the end of 2024.
//!
//! **What a canonical set must state to be here.** A load, a count where it
//! completed, and whether it was a warm-up. Each is a field a canonical set may
//! simply not hold, and none of the three has a defensible default: a missing
//! load is not zero, and a missing kind read as working files a warm-up as
//! volume. So the three are a `WHERE` clause, and a performance left with no set
//! does not appear — which the caller reads as a performance with no working set
//! to progress from, because that is what it is.
//!
//! **And a guess never names the exercise.** A canonical exercise is
//! `recorded` where a source that records what was done named it and a guess
//! where only a watch's classifier placed it. The progression reads the first
//! only, and an exercise the record holds nothing but guesses of comes back as
//! [`LastPerformance::OnlyGuessed`] rather than as never performed: the operator
//! has 957 proposed exercises in the layer, and telling him he has never
//! performed one of them would be a report of a loss that did not happen (§ 37).
//!
//! **Supersession is the layer below's** (§ 10). This used to filter to the
//! latest-served landing record per source id itself, because it read one
//! source's rows directly; now it reads a derivation whose own inputs are the
//! normalised sessions, and where two of those are one source contradicting
//! itself the later supersedes before anything reaches here. No such pair exists
//! in the corpus.
//!
//! **Every set the record holds, warm-ups included.** The heaviest weight lifted
//! is the heaviest weight lifted, whatever the source tagged the set — so no
//! query here narrows by kind, and the kind travels with the set for the one
//! caller whose question is genuinely about working sets. The operator,
//! 2026-09-09: *"there's nothing inherent to warmup sets that means they
//! shouldn't be included in the search of the heaviest weight lifted."*
//!
//! Reading only. There is no write half: prescription may read the performed
//! layer and never the reverse (§ 11), and a type with no `write` is how that
//! stops being a promise about the code.

use std::collections::BTreeMap;

use application::{
    ExerciseHistory, FulfilledSession, LastPerformance, Performance, PerformedSetSummary,
    StoreError,
};
use domain::{
    gym::{Load, Performed, SetKind, SignedKg, exercise::RepsExercise},
    measure::RepCount,
    plan::PlanName,
    schedule::{Relative, SessionRole},
};
use jiff::civil::Date;
use sqlx::SqlitePool;

use super::{canonical_gym::occurred, store_error};

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
    set_kind: String,
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
            Load::Absolute(domain::measure::Kg::from_grams(grams))
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

    let kind = match row.set_kind.as_str() {
        "working" => SetKind::Working,
        "warmup" => SetKind::Warmup,
        other => {
            return Err(StoreError::Corrupt {
                detail: format!("{other:?} is not a set kind"),
            });
        }
    };

    Ok(PerformedSetSummary {
        load,
        outcome,
        kind,
    })
}

/// The prescribed session a performance names, rebuilt from its two columns.
///
/// Both come from the same left join, so they are present together or absent
/// together. A half-present pair is a store something else has written to, and
/// it is reported rather than papered over with a default role.
fn fulfilled_of(
    plan: Option<String>,
    intensity: Option<String>,
    volume: Option<String>,
) -> Result<Option<FulfilledSession>, StoreError> {
    let side = |text: String| {
        Relative::try_from(text).map_err(|error| StoreError::Corrupt {
            detail: error.to_string(),
        })
    };
    match (plan, intensity, volume) {
        (Some(plan), Some(intensity), Some(volume)) => Ok(Some(FulfilledSession {
            plan: PlanName::try_from(plan).map_err(|error| StoreError::Corrupt {
                detail: error.to_string(),
            })?,
            role: SessionRole::new(side(intensity)?, side(volume)?),
        })),
        (None, None, None) => Ok(None),
        _ => Err(StoreError::Corrupt {
            detail: "a prescribed session with only half an identity".to_owned(),
        }),
    }
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
            let answer = match performances.into_iter().next_back() {
                Some(last) => LastPerformance::Performed(last),
                // Nothing a record named, so the question is whether a watch
                // placed it. Asked only here: a series of performances has
                // nowhere to say "and a guess sat between these two".
                None if self.guessed(*exercise).await? => LastPerformance::OnlyGuessed,
                None => LastPerformance::NeverPerformed,
            };
            answers.insert(*exercise, answer);
        }
        Ok(answers)
    }

    async fn performances(&self, exercise: RepsExercise) -> Result<Vec<Performance>, StoreError> {
        grouped(self.sets_of(exercise).await?)
    }

    async fn newest_performance(&self) -> Result<Option<Date>, StoreError> {
        let row = sqlx::query!(
            r#"
            SELECT started_at_utc, zone, on_day
            FROM canonical_gym_session
            ORDER BY COALESCE(started_at_utc, on_day) DESC, id DESC
            LIMIT 1
            "#
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        match row {
            Some(row) => Ok(Some(
                occurred(
                    row.started_at_utc.as_deref(),
                    row.zone.as_deref(),
                    row.on_day.as_deref(),
                )?
                .day(),
            )),
            None => Ok(None),
        }
    }
}

impl SqliteExerciseHistory {
    /// Whether the layer holds a watch's guess at this exercise.
    ///
    /// Asked only where no recorded account exists, so that
    /// [`LastPerformance::OnlyGuessed`] can be told apart from never having
    /// performed it at all.
    async fn guessed(&self, exercise: RepsExercise) -> Result<bool, StoreError> {
        let key = exercise.as_str();
        let found = sqlx::query_scalar!(
            r#"
            SELECT 1 AS "found!: i64"
            FROM canonical_gym_exercise
            WHERE measure = 'reps' AND identified <> 'recorded' AND exercise = ?
            LIMIT 1
            "#,
            key
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(found.is_some())
    }
}

/// One set of one exercise, with the visit it belongs to and the prescription
/// that visit answered.
///
/// Named so the query and the grouping can be two functions: the SQL is most of
/// what this file is, and a hundred lines of it with a fold on the end is one
/// function doing two things.
struct PerformanceRow {
    session: i64,
    started_at_utc: Option<String>,
    zone: Option<String>,
    on_day: Option<String>,
    plan: Option<String>,
    session_intensity: Option<String>,
    session_volume: Option<String>,
    load_kind: String,
    load_grams: i64,
    outcome: String,
    reps: Option<i64>,
    set_kind: String,
}

impl SqliteExerciseHistory {
    /// Every readable set of one exercise, oldest visit first and in performed
    /// order within a visit.
    async fn sets_of(&self, exercise: RepsExercise) -> Result<Vec<PerformanceRow>, StoreError> {
        let key = exercise.as_str();
        sqlx::query_as!(
            PerformanceRow,
            r#"
            SELECT c.id AS "session!: i64",
                   c.started_at_utc, c.zone, c.on_day,
                   prescriber.name AS "plan: String",
                   issued.session_intensity AS "session_intensity: String",
                   issued.session_volume AS "session_volume: String",
                   st.load_kind AS "load_kind!: String",
                   st.load_grams AS "load_grams!: i64",
                   st.outcome AS "outcome!: String",
                   st.reps AS "reps: i64",
                   st.set_kind AS "set_kind!: String"
            FROM canonical_gym_session AS c
            JOIN canonical_gym_exercise AS e ON e.session = c.id
            JOIN canonical_gym_set AS st
              ON st.session = e.session
             AND st.item_position = e.item_position
             AND st.exercise_position = e.position
            -- Which prescription the visit answered, where any account of it
            -- names one.
            --
            -- **The link is one normalised session's and the answer is the
            -- visit's.** Only the source a prescription is delivered to records
            -- what a workout was performed against, so the reference is found on
            -- its rows and carried up through whichever field of the canonical
            -- session names that normalised session. The two attribution columns
            -- joined here are the layer's only `NOT NULL` ones, which is what
            -- makes them enough: an account that contributed anything to the
            -- visit named an exercise or a set's outcome.
            --
            -- `MIN(d.prescription)` keeps the pick deterministic where a
            -- reference somehow resolved twice; a join that fanned out would
            -- duplicate every set of the visit, which the gate would read as
            -- sets performed twice.
            LEFT JOIN (
                SELECT stands_on.canonical AS session, MIN(d.prescription) AS prescription
                FROM (
                    SELECT session AS canonical,
                           identified_normalised_session AS normalised
                    FROM canonical_gym_exercise
                    UNION
                    SELECT session AS canonical,
                           outcome_normalised_session AS normalised
                    FROM canonical_gym_set
                ) AS stands_on
                JOIN gym_workout AS w ON w.session = stands_on.normalised
                JOIN prescription_delivery AS d ON d.reference = w.performed_against
                GROUP BY stands_on.canonical
            ) AS link ON link.session = c.id
            LEFT JOIN prescribed_workout AS issued ON issued.id = link.prescription
            LEFT JOIN gym_mesocycle AS m ON m.id = issued.mesocycle
            LEFT JOIN plan AS prescriber ON prescriber.id = m.plan
            -- Counted in repetitions, because the port is (`ExerciseHistory`'s
            -- own doc): a key the duration vocabulary happened to share would
            -- otherwise hand back a hold's sets as a lift's.
            WHERE e.measure = 'reps'
              AND e.identified = 'recorded'
              AND e.exercise = ?
              AND st.load_kind IS NOT NULL
              AND st.set_kind IS NOT NULL
              AND (st.outcome = 'failed' OR st.reps IS NOT NULL)
            ORDER BY COALESCE(c.started_at_utc, c.on_day) ASC, c.id ASC,
                     e.item_position ASC, e.position ASC, st.position ASC
            "#,
            key
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))
    }
}

/// The rows folded into one performance per visit, set order kept.
///
/// The rows arrive in visit order and in performed order within a visit, so this
/// is a fold over a sequence the database already sorted.
fn grouped(rows: Vec<PerformanceRow>) -> Result<Vec<Performance>, StoreError> {
    let mut performances: Vec<Performance> = Vec::new();
    let mut current: Option<i64> = None;
    for row in rows {
        let day = occurred(
            row.started_at_utc.as_deref(),
            row.zone.as_deref(),
            row.on_day.as_deref(),
        )?
        .day();
        let summary = summary_of(&SetRow {
            load_kind: row.load_kind,
            load_grams: row.load_grams,
            outcome: row.outcome,
            reps: row.reps,
            set_kind: row.set_kind,
        })?;

        match performances.last_mut() {
            Some(last) if current == Some(row.session) => last.sets.push(summary),
            _ => {
                current = Some(row.session);
                performances.push(Performance {
                    on: day,
                    fulfilled: fulfilled_of(row.plan, row.session_intensity, row.session_volume)?,
                    sets: vec![summary],
                });
            }
        }
    }
    Ok(performances)
}
