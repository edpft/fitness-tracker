//! The operator's edits to the canonical gym layer, and what finds what they
//! reach (§ II.2, #893).
//!
//! **Stored beside the layer, never in it.** An edit is applied as the layer
//! is rebuilt, so removing one and rebuilding restores what the sources said.
//! Making, retracting or narrowing one records the days of the sessions it
//! reaches as changed, which is what has the next run rebuild them.
//!
//! **A field an edit states names no normalised session.** Its
//! `*_normalised_session` column in the layer is empty while the value beside
//! it is not, which is how an edited field reads back as the edit's
//! ([`domain::canonical::TakenFrom::Edit`]).
//!
//! **Every write moves the overlay's revision** (#905), so a page reading the
//! edits themselves, as History's list of edits to settle does, is drawn again
//! the moment one is kept or removed rather than after the next run.

use std::collections::{BTreeMap, BTreeSet};

use application::{Authors, CanonicalLayer, Node, StoreError};
use domain::{
    canonical::Occurred,
    gym::{
        Load, Rir, SignedKg,
        edit::{Edit, EditId, EditReason, Found, Measure, SparseGymSession, SparseSet, Taken},
        exercise::Exercise,
        identity::{CanonicalExerciseId, CanonicalSessionId, CanonicalSetId, Placed, SourceSet},
    },
    landing::LandingStream,
    measure::{Distance, Duration, Kg, Metres, RepCount},
};
use jiff::{Timestamp, civil::Date};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::authoring::{Authoring, Writing};
use super::canonical_gym::{SqliteCanonicalGymSessionStore, occurred};
use super::{corrupt, store_error};

/// What a write of the edits authors: the overlay, and nothing else. The
/// layer it is applied to moves when a run rebuilds the days it reaches.
pub struct Overlay;

impl Authors for Overlay {
    fn authors() -> BTreeSet<Node> {
        [Node::Edits(CanonicalLayer::Gym)].into()
    }
}

/// A write of the edits, holding the store's write lock from its first
/// statement.
async fn writing(pool: &SqlitePool) -> Result<Writing<Overlay>, StoreError> {
    Authoring::new(pool.clone()).writing::<Overlay>().await
}

/// One canonical exercise as an edit finds it: the day it was performed and
/// the sets it holds, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldExercise {
    pub session: CanonicalSessionId,
    pub exercise: CanonicalExerciseId,
    pub day: Date,
    pub sets: Vec<CanonicalSetId>,
}

