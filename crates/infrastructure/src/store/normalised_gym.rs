//! Reading every normalised gym session, whatever source recorded it, in the
//! shape the canonical layer merges it in (#247).
//!
//! **One reader over four streams**, where a normalised *writer* is one per
//! stream. A writer is bound to a table because a table holds one shape;
//! matching reads across the sources by definition, and a reader per stream
//! would make the use case enumerate its sources.
//!
//! **The shape is read off the rows, not off the stream name.** A session with
//! a `measured_gym_session` row is a watch's; one dated by `on_day` is a log
//! the operator wrote by hand; anything else is a log that timestamps itself.
//! A fifth source therefore needs nothing here, which is what naming the four
//! streams would have cost.

use application::{NormalisedGymSessionReader, StoreError};
use domain::{
    canonical::{Attributed, NormalisedSessionId},
    gym::{
        CanonicalExercise, CanonicalItem, GuessedExercise, Identified, Load, MeasuredHeartRate,
        MeasuredSet, NormalisedGymSession,
    },
    measure::{Kg, PositiveDuration, RepCount},
    normalised::{OperatorZone, StartedAt},
    sequence::{AtLeastTwo, NonEmpty},
};
use jiff::Timestamp;
use sqlx::SqlitePool;

use super::{
    canonical_gym::{
        StoredSet, beats, distance_sets, duration_sets, identified_from_storage,
        normalised_session_from_storage, occurred, reps_sets,
    },
    corrupt, store_error,
};

/// Every normalised gym session the store holds, as the canonical layer reads
/// them.
pub struct SqliteNormalisedGymSessionReader {
    pool: SqlitePool,
}

impl SqliteNormalisedGymSessionReader {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl NormalisedGymSessionReader for SqliteNormalisedGymSessionReader {
    async fn all(&self) -> Result<Vec<NormalisedGymSession>, StoreError> {
        let spine = self.spine().await?;
        let mut sessions = Vec::with_capacity(spine.len());
        for row in spine {
            sessions.push(self.read_one(&row).await?);
        }
        Ok(sessions)
    }
}

/// One session's dating and the parts of it that do not need a second query.
struct SpineRow {
    id: i64,
    started_at_utc: Option<String>,
    zone: Option<String>,
    on_day: Option<String>,
    /// The session's length, where every part of it stated one. `NULL` where
    /// any part did not, which is the rule `PerformedGymSession::duration`
    /// states: a session one of whose parts says nothing has an unknown end,
    /// and answering with the parts that did state one would report a shorter
    /// session as a fact.
    duration_seconds: Option<i64>,
    /// The workout a watch's recording hangs off, where this is a watch's
    /// session.
    measured_workout: Option<i64>,
    average_bpm: Option<i64>,
    highest_bpm: Option<i64>,
}

impl SqliteNormalisedGymSessionReader {
    /// Every session, oldest first, ties broken by id.
    ///
    /// **A day sorts with the days and an instant with the instants**, which
    /// `COALESCE` gets right only because a `started_at_utc` begins with its
    /// date: `2019-03-14T07:43:22Z` and `2019-03-14` order against each other
    /// by their first ten characters, and the instant comes second. That is the
    /// order a reader wants and the determinism § 9 needs; it is not a claim
    /// that a day happened at midnight.
    async fn spine(&self) -> Result<Vec<SpineRow>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT s.id AS "id!: i64",
                   MIN(w.started_at_utc) AS "started_at_utc?: String",
                   MIN(w.zone) AS "zone?: String",
                   MIN(w.on_day) AS "on_day?: String",
                   CASE WHEN COUNT(*) = COUNT(w.duration_seconds)
                        THEN MAX(unixepoch(w.started_at_utc) + w.duration_seconds)
                             - MIN(unixepoch(w.started_at_utc))
                   END AS "duration_seconds?: i64",
                   MAX(m.workout) AS "measured_workout?: i64",
                   MAX(m.average_bpm) AS "average_bpm?: i64",
                   MAX(m.highest_bpm) AS "highest_bpm?: i64"
            FROM gym_session AS s
            JOIN gym_workout AS w ON w.session = s.id
            LEFT JOIN measured_gym_session AS m ON m.workout = w.id
            GROUP BY s.id
            ORDER BY COALESCE(MIN(w.started_at_utc), MIN(w.on_day)) ASC, s.id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        Ok(rows
            .into_iter()
            .map(|row| SpineRow {
                id: row.id,
                started_at_utc: row.started_at_utc,
                zone: row.zone,
                on_day: row.on_day,
                duration_seconds: row.duration_seconds,
                measured_workout: row.measured_workout,
                average_bpm: row.average_bpm,
                highest_bpm: row.highest_bpm,
            })
            .collect())
    }

