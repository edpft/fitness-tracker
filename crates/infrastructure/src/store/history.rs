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

use application::{
    ExerciseHistory, LastPerformance, Performance, PerformedSetSummary, PerformedWorkoutReader,
    StoreError,
};
use domain::{
    gym::{
        AtLeastTwo, Distance, Duration, GymWorkout, Load, Metres, NonEmpty, OperatorZone,
        Performed, PerformedExercise, RepCount, Rir, Set, SetKind, SignedKg, WorkoutItem,
        WorkoutStart,
        exercise::{DistanceExercise, DurationExercise, RepsExercise},
    },
    landing::{Endpoint, EventKind, EventProvenance, EventTime, LandingRecordId, Provenance},
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

// --- The whole performance, for the round trip -------------------------------

/// The performed record, read whole rather than per exercise.
///
/// **What the round trip needs and history does not.** [`SqliteExerciseHistory`]
/// answers "the last time this exercise was performed" and can throw away
/// everything else; projecting a session into a prescription shape needs the
/// items, their groupings, the exercise order within each and every set — which
/// is the whole `GymWorkout` reassembled from the five tables the normalised
/// store writes.
///
/// Reading only, and § 10 applied exactly as above: a workout stands unless a
/// later-served landing record shares its source record id.
#[derive(Debug, Clone)]
pub struct SqlitePerformedWorkoutReader {
    pool: SqlitePool,
}

impl SqlitePerformedWorkoutReader {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// One set as the round-trip query hands it back.
struct WholeSetRow {
    item_position: i64,
    exercise_position: i64,
    is_superset: i64,
    exercise: String,
    measure: String,
    load_kind: String,
    load_grams: i64,
    outcome: String,
    reps: Option<i64>,
    duration_seconds: Option<i64>,
    distance_mm: Option<i64>,
    rir: Option<String>,
    set_kind: String,
    rest_after_seconds: Option<i64>,
}

/// A load from its two flat columns.
fn load_of(kind: &str, grams: i64) -> Result<Load, StoreError> {
    match kind {
        "absolute" => {
            let grams = u64::try_from(grams).map_err(|_| StoreError::Corrupt {
                detail: "an absolute load stored as a negative mass".to_owned(),
            })?;
            Ok(Load::Absolute(domain::gym::Kg::from_grams(grams)))
        }
        "relative" => Ok(Load::Relative(SignedKg::from_grams(grams))),
        other => Err(StoreError::Corrupt {
            detail: format!("{other:?} is not a load kind"),
        }),
    }
}

/// Everything about a set except its measure, which its exercise decides.
fn set_frame(
    row: &WholeSetRow,
) -> Result<(Load, Option<Rir>, SetKind, Option<Duration>), StoreError> {
    let load = load_of(&row.load_kind, row.load_grams)?;
    let intensity = match &row.rir {
        Some(text) => Some(
            Rir::try_from(text.clone()).map_err(|error| StoreError::Corrupt {
                detail: error.to_string(),
            })?,
        ),
        None => None,
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
    let rest_after = match row.rest_after_seconds {
        Some(seconds) => Some(Duration::from_seconds(u64::try_from(seconds).map_err(
            |_| StoreError::Corrupt {
                detail: "a negative rest".to_owned(),
            },
        )?)),
        None => None,
    };
    Ok((load, intensity, kind, rest_after))
}

/// A set's outcome in whichever measure its exercise is counted in.
///
/// The three measure columns are the sum type projected flat, and which one is
/// read follows from the exercise — never from "whichever column is filled",
/// which is the reading that would turn a missing value into a different
/// measure.
fn outcome_of<M>(
    row: &WholeSetRow,
    column: Option<i64>,
    build: impl FnOnce(i64) -> Result<M, StoreError>,
) -> Result<Performed<M>, StoreError> {
    match row.outcome.as_str() {
        "failed" => Ok(Performed::Failed),
        "completed" => {
            let value = column.ok_or_else(|| StoreError::Corrupt {
                detail: "a completed set with no measure".to_owned(),
            })?;
            Ok(Performed::Completed(build(value)?))
        }
        other => Err(StoreError::Corrupt {
            detail: format!("{other:?} is not a set outcome"),
        }),
    }
}

fn reps_of(value: i64) -> Result<RepCount, StoreError> {
    let count = u32::try_from(value).map_err(|_| StoreError::Corrupt {
        detail: "a repetition count the domain cannot hold".to_owned(),
    })?;
    RepCount::new(count).map_err(|error| StoreError::Corrupt {
        detail: error.to_string(),
    })
}

fn seconds_of(value: i64) -> Result<Duration, StoreError> {
    u64::try_from(value)
        .map(Duration::from_seconds)
        .map_err(|_| StoreError::Corrupt {
            detail: "a negative duration".to_owned(),
        })
}

fn distance_of(value: i64) -> Result<Distance, StoreError> {
    u64::try_from(value)
        .map(|mm| Distance {
            metres: Metres::from_millimetres(mm),
        })
        .map_err(|_| StoreError::Corrupt {
            detail: "a negative distance".to_owned(),
        })
}

/// One exercise part-way through assembly.
///
/// Named rather than a tuple because the nesting is three deep — item, exercise,
/// set — and a tuple at that depth is where a set ends up under the wrong
/// exercise.
struct PartialExercise {
    key: String,
    measure: String,
    sets: Vec<WholeSetRow>,
}

/// One item part-way through assembly, its exercises keyed by stored position.
struct PartialItem {
    is_superset: bool,
    exercises: BTreeMap<i64, PartialExercise>,
}

/// One exercise and its sets, in the measure its key resolves to.
fn exercise_of(
    key: &str,
    measure: &str,
    rows: &[WholeSetRow],
) -> Result<PerformedExercise, StoreError> {
    let unknown = || StoreError::Corrupt {
        detail: format!("{key:?} is not an exercise this build knows"),
    };
    match measure {
        "reps" => {
            let exercise = RepsExercise::try_from(key.to_owned()).map_err(|_| unknown())?;
            let sets = sets_of(rows, |row| outcome_of(row, row.reps, reps_of))?;
            Ok(PerformedExercise::ForReps { exercise, sets })
        }
        "duration" => {
            let exercise = DurationExercise::try_from(key.to_owned()).map_err(|_| unknown())?;
            let sets = sets_of(rows, |row| {
                outcome_of(row, row.duration_seconds, seconds_of)
            })?;
            Ok(PerformedExercise::ForDuration { exercise, sets })
        }
        "distance" => {
            let exercise = DistanceExercise::try_from(key.to_owned()).map_err(|_| unknown())?;
            let sets = sets_of(rows, |row| outcome_of(row, row.distance_mm, distance_of))?;
            Ok(PerformedExercise::ForDistance { exercise, sets })
        }
        other => Err(StoreError::Corrupt {
            detail: format!("{other:?} is not a measure"),
        }),
    }
}

/// The sets of one exercise, in stored order and never empty.
fn sets_of<M>(
    rows: &[WholeSetRow],
    outcome: impl Fn(&WholeSetRow) -> Result<Performed<M>, StoreError>,
) -> Result<NonEmpty<Set<M>>, StoreError> {
    let mut sets = Vec::with_capacity(rows.len());
    for row in rows {
        let (load, intensity, kind, rest_after) = set_frame(row)?;
        sets.push(Set {
            load,
            outcome: outcome(row)?,
            intensity,
            kind,
            rest_after,
        });
    }
    NonEmpty::new(sets).map_err(|_| StoreError::Corrupt {
        detail: "a performed exercise with no sets".to_owned(),
    })
}

impl PerformedWorkoutReader for SqlitePerformedWorkoutReader {
    async fn between(&self, from: Date, to: Date) -> Result<Vec<GymWorkout>, StoreError> {
        // **The window is widened in SQL and narrowed in Rust.** Which day a
        // workout was trained on depends on the zone *on its row*, so the exact
        // comparison cannot be a `WHERE` clause without assuming every session
        // shares one offset. A day either side covers every zone there is, and
        // the precise filter happens below through the same `day_of` the rest of
        // this file uses.
        let lower = from
            .checked_sub(jiff::Span::new().days(1))
            .map_err(|_| StoreError::Corrupt {
                detail: "a date before the calendar".to_owned(),
            })?
            .to_string();
        let upper = to
            .checked_add(jiff::Span::new().days(2))
            .map_err(|_| StoreError::Corrupt {
                detail: "a date beyond the calendar".to_owned(),
            })?
            .to_string();

        let workouts = sqlx::query!(
            r#"
            SELECT w.landing_record_id AS "landed_as!: i64",
                   w.source_record_id AS "source_record_id!: String",
                   w.started_at_utc AS "on_utc!: String", w.zone AS "zone!: String",
                   w.endpoint AS "endpoint!: String", w.event_kind AS "event_kind!: String",
                   w.event_time AS "event_time: String",
                   w.performed_against AS "performed_against: String"
            FROM gym_workout AS w
            WHERE w.started_at_utc >= ? AND w.started_at_utc < ?
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
            ORDER BY w.started_at_utc ASC, w.landing_record_id ASC
            "#,
            lower,
            upper
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut assembled = Vec::new();
        for row in workouts {
            let day = day_of(&row.on_utc, &row.zone)?;
            if day < from || day > to {
                continue;
            }
            let items = self.items_of(row.landed_as).await?;
            let started_at = start_of(&row.on_utc, &row.zone)?;
            let provenance = provenance_of(&row.endpoint, &row.event_kind, row.event_time)?;
            let source_record_id = domain::landing::SourceRecordId::try_from(
                row.source_record_id.as_str(),
            )
            .map_err(|error| StoreError::Corrupt {
                detail: error.to_string(),
            })?;
            let landed_as =
                LandingRecordId::try_from(row.landed_as).map_err(|error| StoreError::Corrupt {
                    detail: error.to_string(),
                })?;

            // A reference the store cannot read back is corrupt rather than
            // absent: it was written by something that had one.
            let performed_against = row
                .performed_against
                .map(domain::prescription::DeliveryReference::try_from)
                .transpose()
                .map_err(|error| StoreError::Corrupt {
                    detail: error.to_string(),
                })?;

            assembled.push(GymWorkout::new(
                items,
                started_at,
                provenance,
                source_record_id,
                landed_as,
                performed_against,
            ));
        }

        Ok(assembled)
    }
}

impl SqlitePerformedWorkoutReader {
    /// Every item of one workout, groupings and set order intact.
    ///
    /// One query per workout rather than one for the range: the assembly is
    /// nested three deep and a single flat result set would have to be
    /// re-partitioned by three keys at once, which is where an off-by-one puts a
    /// set under the wrong exercise. The inner joins are safe because the writer
    /// cannot produce an item with no exercises or an exercise with no sets —
    /// both are `NonEmpty` on the way in.
    async fn items_of(&self, workout: i64) -> Result<NonEmpty<WorkoutItem>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT i.position AS "item_position!: i64", i.is_superset AS "is_superset!: i64",
                   e.position AS "exercise_position!: i64",
                   e.exercise AS "exercise!: String", e.measure AS "measure!: String",
                   s.load_kind AS "load_kind!: String", s.load_grams AS "load_grams!: i64",
                   s.outcome AS "outcome!: String",
                   s.reps AS "reps: i64", s.duration_seconds AS "duration_seconds: i64",
                   s.distance_mm AS "distance_mm: i64",
                   s.rir AS "rir: String", s.set_kind AS "set_kind!: String",
                   s.rest_after_seconds AS "rest_after_seconds: i64"
            FROM workout_item AS i
            JOIN performed_exercise AS e
              ON e.workout = i.workout AND e.item_position = i.position
            JOIN performed_set AS s
              ON s.workout = e.workout AND s.item_position = e.item_position
             AND s.exercise_position = e.position
            WHERE i.workout = ?
            ORDER BY i.position ASC, e.position ASC, s.position ASC
            "#,
            workout
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        // Grouped by position rather than by "when the key changes", so the
        // ordering above is what decides order and nothing depends on rows for
        // one exercise being contiguous.
        let mut items: BTreeMap<i64, PartialItem> = BTreeMap::new();
        for row in rows {
            let row = WholeSetRow {
                item_position: row.item_position,
                exercise_position: row.exercise_position,
                is_superset: row.is_superset,
                exercise: row.exercise,
                measure: row.measure,
                load_kind: row.load_kind,
                load_grams: row.load_grams,
                outcome: row.outcome,
                reps: row.reps,
                duration_seconds: row.duration_seconds,
                distance_mm: row.distance_mm,
                rir: row.rir,
                set_kind: row.set_kind,
                rest_after_seconds: row.rest_after_seconds,
            };
            let item = items
                .entry(row.item_position)
                .or_insert_with(|| PartialItem {
                    is_superset: row.is_superset != 0,
                    exercises: BTreeMap::new(),
                });
            let exercise = item
                .exercises
                .entry(row.exercise_position)
                .or_insert_with(|| PartialExercise {
                    key: row.exercise.clone(),
                    measure: row.measure.clone(),
                    sets: Vec::new(),
                });
            exercise.sets.push(row);
        }

        let mut assembled = Vec::with_capacity(items.len());
        for item in items.into_values() {
            let mut performed = Vec::with_capacity(item.exercises.len());
            for exercise in item.exercises.into_values() {
                performed.push(exercise_of(
                    &exercise.key,
                    &exercise.measure,
                    &exercise.sets,
                )?);
            }
            assembled.push(if item.is_superset {
                WorkoutItem::Superset(domain::gym::Superset {
                    members: AtLeastTwo::new(performed).map_err(|error| StoreError::Corrupt {
                        detail: error.to_string(),
                    })?,
                })
            } else {
                let only = performed
                    .into_iter()
                    .next()
                    .ok_or_else(|| StoreError::Corrupt {
                        detail: "a workout item with no exercise".to_owned(),
                    })?;
                WorkoutItem::Exercise(only)
            });
        }

        NonEmpty::new(assembled).map_err(|_| StoreError::Corrupt {
            detail: "a stored workout with no items".to_owned(),
        })
    }
}

/// The stored instant and zone, as the domain's start.
fn start_of(started_at_utc: &str, zone: &str) -> Result<WorkoutStart, StoreError> {
    let instant: jiff::Timestamp = started_at_utc.parse().map_err(|_| StoreError::Corrupt {
        detail: format!("{started_at_utc:?} is not an instant"),
    })?;
    let zone = OperatorZone::try_from(zone).map_err(|error| StoreError::Corrupt {
        detail: error.to_string(),
    })?;
    Ok(WorkoutStart::new(instant, zone))
}

/// Provenance, mandatory and never inferred (§ II.3).
fn provenance_of(
    endpoint: &str,
    event_kind: &str,
    event_time: Option<String>,
) -> Result<Provenance, StoreError> {
    let endpoint =
        Endpoint::try_from(endpoint.to_owned()).map_err(|error| StoreError::Corrupt {
            detail: error.to_string(),
        })?;
    let kind = EventKind::try_from(event_kind.to_owned()).map_err(|error| StoreError::Corrupt {
        detail: error.to_string(),
    })?;
    let occurred_at = match event_time {
        Some(text) => {
            Some(
                EventTime::try_from(text.as_str()).map_err(|error| StoreError::Corrupt {
                    detail: error.to_string(),
                })?,
            )
        }
        None => None,
    };
    Ok(Provenance::Event(EventProvenance::new(
        endpoint,
        kind,
        occurred_at,
    )))
}