/// Every edit, in the order asserted, each as the sparse sessions it holds.
pub(super) async fn all(pool: &SqlitePool) -> Result<Vec<Edit>, StoreError> {
    let edits = sqlx::query!(
        r#"SELECT id AS "id!", asserted_at, reason FROM canonical_gym_edit ORDER BY asserted_at, id"#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| store_error(&e))?;
    let exercises = sqlx::query!(
        r#"
        SELECT edit AS "edit!", session AS "session!", exercise AS "exercise!", becomes, read, sets
        FROM canonical_gym_exercise_edit
        ORDER BY edit, exercise
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| store_error(&e))?;
    let sets = sqlx::query!(
        r#"
        SELECT edit AS "edit!", session AS "session!", exercise AS "exercise!",
               canonical_set AS "canonical_set!", reps, duration_seconds, distance_mm,
               load_kind, load_grams, rir
        FROM canonical_gym_set_edit
        ORDER BY edit, canonical_set
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| store_error(&e))?;

    let mut read: BTreeMap<i64, Edit> = BTreeMap::new();
    for row in edits {
        read.insert(
            row.id,
            Edit {
                id: EditId::from(row.id),
                asserted_at: row.asserted_at.parse().map_err(|e| corrupt(&e))?,
                reason: EditReason::try_from(row.reason).map_err(|e| corrupt(&e))?,
                sessions: BTreeMap::new(),
                found: BTreeMap::new(),
            },
        );
    }
    for row in exercises {
        let exercise = CanonicalExerciseId::try_from(row.exercise).map_err(|e| corrupt(&e))?;
        let assertion = assertion(&mut read, row.edit)?;
        assertion
            .sessions
            .entry(CanonicalSessionId::try_from(row.session).map_err(|e| corrupt(&e))?)
            .or_default()
            .exercises
            .entry(exercise)
            .or_default()
            .exercise = Some(self::exercise(&row.becomes)?);
        assertion.found.insert(
            exercise,
            Found {
                read: self::exercise(&row.read)?,
                sets: set_ids(&row.sets)?.into_iter().collect(),
            },
        );
    }
    for row in sets {
        let fields = SparseSet::new(
            measured(row.reps, row.duration_seconds, row.distance_mm)?,
            loaded(row.load_kind.as_deref(), row.load_grams)?,
            row.rir
                .map(|rir| Rir::try_from(rir).map_err(|e| corrupt(&e)))
                .transpose()?,
        )
        .ok_or_else(|| StoreError::Corrupt {
            detail: "a set edit that changes nothing".to_owned(),
        })?;
        assertion(&mut read, row.edit)?
            .sessions
            .entry(CanonicalSessionId::try_from(row.session).map_err(|e| corrupt(&e))?)
            .or_default()
            .exercises
            .entry(CanonicalExerciseId::try_from(row.exercise).map_err(|e| corrupt(&e))?)
            .or_default()
            .sets
            .insert(
                CanonicalSetId::try_from(row.canonical_set).map_err(|e| corrupt(&e))?,
                fields,
            );
    }
    read_bounds(pool, &mut read).await?;
    let mut ordered: Vec<Edit> = read.into_values().collect();
    if let Some(empty) = ordered.iter().find(|edit| edit.sessions.is_empty()) {
        // An assertion whose every field was dropped is removed with the
        // last of them, so this is one written half.
        return Err(StoreError::Corrupt {
            detail: format!("edit {} edits nothing", empty.id),
        });
    }
    ordered.sort_by_key(|edit| (edit.asserted_at, edit.id));
    Ok(ordered)
}

/// Each assertion's starts and ends, into the sessions it holds.
async fn read_bounds(pool: &SqlitePool, read: &mut BTreeMap<i64, Edit>) -> Result<(), StoreError> {
    let bounds = sqlx::query!(
        r#"
        SELECT edit AS "edit!", session AS "session!", started_from, started_at_utc,
               started_zone, started_on_day, ended_from, ended_at_utc
        FROM canonical_gym_session_edit
        ORDER BY edit, session
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| store_error(&e))?;

    for row in bounds {
        let started = started_of(
            row.started_from,
            row.started_at_utc.as_deref(),
            row.started_zone.as_deref(),
            row.started_on_day.as_deref(),
        )?;
        let ended = ended_of(row.ended_from, row.ended_at_utc)?;
        let session = assertion(read, row.edit)?
            .sessions
            .entry(CanonicalSessionId::try_from(row.session).map_err(|e| corrupt(&e))?)
            .or_default();
        session.started = started;
        session.ended = ended;
    }
    Ok(())
}

/// A session edit's start: a stream's, or one stated as an instant or a day.
fn started_of(
    from: Option<String>,
    at: Option<&str>,
    zone: Option<&str>,
    day: Option<&str>,
) -> Result<Option<Taken<Occurred>>, StoreError> {
    match (from, at, day) {
        (Some(stream), None, None) => {
            Ok(Some(Taken::From(stream.parse().map_err(|e| corrupt(&e))?)))
        }
        (None, None, None) => Ok(None),
        (None, at, day) => Ok(Some(Taken::Stated(occurred(at, zone, day)?))),
        _ => Err(StoreError::Corrupt {
            detail: "a start both taken and stated".to_owned(),
        }),
    }
}

/// A session edit's end: a stream's, or one stated.
fn ended_of(
    from: Option<String>,
    at: Option<String>,
) -> Result<Option<Taken<Timestamp>>, StoreError> {
    match (from, at) {
        (Some(stream), None) => Ok(Some(Taken::From(stream.parse().map_err(|e| corrupt(&e))?))),
        (None, Some(at)) => Ok(Some(Taken::Stated(
            at.parse::<Timestamp>().map_err(|e| corrupt(&e))?,
        ))),
        (None, None) => Ok(None),
        _ => Err(StoreError::Corrupt {
            detail: "an end both taken and stated".to_owned(),
        }),
    }
}

/// The assertion a field row belongs to.
fn assertion(read: &mut BTreeMap<i64, Edit>, edit: i64) -> Result<&mut Edit, StoreError> {
    read.get_mut(&edit).ok_or_else(|| StoreError::Corrupt {
        detail: format!("edit {edit} has a field but no assertion"),
    })
}

/// A set edit's load, from its kind and its grams.
fn loaded(kind: Option<&str>, grams: Option<i64>) -> Result<Option<Load>, StoreError> {
    match (kind, grams) {
        (Some("absolute"), Some(grams)) => Ok(Some(Load::absolute(Kg::from_grams(
            u64::try_from(grams).map_err(|e| corrupt(&e))?,
        )))),
        (Some("relative"), Some(grams)) => Ok(Some(Load::relative(SignedKg::from_grams(grams)))),
        (None, None) => Ok(None),
        _ => Err(StoreError::Corrupt {
            detail: "a set edit's load is half written".to_owned(),
        }),
    }
}

/// A set edit's measure, from whichever of its three columns holds one.
fn measured(
    reps: Option<i64>,
    seconds: Option<i64>,
    millimetres: Option<i64>,
) -> Result<Option<Measure>, StoreError> {
    match (reps, seconds, millimetres) {
        (Some(reps), None, None) => Ok(Some(Measure::Reps(
            RepCount::new(u32::try_from(reps).map_err(|e| corrupt(&e))?)
                .map_err(|e| corrupt(&e))?,
        ))),
        (None, Some(seconds), None) => Ok(Some(Measure::Duration(Duration::from_seconds(
            u64::try_from(seconds).map_err(|e| corrupt(&e))?,
        )))),
        (None, None, Some(millimetres)) => Ok(Some(Measure::Distance(Distance {
            metres: Metres::from_millimetres(u64::try_from(millimetres).map_err(|e| corrupt(&e))?),
        }))),
        (None, None, None) => Ok(None),
        _ => Err(StoreError::Corrupt {
            detail: "a set edit states two measures".to_owned(),
        }),
    }
}

fn exercise(key: &str) -> Result<Exercise, StoreError> {
    Exercise::named(key).ok_or_else(|| StoreError::Corrupt {
        detail: format!("{key} is not an exercise this build knows"),
    })
}

/// The column an exercise's measure is stored under.
const fn measure(exercise: Exercise) -> &'static str {
    match exercise {
        Exercise::Reps(_) => "reps",
        Exercise::Duration(_) => "duration",
        Exercise::Distance(_) => "distance",
    }
}

