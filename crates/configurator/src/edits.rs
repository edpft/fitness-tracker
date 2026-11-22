//! The operator's edits to the canonical gym layer, and what finds what they
//! reach (§ II.2, #893).
//!
//! **An edit is anchored to a canonical id**, so whichever source a session
//! was drawn from — Hevy, a watch, a spreadsheet, Beyond the Whiteboard — its
//! exercises and sets are edited the same way. A page finds those ids from
//! the session it shows: each set names the source sets it was built from,
//! and the layer records where each was last placed.

use std::collections::BTreeSet;

use application::{CanonicalGymSessionStore as _, StoreError};
use domain::{
    gym::{
        CanonicalExercise,
        edit::{Conflict, Edit, EditId, EditReason, Edits, ExerciseEdit, SetEdit},
        exercise::Exercise,
        identity::{CanonicalExerciseId, CanonicalSetId, ExerciseIds},
    },
    sequence::NonEmpty,
};
use infrastructure::SqliteCanonicalGymSessionStore;
use jiff::Timestamp;

use crate::store::Store;

/// One canonical exercise as an edit finds it: its day and its sets.
pub use infrastructure::HeldExercise;

/// The ids an exercise read from the layer holds, found by its sets' sources:
/// [`None`] where one of its sets was placed nowhere, which is a layer being
/// rebuilt under the page.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn ids_of(
    store: &Store,
    exercise: &CanonicalExercise,
) -> Result<Option<ExerciseIds>, StoreError> {
    let adapter = store.adapter(SqliteCanonicalGymSessionStore::new);
    let mut held: Option<CanonicalExerciseId> = None;
    let mut sets = Vec::new();
    for sources in exercise.set_sources() {
        let Some(source) = sources.first() else {
            return Ok(None);
        };
        let Some(placed) = adapter
            .call("SqliteCanonicalGymSessionStore::placed", |adapter| {
                adapter.placed(source)
            })
            .await?
        else {
            return Ok(None);
        };
        if *held.get_or_insert(placed.exercise) != placed.exercise {
            return Ok(None);
        }
        sets.push(placed.set);
    }
    Ok(held.map(|exercise| ExerciseIds { exercise, sets }))
}

/// Every canonical exercise that reads as `exercise` now, oldest first.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn reading_as(
    store: &Store,
    exercise: Exercise,
) -> Result<Vec<HeldExercise>, StoreError> {
    store
        .adapter(SqliteCanonicalGymSessionStore::new)
        .call("SqliteCanonicalGymSessionStore::reading_as", |adapter| {
            adapter.reading_as(exercise)
        })
        .await
}

/// Each of these exercises the layer holds, oldest first.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn exercises_held(
    store: &Store,
    exercises: &BTreeSet<CanonicalExerciseId>,
) -> Result<Vec<HeldExercise>, StoreError> {
    store
        .adapter(SqliteCanonicalGymSessionStore::new)
        .call(
            "SqliteCanonicalGymSessionStore::exercises_held",
            |adapter| adapter.exercises_held(exercises),
        )
        .await
}

/// Assert what exercises were, in one assertion.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be written.
pub async fn assert_exercises(
    store: &Store,
    reason: &EditReason,
    edits: &NonEmpty<ExerciseEdit>,
) -> Result<EditId, StoreError> {
    store
        .adapter(SqliteCanonicalGymSessionStore::new)
        .call(
            "SqliteCanonicalGymSessionStore::assert_exercises",
            |adapter| adapter.assert_exercises(Timestamp::now(), reason, edits),
        )
        .await
}

/// Assert what one set was.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be written.
pub async fn assert_set(
    store: &Store,
    reason: &EditReason,
    set: CanonicalSetId,
    edit: SetEdit,
) -> Result<EditId, StoreError> {
    store
        .adapter(SqliteCanonicalGymSessionStore::new)
        .call("SqliteCanonicalGymSessionStore::assert_set", |adapter| {
            adapter.assert_set(Timestamp::now(), reason, set, edit)
        })
        .await
}

/// Every edit, in the order asserted.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn all(store: &Store) -> Result<Vec<Edit>, StoreError> {
    store.canonical_gym().edits().await
}

/// One assertion, by its number, where there is one.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn one(store: &Store, id: EditId) -> Result<Option<Edit>, StoreError> {
    Ok(all(store).await?.into_iter().find(|edit| edit.id == id))
}

/// Retract one assertion whole. `false` if there is none.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be written.
pub async fn retract(store: &Store, id: EditId) -> Result<bool, StoreError> {
    store
        .adapter(SqliteCanonicalGymSessionStore::new)
        .call("SqliteCanonicalGymSessionStore::retract", |adapter| {
            adapter.retract(id)
        })
        .await
}

/// What the layer, as it now stands, leaves the edits for the operator to
/// settle.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn conflicts(store: &Store) -> Result<Vec<Conflict>, StoreError> {
    let edits = Edits::of(&all(store).await?);
    if edits.is_empty() {
        return Ok(Vec::new());
    }
    let held = store
        .adapter(SqliteCanonicalGymSessionStore::new)
        .call(
            "SqliteCanonicalGymSessionStore::held",
            SqliteCanonicalGymSessionStore::held,
        )
        .await?;
    Ok(edits.conflicts(&held))
}

/// Take a set a source added into its exercise's edits.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be written.
pub async fn keep(
    store: &Store,
    exercise: CanonicalExerciseId,
    set: CanonicalSetId,
) -> Result<(), StoreError> {
    store
        .adapter(SqliteCanonicalGymSessionStore::new)
        .call("SqliteCanonicalGymSessionStore::keep", |adapter| {
            adapter.keep(exercise, set)
        })
        .await
}

/// Remove every edit of one exercise.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be written.
pub async fn unedit_exercise(
    store: &Store,
    exercise: CanonicalExerciseId,
) -> Result<(), StoreError> {
    store
        .adapter(SqliteCanonicalGymSessionStore::new)
        .call(
            "SqliteCanonicalGymSessionStore::unedit_exercise",
            |adapter| adapter.unedit_exercise(exercise),
        )
        .await
}

/// Remove every edit of one set.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be written.
pub async fn unedit_set(store: &Store, set: CanonicalSetId) -> Result<(), StoreError> {
    store
        .adapter(SqliteCanonicalGymSessionStore::new)
        .call("SqliteCanonicalGymSessionStore::unedit_set", |adapter| {
            adapter.unedit_set(set)
        })
        .await
}