    async fn read_one(&self, row: &SpineRow) -> Result<NormalisedGymSession, StoreError> {
        let id = normalised_session_from_storage(row.id)?;
        let when = occurred(
            row.started_at_utc.as_deref(),
            row.zone.as_deref(),
            row.on_day.as_deref(),
        )?;
        let duration = match row.duration_seconds {
            Some(seconds) => Some(Attributed::new(
                PositiveDuration::from_seconds(u64::try_from(seconds).map_err(|e| corrupt(&e))?)
                    .map_err(|e| corrupt(&e))?,
                id,
            )),
            None => None,
        };

        let Some(workout) = row.measured_workout else {
            let items = self.items(row.id, id).await?;
            return Ok(NormalisedGymSession::from_log(id, when, duration, items));
        };

        let heart_rate = match (row.average_bpm, row.highest_bpm) {
            (Some(average), Some(highest)) => {
                let summary =
                    domain::measure::HeartRateSummary::new(beats(average)?, beats(highest)?);
                let series = self.series(workout).await?;
                Some(Attributed::new(MeasuredHeartRate::new(summary, series), id))
            }
            _ => None,
        };
        let sets = self.measured_sets(workout).await?;
        Ok(NormalisedGymSession::from_watch(
            id, when, duration, heart_rate, &sets,
        ))
    }

    /// The watch's own recording, where one landed.
    async fn series(
        &self,
        workout: i64,
    ) -> Result<Option<domain::measure::HeartRateSeries>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT at_seconds AS "at_seconds!: i64", beats_per_minute AS "beats_per_minute!: i64"
            FROM measured_gym_session_heart_rate
            WHERE workout = ?
            ORDER BY at_seconds
            "#,
            workout
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut samples = Vec::with_capacity(rows.len());
        for row in rows {
            samples.push(domain::measure::HeartRateSample {
                at: domain::measure::Duration::from_seconds(
                    u64::try_from(row.at_seconds).map_err(|e| corrupt(&e))?,
                ),
                beats_per_minute: beats(row.beats_per_minute)?,
            });
        }
        Ok(NonEmpty::new(samples)
            .ok()
            .map(domain::measure::HeartRateSeries::new))
    }

    /// The sets a watch classified, in the order it recorded them.
    async fn measured_sets(&self, workout: i64) -> Result<Vec<MeasuredSet>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT started_at_utc AS "started_at_utc!: String", zone AS "zone!: String",
                   reps AS "reps!: i64",
                   load_kind AS "load_kind: String", load_grams AS "load_grams: i64",
                   guess_exercise AS "guess_exercise: String",
                   guess_movement AS "guess_movement: String",
                   guess_implement AS "guess_implement: String",
                   guess_from AS "guess_from: String"
            FROM measured_set
            WHERE workout = ?
            ORDER BY position
            "#,
            workout
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut sets = Vec::with_capacity(rows.len());
        for row in rows {
            let instant: Timestamp = row.started_at_utc.parse().map_err(|e| corrupt(&e))?;
            let zone = OperatorZone::try_from(row.zone).map_err(|e| corrupt(&e))?;
            sets.push(MeasuredSet {
                at: StartedAt::new(instant, zone),
                reps: RepCount::new(u32::try_from(row.reps).map_err(|e| corrupt(&e))?)
                    .map_err(|e| corrupt(&e))?,
                load: load_of(row.load_kind.as_deref(), row.load_grams)?,
                guess: guess_of(
                    row.guess_from.as_deref(),
                    row.guess_exercise.as_deref(),
                    row.guess_movement.as_deref(),
                    row.guess_implement.as_deref(),
                )?,
            });
        }
        Ok(sets)
    }

    /// A log's items, in the order performed, every field named for `id`.
    ///
    /// **Across the session's workouts, in their order.** A Hevy session split
    /// across four routines was one sequence of items as it was performed, and
    /// which routine each came from is the normalised layer's provenance
    /// rather than this layer's shape.
    async fn items(
        &self,
        session: i64,
        id: NormalisedSessionId,
    ) -> Result<Vec<CanonicalItem>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT w.id AS "workout!: i64", i.position AS "item_position!: i64",
                   i.is_superset AS "is_superset!: i64",
                   e.position AS "exercise_position!: i64",
                   e.exercise AS "exercise!: String", e.measure AS "measure!: String"
            FROM gym_workout AS w
            JOIN workout_item AS i ON i.workout = w.id
            JOIN performed_exercise AS e
              ON e.workout = i.workout AND e.item_position = i.position
            WHERE w.session = ?
            ORDER BY w.started_at_utc ASC, w.on_day ASC, w.id ASC,
                     i.position ASC, e.position ASC
            "#,
            session
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut items: Vec<CanonicalItem> = Vec::new();
        let mut pending: Vec<CanonicalExercise> = Vec::new();
        let mut at: Option<(i64, i64, bool)> = None;

        for row in rows {
            let here = (row.workout, row.item_position, row.is_superset == 1);
            if at != Some(here) {
                if let Some((_, _, superset)) = at {
                    push_item(&mut items, std::mem::take(&mut pending), superset)?;
                }
                at = Some(here);
            }
            let sets = self
                .sets(row.workout, row.item_position, row.exercise_position, id)
                .await?;
            pending.push(exercise_of(&row.exercise, &row.measure, sets, id)?);
        }
        if let Some((_, _, superset)) = at {
            push_item(&mut items, pending, superset)?;
        }
        Ok(items)
    }

    /// One exercise's sets, every field of them named for `id`.
    async fn sets(
        &self,
        workout: i64,
        item_position: i64,
        exercise_position: i64,
        id: NormalisedSessionId,
    ) -> Result<Vec<StoredSet>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT outcome AS "outcome!: String", reps, duration_seconds, distance_mm,
                   load_kind, load_grams, rir, set_kind AS "set_kind!: String",
                   rest_after_seconds
            FROM performed_set
            WHERE workout = ? AND item_position = ? AND exercise_position = ?
            ORDER BY position
            "#,
            workout,
            item_position,
            exercise_position
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let named = id.as_i64();
        Ok(rows
            .into_iter()
            .map(|row| StoredSet {
                outcome: row.outcome,
                reps: row.reps,
                duration_seconds: row.duration_seconds,
                distance_mm: row.distance_mm,
                outcome_normalised_session: named,
                load_normalised_session: row.load_kind.as_ref().map(|_| named),
                load_kind: row.load_kind,
                load_grams: row.load_grams,
                // A log states no clock for a set. Only a watch does, and a
                // watch's sets do not come through here.
                began_at_utc: None,
                began_zone: None,
                began_normalised_session: None,
                rir_normalised_session: row.rir.as_ref().map(|_| named),
                rir: row.rir,
                set_kind_normalised_session: Some(named),
                set_kind: Some(row.set_kind),
                rest_after_normalised_session: row.rest_after_seconds.map(|_| named),
                rest_after_seconds: row.rest_after_seconds,
            })
            .collect())
    }
}

