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

use std::collections::{BTreeMap, BTreeSet};

use application::{CanonicalLayer, StoreError};
use domain::{
    gym::{
        Load, Rir, SignedKg,
        edit::{Edit, EditId, EditReason, Edited, ExerciseEdit, SetEdit},
        exercise::Exercise,
        identity::{CanonicalExerciseId, CanonicalSessionId, CanonicalSetId, Placed, SourceSet},
    },
    measure::{Kg, RepCount},
    sequence::NonEmpty,
};
use jiff::{Timestamp, civil::Date};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::canonical_gym::{SqliteCanonicalGymSessionStore, occurred};
use super::{corrupt, store_error};

/// One canonical exercise as an edit finds it: the day it was performed and
/// the sets it holds, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldExercise {
    pub exercise: CanonicalExerciseId,
    pub day: Date,
    pub sets: Vec<CanonicalSetId>,
}

/// Every edit, in the order asserted.
pub(super) async fn all(pool: &SqlitePool) -> Result<Vec<Edit>, StoreError> {
    let edits = sqlx::query!(
        r#"SELECT id AS "id!", asserted_at, reason FROM canonical_gym_edit ORDER BY asserted_at, id"#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| store_error(&e))?;
    let exercises = sqlx::query!(
        r#"
        SELECT edit AS "edit!", exercise AS "exercise!", becomes, read, sets
        FROM canonical_gym_exercise_edit
        ORDER BY edit, exercise
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| store_error(&e))?;
    let sets = sqlx::query!(
        r#"
        SELECT edit AS "edit!", canonical_set, reps, load_kind, load_grams, rir
        FROM canonical_gym_set_edit
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| store_error(&e))?;

    let mut by_edit: BTreeMap<i64, Vec<ExerciseEdit>> = BTreeMap::new();
    for row in exercises {
        by_edit.entry(row.edit).or_default().push(ExerciseEdit {
            exercise: CanonicalExerciseId::try_from(row.exercise).map_err(|e| corrupt(&e))?,
            becomes: exercise(&row.becomes)?,
            read: exercise(&row.read)?,
            sets: serde_json::from_str::<Vec<i64>>(&row.sets)
                .map_err(|e| corrupt(&e))?
                .into_iter()
                .map(|set| CanonicalSetId::try_from(set).map_err(|e| corrupt(&e)))
                .collect::<Result<_, _>>()?,
        });
    }
    let mut set_edits: BTreeMap<i64, Edited> = BTreeMap::new();
    for row in sets {
        let load = match (row.load_kind.as_deref(), row.load_grams) {
            (Some("absolute"), Some(grams)) => Some(Load::absolute(Kg::from_grams(
                u64::try_from(grams).map_err(|e| corrupt(&e))?,
            ))),
            (Some("relative"), Some(grams)) => Some(Load::relative(SignedKg::from_grams(grams))),
            (None, None) => None,
            _ => {
                return Err(StoreError::Corrupt {
                    detail: "a set edit's load is half written".to_owned(),
                });
            }
        };
        let reps = match row.reps {
            Some(reps) => Some(
                RepCount::new(u32::try_from(reps).map_err(|e| corrupt(&e))?)
                    .map_err(|e| corrupt(&e))?,
            ),
            None => None,
        };
        let effort = match row.rir {
            Some(rir) => Some(Rir::try_from(rir).map_err(|e| corrupt(&e))?),
            None => None,
        };
        let edit = SetEdit::new(reps, load, effort).ok_or_else(|| StoreError::Corrupt {
            detail: "a set edit that changes nothing".to_owned(),
        })?;
        set_edits.insert(
            row.edit,
            Edited::Set {
                set: CanonicalSetId::try_from(row.canonical_set).map_err(|e| corrupt(&e))?,
                edit,
            },
        );
    }

    let mut read = Vec::with_capacity(edits.len());
    for row in edits {
        let edited = match (by_edit.remove(&row.id), set_edits.remove(&row.id)) {
            (Some(exercises), None) => {
                Edited::Exercises(NonEmpty::new(exercises).map_err(|e| corrupt(&e))?)
            }
            (None, Some(set)) => set,
            // An assertion whose every exercise was dropped is removed with
            // the last of them, so this is one written half.
            _ => {
                return Err(StoreError::Corrupt {
                    detail: format!("edit {} edits nothing, or two kinds of thing", row.id),
                });
            }
        };
        read.push(Edit {
            id: EditId::from(row.id),
            asserted_at: row.asserted_at.parse().map_err(|e| corrupt(&e))?,
            reason: EditReason::try_from(row.reason).map_err(|e| corrupt(&e))?,
            edited,
        });
    }
    Ok(read)
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
            SELECT e.id AS "exercise!", s.started_at_utc, s.zone, s.on_day,
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
            SELECT e.id AS "exercise!", s.started_at_utc, s.zone, s.on_day,
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

    /// Assert what exercises were, in one assertion.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn assert_exercises(
        &self,
        at: Timestamp,
        reason: &EditReason,
        edits: &NonEmpty<ExerciseEdit>,
    ) -> Result<EditId, StoreError> {
        let mut tx = super::writing(self.pool())
            .await
            .map_err(|e| store_error(&e))?;
        let id = begin(&mut tx, at, reason).await?;
        for edit in edits.iter() {
            let exercise = edit.exercise.as_i64();
            let becomes = edit.becomes.as_str();
            let read = edit.read.as_str();
            let sets = sets_json(&edit.sets)?;
            sqlx::query!(
                r#"
                INSERT INTO canonical_gym_exercise_edit (edit, exercise, becomes, read, sets)
                VALUES (?, ?, ?, ?, ?)
                "#,
                id,
                exercise,
                becomes,
                read,
                sets
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| store_error(&e))?;
        }
        changed(&mut tx, id).await?;
        tx.commit().await.map_err(|e| store_error(&e))?;
        Ok(EditId::from(id))
    }

    /// Assert what one set was.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn assert_set(
        &self,
        at: Timestamp,
        reason: &EditReason,
        set: CanonicalSetId,
        edit: SetEdit,
    ) -> Result<EditId, StoreError> {
        let mut tx = super::writing(self.pool())
            .await
            .map_err(|e| store_error(&e))?;
        let id = begin(&mut tx, at, reason).await?;
        let canonical_set = set.as_i64();
        let reps = edit.reps().map(|reps| i64::from(reps.as_u32()));
        let (load_kind, load_grams) = match edit.load() {
            Some(Load::Absolute(mass)) => (
                Some("absolute"),
                Some(i64::try_from(mass.as_grams()).map_err(|e| corrupt(&e))?),
            ),
            Some(Load::Relative(delta)) => (Some("relative"), Some(delta.as_grams())),
            None => (None, None),
        };
        let rir = edit.effort().map(Rir::as_str);
        sqlx::query!(
            r#"
            INSERT INTO canonical_gym_set_edit (edit, canonical_set, reps, load_kind, load_grams, rir)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
            id,
            canonical_set,
            reps,
            load_kind,
            load_grams,
            rir
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| store_error(&e))?;
        changed(&mut tx, id).await?;
        tx.commit().await.map_err(|e| store_error(&e))?;
        Ok(EditId::from(id))
    }

    /// Retract one assertion whole. `false` if there is none.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn retract(&self, edit: EditId) -> Result<bool, StoreError> {
        let id = edit.as_i64();
        let mut tx = super::writing(self.pool())
            .await
            .map_err(|e| store_error(&e))?;
        changed(&mut tx, id).await?;
        let removed = sqlx::query!("DELETE FROM canonical_gym_edit WHERE id = ?", id)
            .execute(&mut *tx)
            .await
            .map_err(|e| store_error(&e))?
            .rows_affected();
        tx.commit().await.map_err(|e| store_error(&e))?;
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
        .execute(self.pool())
        .await
        .map_err(|e| store_error(&e))?;
        Ok(())
    }

    /// Remove every edit of one exercise, and any assertion left editing
    /// nothing.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn unedit_exercise(&self, exercise: CanonicalExerciseId) -> Result<(), StoreError> {
        let exercise = exercise.as_i64();
        let mut tx = super::writing(self.pool())
            .await
            .map_err(|e| store_error(&e))?;
        let sessions = sqlx::query_scalar!(
            r#"SELECT session AS "session!" FROM canonical_gym_exercise WHERE id = ?"#,
            exercise
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| store_error(&e))?;
        record_sessions(&mut tx, sessions).await?;
        sqlx::query!(
            "DELETE FROM canonical_gym_exercise_edit WHERE exercise = ?",
            exercise
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| store_error(&e))?;
        empty_assertions(&mut tx).await?;
        tx.commit().await.map_err(|e| store_error(&e))
    }

    /// Remove every edit of one set.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn unedit_set(&self, set: CanonicalSetId) -> Result<(), StoreError> {
        let set = set.as_i64();
        let mut tx = super::writing(self.pool())
            .await
            .map_err(|e| store_error(&e))?;
        let sessions = sqlx::query_scalar!(
            r#"SELECT session AS "session!" FROM canonical_gym_set WHERE id = ?"#,
            set
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| store_error(&e))?;
        record_sessions(&mut tx, sessions).await?;
        sqlx::query!(
            "DELETE FROM canonical_gym_set_edit WHERE canonical_set = ?",
            set
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| store_error(&e))?;
        empty_assertions(&mut tx).await?;
        tx.commit().await.map_err(|e| store_error(&e))
    }
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
        SELECT e.session AS "session!"
        FROM canonical_gym_exercise_edit AS x
        JOIN canonical_gym_exercise AS e ON e.id = x.exercise
        WHERE x.edit = ?
        UNION
        SELECT st.session
        FROM canonical_gym_set_edit AS x
        JOIN canonical_gym_set AS st ON st.id = x.canonical_set
        WHERE x.edit = ?
        "#,
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