impl SqliteCanonicalGymSessionStore {
    /// Where a source set was last placed: its session, exercise and set.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn placed(&self, source: &SourceSet) -> Result<Option<Placed>, StoreError> {
        let stream = source.stream.to_string();
        let record = source.record.as_str();
        let term = source.term.as_str();
        let row = sqlx::query!(
            r#"
            SELECT session AS "session!", exercise AS "exercise!", canonical_set AS "set!"
            FROM canonical_gym_source_set
            WHERE stream = ? AND record = ? AND term = ?
            "#,
            stream,
            record,
            term
        )
        .fetch_optional(self.pool())
        .await
        .map_err(|e| store_error(&e))?;
        row.map(|row| {
            Ok(Placed {
                session: CanonicalSessionId::try_from(row.session).map_err(|e| corrupt(&e))?,
                exercise: CanonicalExerciseId::try_from(row.exercise).map_err(|e| corrupt(&e))?,
                set: CanonicalSetId::try_from(row.set).map_err(|e| corrupt(&e))?,
            })
        })
        .transpose()
    }

    /// Every canonical exercise that reads as `exercise` now, on the day it
    /// was performed, oldest first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn reading_as(&self, exercise: Exercise) -> Result<Vec<HeldExercise>, StoreError> {
        let key = exercise.as_str();
        let measure = measure(exercise);
        let rows = sqlx::query!(
            r#"
            SELECT e.id AS "exercise!", e.session AS "session!", s.started_at_utc, s.zone, s.on_day,
                   (SELECT json_group_array(st.id) FROM (
                        SELECT id FROM canonical_gym_set
                         WHERE session = e.session AND item_position = e.item_position
                           AND exercise_position = e.position
                         ORDER BY position) AS st) AS "sets!: String"
            FROM canonical_gym_exercise AS e
            JOIN canonical_gym_session AS s ON s.id = e.session
            WHERE e.exercise = ? AND e.measure = ?
            "#,
            key,
            measure
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| store_error(&e))?;
        let mut held = Vec::with_capacity(rows.len());
        for row in rows {
            held.push(HeldExercise {
                session: CanonicalSessionId::try_from(row.session).map_err(|e| corrupt(&e))?,
                exercise: CanonicalExerciseId::try_from(row.exercise).map_err(|e| corrupt(&e))?,
                day: occurred(
                    row.started_at_utc.as_deref(),
                    row.zone.as_deref(),
                    row.on_day.as_deref(),
                )?
                .day(),
                sets: set_ids(&row.sets)?,
            });
        }
        held.sort_by_key(|exercise| (exercise.day, exercise.exercise));
        Ok(held)
    }

    /// Each of these exercises the layer holds, on the day it was performed.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn exercises_held(
        &self,
        exercises: &BTreeSet<CanonicalExerciseId>,
    ) -> Result<Vec<HeldExercise>, StoreError> {
        let ids = super::ids_json(exercises.iter().map(|id| id.as_i64()));
        let rows = sqlx::query!(
            r#"
            SELECT e.id AS "exercise!", e.session AS "session!", s.started_at_utc, s.zone, s.on_day,
                   (SELECT json_group_array(st.id) FROM (
                        SELECT id FROM canonical_gym_set
                         WHERE session = e.session AND item_position = e.item_position
                           AND exercise_position = e.position
                         ORDER BY position) AS st) AS "sets!: String"
            FROM canonical_gym_exercise AS e
            JOIN canonical_gym_session AS s ON s.id = e.session
            WHERE e.id IN (SELECT value FROM json_each(?))
            "#,
            ids
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| store_error(&e))?;
        let mut held = Vec::with_capacity(rows.len());
        for row in rows {
            held.push(HeldExercise {
                session: CanonicalSessionId::try_from(row.session).map_err(|e| corrupt(&e))?,
                exercise: CanonicalExerciseId::try_from(row.exercise).map_err(|e| corrupt(&e))?,
                day: occurred(
                    row.started_at_utc.as_deref(),
                    row.zone.as_deref(),
                    row.on_day.as_deref(),
                )?
                .day(),
                sets: set_ids(&row.sets)?,
            });
        }
        held.sort_by_key(|exercise| (exercise.day, exercise.exercise));
        Ok(held)
    }

    /// Every exercise the layer holds and its sets, for telling what a
    /// rebuild left an edit standing on.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn held(
        &self,
    ) -> Result<BTreeMap<CanonicalExerciseId, BTreeSet<CanonicalSetId>>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT e.id AS "exercise!", st.id AS "set!"
            FROM canonical_gym_exercise AS e
            JOIN canonical_gym_set AS st
              ON st.session = e.session AND st.item_position = e.item_position
             AND st.exercise_position = e.position
            "#
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| store_error(&e))?;
        let mut held: BTreeMap<CanonicalExerciseId, BTreeSet<CanonicalSetId>> = BTreeMap::new();
        for row in rows {
            held.entry(CanonicalExerciseId::try_from(row.exercise).map_err(|e| corrupt(&e))?)
                .or_default()
                .insert(CanonicalSetId::try_from(row.set).map_err(|e| corrupt(&e))?);
        }
        Ok(held)
    }

    /// Every session the layer holds, and the streams accounting for it,
    /// for telling what a rebuild left an edit of its start or end standing
    /// on.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn sessions_held(
        &self,
    ) -> Result<BTreeMap<CanonicalSessionId, BTreeSet<LandingStream>>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT s.id AS "session!", a.stream AS "stream?"
            FROM canonical_gym_session AS s
            LEFT JOIN canonical_gym_session_account AS a ON a.session = s.id
            "#
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| store_error(&e))?;
        let mut held: BTreeMap<CanonicalSessionId, BTreeSet<LandingStream>> = BTreeMap::new();
        for row in rows {
            let streams = held
                .entry(CanonicalSessionId::try_from(row.session).map_err(|e| corrupt(&e))?)
                .or_default();
            if let Some(stream) = row.stream {
                streams.insert(stream.parse().map_err(|e| corrupt(&e))?);
            }
        }
        Ok(held)
    }

    /// Remove every edit of when one session started and ended.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn unedit_bounds(&self, session: CanonicalSessionId) -> Result<(), StoreError> {
        let session = session.as_i64();
        let mut tx = writing(self.pool()).await?;
        record_sessions(&mut tx, vec![session]).await?;
        sqlx::query!(
            "DELETE FROM canonical_gym_session_edit WHERE session = ?",
            session
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| store_error(&e))?;
        empty_assertions(&mut tx).await?;
        tx.commit().await
    }

    /// Assert what sessions were, in one assertion: each field the sparse
    /// sessions hold, and how each exercise whose name they state was found.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written, or an exercise whose
    /// name is stated has no account of how it was found.
    pub async fn assert(
        &self,
        at: Timestamp,
        reason: &EditReason,
        sessions: &BTreeMap<CanonicalSessionId, SparseGymSession>,
        found: &BTreeMap<CanonicalExerciseId, Found>,
    ) -> Result<EditId, StoreError> {
        let mut tx = writing(self.pool()).await?;
        let id = begin(&mut tx, at, reason).await?;
        for (session, sparse) in sessions {
            if sparse.started.is_some() || sparse.ended.is_some() {
                write_bounds(&mut tx, id, *session, sparse).await?;
            }
            for (exercise, fields) in &sparse.exercises {
                if let Some(becomes) = fields.exercise {
                    let as_found = found.get(exercise).ok_or_else(|| StoreError::Corrupt {
                        detail: format!("exercise {exercise} is renamed with nothing found"),
                    })?;
                    write_exercise(&mut tx, id, *session, *exercise, becomes, as_found).await?;
                }
                for (set, set_fields) in &fields.sets {
                    write_set(&mut tx, id, *session, *exercise, *set, *set_fields).await?;
                }
            }
        }
        changed(&mut tx, id).await?;
        tx.commit().await?;
        Ok(EditId::from(id))
    }

    /// Retract one assertion whole. `false` if there is none.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn retract(&self, edit: EditId) -> Result<bool, StoreError> {
        let id = edit.as_i64();
        let mut tx = writing(self.pool()).await?;
        changed(&mut tx, id).await?;
        let removed = sqlx::query!("DELETE FROM canonical_gym_edit WHERE id = ?", id)
            .execute(&mut **tx)
            .await
            .map_err(|e| store_error(&e))?
            .rows_affected();
        tx.commit().await?;
        Ok(removed > 0)
    }

    /// Take a set a source added into every edit of its exercise: the
    /// operator says it reads as the edit says.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn keep(
        &self,
        exercise: CanonicalExerciseId,
        set: CanonicalSetId,
    ) -> Result<(), StoreError> {
        let exercise = exercise.as_i64();
        let set = set.as_i64();
        let mut tx = writing(self.pool()).await?;
        sqlx::query!(
            r#"
            UPDATE canonical_gym_exercise_edit
               SET sets = (SELECT json_group_array(value) FROM (
                             SELECT value FROM json_each(sets)
                             UNION SELECT ?
                             ORDER BY value))
             WHERE exercise = ?
            "#,
            set,
            exercise
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| store_error(&e))?;
        tx.commit().await
    }

    /// Remove every edit of one exercise, and any assertion left editing
    /// nothing.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn unedit_exercise(&self, exercise: CanonicalExerciseId) -> Result<(), StoreError> {
        let exercise = exercise.as_i64();
        let mut tx = writing(self.pool()).await?;
        let sessions = sqlx::query_scalar!(
            r#"SELECT session AS "session!" FROM canonical_gym_exercise WHERE id = ?"#,
            exercise
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| store_error(&e))?;
        record_sessions(&mut tx, sessions).await?;
        sqlx::query!(
            "DELETE FROM canonical_gym_exercise_edit WHERE exercise = ?",
            exercise
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| store_error(&e))?;
        empty_assertions(&mut tx).await?;
        tx.commit().await
    }

    /// Remove every edit of one set.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn unedit_set(&self, set: CanonicalSetId) -> Result<(), StoreError> {
        let set = set.as_i64();
        let mut tx = writing(self.pool()).await?;
        let sessions = sqlx::query_scalar!(
            r#"SELECT session AS "session!" FROM canonical_gym_set WHERE id = ?"#,
            set
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| store_error(&e))?;
        record_sessions(&mut tx, sessions).await?;
        sqlx::query!(
            "DELETE FROM canonical_gym_set_edit WHERE canonical_set = ?",
            set
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| store_error(&e))?;
        empty_assertions(&mut tx).await?;
        tx.commit().await
    }
}