/// One item, from the exercises read for it.
fn push_item(
    items: &mut Vec<CanonicalItem>,
    exercises: Vec<CanonicalExercise>,
    superset: bool,
) -> Result<(), StoreError> {
    if superset {
        let members = AtLeastTwo::new(exercises).map_err(|e| corrupt(&e))?;
        items.push(CanonicalItem::Superset(Box::new(members)));
    } else {
        for exercise in exercises {
            items.push(CanonicalItem::Exercise(exercise));
        }
    }
    Ok(())
}

/// One exercise a log named, in the arm its measure fixes.
fn exercise_of(
    exercise: &str,
    measure: &str,
    sets: Vec<StoredSet>,
    id: NormalisedSessionId,
) -> Result<CanonicalExercise, StoreError> {
    let unreadable = |detail: String| StoreError::Corrupt { detail };
    match measure {
        "reps" => Ok(CanonicalExercise::ForReps {
            identified: Attributed::new(
                domain::gym::Identified::Recorded(
                    domain::gym::exercise::RepsExercise::try_from(exercise.to_owned())
                        .map_err(|e| corrupt(&e))?,
                ),
                id,
            ),
            sets: NonEmpty::new(reps_sets(sets)?).map_err(|e| corrupt(&e))?,
        }),
        "duration" => Ok(CanonicalExercise::ForDuration {
            exercise: Attributed::new(
                domain::gym::DurationExercise::try_from(exercise.to_owned())
                    .map_err(|e| corrupt(&e))?,
                id,
            ),
            sets: NonEmpty::new(duration_sets(sets)?).map_err(|e| corrupt(&e))?,
        }),
        "distance" => Ok(CanonicalExercise::ForDistance {
            exercise: Attributed::new(
                domain::gym::DistanceExercise::try_from(exercise.to_owned())
                    .map_err(|e| corrupt(&e))?,
                id,
            ),
            sets: NonEmpty::new(distance_sets(sets)?).map_err(|e| corrupt(&e))?,
        }),
        other => Err(unreadable(format!("{other} is not a measure"))),
    }
}

/// A load, where the row holds one.
fn load_of(kind: Option<&str>, grams: Option<i64>) -> Result<Option<Load>, StoreError> {
    match (kind, grams) {
        (Some("absolute"), Some(grams)) => Ok(Some(Load::absolute(Kg::from_grams(
            u64::try_from(grams).map_err(|e| corrupt(&e))?,
        )))),
        (Some("relative"), Some(grams)) => Ok(Some(Load::relative(
            domain::gym::SignedKg::from_grams(grams),
        ))),
        (None, None) => Ok(None),
        _ => Err(StoreError::Corrupt {
            detail: "a measured set's load is half written".to_owned(),
        }),
    }
}

/// What the watch made of a set's movement.
///
/// **Read by the canonical layer's own reader**, with `guess_from` of `NULL`
/// spelled as the `undetermined` that column means: the normalised table says
/// nothing where the canonical one says so in a word, and the two have to
/// arrive at the same [`GuessedExercise`] or a session would change shape on
/// its way up a layer.
fn guess_of(
    how: Option<&str>,
    exercise: Option<&str>,
    movement: Option<&str>,
    implement: Option<&str>,
) -> Result<GuessedExercise, StoreError> {
    match identified_from_storage(how.unwrap_or("undetermined"), exercise, movement, implement)? {
        Identified::Guessed(guess) => Ok(guess),
        Identified::Recorded(_) => Err(StoreError::Corrupt {
            detail: "a watch's set records an exercise rather than guessing one".to_owned(),
        }),
    }
}