/// When a session started and ended, as an assertion states either.
async fn write_bounds(
    tx: &mut Transaction<'_, Sqlite>,
    edit: i64,
    session: CanonicalSessionId,
    sparse: &SparseGymSession,
) -> Result<(), StoreError> {
    let session = session.as_i64();
    let (started_from, started_at_utc, started_zone, started_on_day) = match &sparse.started {
        Some(Taken::From(stream)) => (Some(stream.to_string()), None, None, None),
        Some(Taken::Stated(Occurred::At(at))) => (
            None,
            Some(at.instant().to_string()),
            Some(at.zone().id().to_owned()),
            None,
        ),
        Some(Taken::Stated(Occurred::On(day))) => (None, None, None, Some(day.to_string())),
        None => (None, None, None, None),
    };
    let (ended_from, ended_at_utc) = match &sparse.ended {
        Some(Taken::From(stream)) => (Some(stream.to_string()), None),
        Some(Taken::Stated(at)) => (None, Some(at.to_string())),
        None => (None, None),
    };
    sqlx::query!(
        r#"
        INSERT INTO canonical_gym_session_edit
            (edit, session, started_from, started_at_utc, started_zone, started_on_day,
             ended_from, ended_at_utc)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        edit,
        session,
        started_from,
        started_at_utc,
        started_zone,
        started_on_day,
        ended_from,
        ended_at_utc
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;
    Ok(())
}

/// One exercise's name, as an assertion states it.
async fn write_exercise(
    tx: &mut Transaction<'_, Sqlite>,
    edit: i64,
    session: CanonicalSessionId,
    exercise: CanonicalExerciseId,
    becomes: Exercise,
    found: &Found,
) -> Result<(), StoreError> {
    let session = session.as_i64();
    let exercise = exercise.as_i64();
    let becomes = becomes.as_str();
    let read = found.read.as_str();
    let sets = sets_json(&found.sets)?;
    sqlx::query!(
        r#"
        INSERT INTO canonical_gym_exercise_edit (edit, session, exercise, becomes, read, sets)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
        edit,
        session,
        exercise,
        becomes,
        read,
        sets
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;
    Ok(())
}

/// One set's fields, as an assertion states them.
async fn write_set(
    tx: &mut Transaction<'_, Sqlite>,
    edit: i64,
    session: CanonicalSessionId,
    exercise: CanonicalExerciseId,
    set: CanonicalSetId,
    fields: SparseSet,
) -> Result<(), StoreError> {
    let session = session.as_i64();
    let exercise = exercise.as_i64();
    let canonical_set = set.as_i64();
    let (reps, duration_seconds, distance_mm) = match fields.measure() {
        Some(Measure::Reps(reps)) => (Some(i64::from(reps.as_u32())), None, None),
        Some(Measure::Duration(duration)) => (
            None,
            Some(i64::try_from(duration.as_seconds()).map_err(|e| corrupt(&e))?),
            None,
        ),
        Some(Measure::Distance(distance)) => (
            None,
            None,
            Some(i64::try_from(distance.metres.as_millimetres()).map_err(|e| corrupt(&e))?),
        ),
        None => (None, None, None),
    };
    let (load_kind, load_grams) = match fields.load() {
        Some(Load::Absolute(mass)) => (
            Some("absolute"),
            Some(i64::try_from(mass.as_grams()).map_err(|e| corrupt(&e))?),
        ),
        Some(Load::Relative(delta)) => (Some("relative"), Some(delta.as_grams())),
        None => (None, None),
    };
    let rir = fields.intensity().map(Rir::as_str);
    sqlx::query!(
        r#"
        INSERT INTO canonical_gym_set_edit
            (edit, session, exercise, canonical_set, reps, duration_seconds, distance_mm,
             load_kind, load_grams, rir)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        edit,
        session,
        exercise,
        canonical_set,
        reps,
        duration_seconds,
        distance_mm,
        load_kind,
        load_grams,
        rir
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;
    Ok(())
}

/// The assertion's own row.
async fn begin(
    tx: &mut Transaction<'_, Sqlite>,
    at: Timestamp,
    reason: &EditReason,
) -> Result<i64, StoreError> {
    let at = at.to_string();
    let reason = reason.as_str();
    sqlx::query_scalar!(
        r#"INSERT INTO canonical_gym_edit (asserted_at, reason) VALUES (?, ?) RETURNING id AS "id!""#,
        at,
        reason
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| store_error(&e))
}

/// Record the days of every session an assertion reaches as changed, so the
/// next run rebuilds them under it, or without it.
async fn changed(tx: &mut Transaction<'_, Sqlite>, edit: i64) -> Result<(), StoreError> {
    let sessions = sqlx::query_scalar!(
        r#"
        SELECT session AS "session!" FROM canonical_gym_exercise_edit WHERE edit = ?
        UNION
        SELECT session FROM canonical_gym_set_edit WHERE edit = ?
        UNION
        SELECT session FROM canonical_gym_session_edit WHERE edit = ?
        "#,
        edit,
        edit,
        edit
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;
    record_sessions(tx, sessions).await
}

/// Record the days these sessions were performed on as changed.
async fn record_sessions(
    tx: &mut Transaction<'_, Sqlite>,
    sessions: Vec<i64>,
) -> Result<(), StoreError> {
    if sessions.is_empty() {
        return Ok(());
    }
    let ids = super::ids_json(sessions);
    let rows = sqlx::query!(
        r#"
        SELECT started_at_utc, zone, on_day
        FROM canonical_gym_session
        WHERE id IN (SELECT value FROM json_each(?))
        "#,
        ids
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;
    let mut days = BTreeSet::new();
    for row in rows {
        days.insert(
            occurred(
                row.started_at_utc.as_deref(),
                row.zone.as_deref(),
                row.on_day.as_deref(),
            )?
            .day(),
        );
    }
    super::canonical_changes::record(tx, CanonicalLayer::Gym, &days).await
}

/// Remove every assertion that edits nothing any more.
async fn empty_assertions(tx: &mut Transaction<'_, Sqlite>) -> Result<(), StoreError> {
    sqlx::query!(
        r#"
        DELETE FROM canonical_gym_edit
        WHERE id NOT IN (SELECT edit FROM canonical_gym_exercise_edit)
          AND id NOT IN (SELECT edit FROM canonical_gym_set_edit)
          AND id NOT IN (SELECT edit FROM canonical_gym_session_edit)
        "#
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;
    Ok(())
}

fn sets_json(sets: &BTreeSet<CanonicalSetId>) -> Result<String, StoreError> {
    serde_json::to_string(&sets.iter().map(|set| set.as_i64()).collect::<Vec<_>>())
        .map_err(|e| corrupt(&e))
}

fn set_ids(stored: &str) -> Result<Vec<CanonicalSetId>, StoreError> {
    serde_json::from_str::<Vec<i64>>(stored)
        .map_err(|e| corrupt(&e))?
        .into_iter()
        .map(|set| CanonicalSetId::try_from(set).map_err(|e| corrupt(&e)))
        .collect()
}
