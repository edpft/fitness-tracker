//! The operator's edits to the canonical gym layer, and what finds what they
//! reach (§ II.2, #893).
//!
//! **An edit is anchored to a canonical id**, so whichever source a session
//! was drawn from — Hevy, a watch, a spreadsheet, Beyond the Whiteboard — its
//! exercises and sets are edited the same way. A page finds those ids from
//! the session it shows: each set names the source sets it was built from,
//! and the layer records where each was last placed.

use std::collections::{BTreeMap, BTreeSet};

use application::{
    CanonicalGymSessionStore as _, CanonicalLayer, Input, Node, Reads, StoreError,
    graph::NotRaw,
    history::{Number, opened::Activity},
};
use domain::{
    gym::{
        CanonicalExercise, CanonicalGymSession,
        edit::{
            self, Bounds, Conflict, Edit, EditId, EditReason, Edits, Found, SparseGymSession,
            SparseSet,
        },
        exercise::Exercise,
        identity::{
            CanonicalExerciseId, CanonicalSessionId, CanonicalSetId, ExerciseIds, SessionIds,
        },
    },
    normalised::OperatorZone,
};
use infrastructure::SqliteCanonicalGymSessionStore;
use jiff::{Timestamp, civil::Date};

use crate::store::Store;

/// What an edit says one exercise was, and what it read as when edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Renamed {
    pub exercise: CanonicalExerciseId,
    pub read: Exercise,
    pub becomes: Exercise,
}

/// An edit a rebuild left standing on something it did not stand on when it
/// was made, for the operator to settle (§ II.2, #905).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToSettle {
    /// A set a source added under an edited exercise, reading as the edit
    /// says: kept, or the exercise's edit removed.
    Added {
        set: CanonicalSetId,
        edit: Renamed,
        /// Where it is opened; [`None`] where the layer is being rebuilt under
        /// the page.
        at: Option<Located>,
    },
    /// An edited exercise no session holds any more.
    ExerciseGone(Renamed),
    /// An edited set no exercise holds any more.
    SetGone {
        set: CanonicalSetId,
        edit: SparseSet,
    },
    /// A session whose start or end was edited, which no rebuild holds any
    /// more.
    SessionGone(CanonicalSessionId),
    /// A session whose start or end was taken from a source that no longer
    /// accounts for it: it reads as derived until settled.
    SourceGone {
        session: CanonicalSessionId,
        bounds: Bounds,
    },
}

/// What the folded edits say `exercise` was, where they say anything.
fn renamed(edits: &Edits, exercise: CanonicalExerciseId) -> Option<Renamed> {
    edits.exercise(exercise).map(|(becomes, found)| Renamed {
        exercise,
        read: found.read,
        becomes,
    })
}

/// Where a set is opened: its session, and its place among its exercise's
/// sets, from one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Located {
    pub on: Date,
    pub number: Number,
    pub set: usize,
}

/// The edits, and the layer they are applied to, as what is to settle reads
/// them.
pub struct Overlay {
    store: Store,
    zone: OperatorZone,
}

/// **The edits themselves**, which a page otherwise reads only through the
/// layer they are applied to.
impl Input for Overlay {
    fn reads() -> Reads {
        Reads::of([Node::Edits(CanonicalLayer::Gym)])
            .and(<SqliteCanonicalGymSessionStore as Input>::reads())
            .and(crate::history::opened_reads())
    }
}

impl NotRaw for Overlay {}

impl Overlay {
    #[must_use]
    pub fn open(store: &Store, zone: &OperatorZone) -> Self {
        Self {
            store: store.clone(),
            zone: zone.clone(),
        }
    }

    /// What the layer, as it now stands, leaves the edits for the operator
    /// to settle.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn to_settle(&self) -> Result<Vec<ToSettle>, StoreError> {
        let edits = Edits::of(&all(&self.store).await?);
        if edits.is_empty() {
            return Ok(Vec::new());
        }
        let adapter = self.store.adapter(SqliteCanonicalGymSessionStore::new);
        let held = adapter
            .call(
                "SqliteCanonicalGymSessionStore::held",
                SqliteCanonicalGymSessionStore::held,
            )
            .await?;
        let sessions = adapter
            .call(
                "SqliteCanonicalGymSessionStore::sessions_held",
                SqliteCanonicalGymSessionStore::sessions_held,
            )
            .await?;
        let mut settle = Vec::new();
        for conflict in edits.conflicts(&held, &sessions) {
            match conflict {
                Conflict::Added { exercise, set } => {
                    if let Some(edit) = renamed(&edits, exercise) {
                        settle.push(ToSettle::Added {
                            set,
                            edit,
                            at: self.located(exercise, set).await?,
                        });
                    }
                }
                Conflict::ExerciseGone(exercise) => {
                    settle.extend(renamed(&edits, exercise).map(ToSettle::ExerciseGone));
                }
                Conflict::SetGone(set) => {
                    settle.extend(edits.set(set).map(|edit| ToSettle::SetGone { set, edit }));
                }
                Conflict::SessionGone(session) => settle.push(ToSettle::SessionGone(session)),
                Conflict::SourceGone(session) => {
                    settle.extend(edits.bounds(session).map(|bounds| ToSettle::SourceGone {
                        session,
                        bounds: bounds.clone(),
                    }));
                }
            }
        }
        Ok(settle)
    }

    /// Where `set`, held by `exercise`, is opened: found by opening each
    /// activity of its day in turn, which is rare enough to afford.
    async fn located(
        &self,
        exercise: CanonicalExerciseId,
        set: CanonicalSetId,
    ) -> Result<Option<Located>, StoreError> {
        let Some(held) = exercises_held(&self.store, &BTreeSet::from([exercise]))
            .await?
            .into_iter()
            .next()
        else {
            return Ok(None);
        };
        let opened = crate::history::Opened::open(&self.store, &self.zone);
        let mut counted = 1;
        while let Ok(number) = Number::try_from(counted) {
            counted += 1;
            match opened.activity(held.day, number).await? {
                None => break,
                Some(Activity::Gym { session, .. }) => {
                    for read in session.exercises() {
                        let Some((_, ids)) = ids_of(&self.store, read).await? else {
                            continue;
                        };
                        if ids.exercise == exercise {
                            return Ok(ids.sets.iter().position(|held| *held == set).map(
                                |index| Located {
                                    on: held.day,
                                    number,
                                    set: index + 1,
                                },
                            ));
                        }
                    }
                }
                Some(_) => {}
            }
        }
        Ok(None)
    }
}

/// An edit in force on an opened session, and where on it (#895).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InForce {
    pub edit: Edit,
    pub here: Here,
    /// How many sessions it reaches, this one among them: what undoing it
    /// undoes, since it is retracted as it was made.
    pub sessions: usize,
}

/// Where on the session an edit says something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Here {
    /// When it started or ended.
    Bounds(Bounds),
    /// One of its exercises: what it read as when it was changed, and what
    /// it became.
    Exercise { read: Exercise, becomes: Exercise },
    /// The set at this place, from one, of the exercise at `position`.
    Set {
        position: usize,
        set: usize,
        edit: SparseSet,
    },
}

impl Overlay {
    /// The edits in force on `session`, newest first, with where on it each
    /// says something; an exercise whose sets the layer is placing under the
    /// page has none listed.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn in_force(
        &self,
        session: &CanonicalGymSession,
    ) -> Result<Vec<InForce>, StoreError> {
        let every = all(&self.store).await?;
        if every.is_empty() {
            return Ok(Vec::new());
        }
        let mut placed: Vec<(usize, ExerciseIds)> = Vec::new();
        let mut held: Option<CanonicalSessionId> = None;
        for (index, exercise) in session.exercises().enumerate() {
            if let Some((session, ids)) = ids_of(&self.store, exercise).await? {
                held.get_or_insert(session);
                placed.push((index + 1, ids));
            }
        }
        let Some(session) = held else {
            return Ok(Vec::new());
        };
        let ids = SessionIds {
            session,
            exercises: placed.iter().map(|(_, ids)| ids.clone()).collect(),
        };
        Ok(edit::in_force(&every, &ids)
            .into_iter()
            .filter_map(|edit| {
                here(edit, session, &placed).map(|here| InForce {
                    edit: edit.clone(),
                    here,
                    sessions: edit.reaches(),
                })
            })
            .collect())
    }
}

/// Where on a session holding `placed` an edit says something: an exercise
/// it renames, before a set it changes.
fn here(edit: &Edit, session: CanonicalSessionId, placed: &[(usize, ExerciseIds)]) -> Option<Here> {
    let bounds = edit
        .bounds()
        .find(|(id, _, _)| *id == session)
        .map(|(_, started, lasted)| {
            Here::Bounds(Bounds {
                started: started.cloned(),
                lasted: lasted.cloned(),
            })
        });
    if bounds.is_some() {
        return bounds;
    }
    let renamed = edit.exercises().find_map(|(exercise, becomes)| {
        let found = edit.found.get(&exercise)?;
        placed
            .iter()
            .any(|(_, ids)| ids.exercise == exercise)
            .then_some(Here::Exercise {
                read: found.read,
                becomes,
            })
    });
    renamed.or_else(|| {
        edit.sets().find_map(|(set, fields)| {
            placed.iter().find_map(|(position, ids)| {
                ids.sets
                    .iter()
                    .position(|held| *held == set)
                    .map(|index| Here::Set {
                        position: *position,
                        set: index + 1,
                        edit: fields,
                    })
            })
        })
    })
}

/// One canonical exercise as an edit finds it: its day and its sets.
pub use infrastructure::HeldExercise;

/// The session an exercise read from the layer is in, and the ids it holds,
/// found by its sets' sources: [`None`] where one of its sets was placed
/// nowhere, which is a layer being rebuilt under the page.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn ids_of(
    store: &Store,
    exercise: &CanonicalExercise,
) -> Result<Option<(CanonicalSessionId, ExerciseIds)>, StoreError> {
    let adapter = store.adapter(SqliteCanonicalGymSessionStore::new);
    let mut held: Option<(CanonicalSessionId, CanonicalExerciseId)> = None;
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
        if *held.get_or_insert((placed.session, placed.exercise))
            != (placed.session, placed.exercise)
        {
            return Ok(None);
        }
        sets.push(placed.set);
    }
    Ok(held.map(|(session, exercise)| (session, ExerciseIds { exercise, sets })))
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

/// Assert what sessions were, in one assertion.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be written.
pub async fn assert(
    store: &Store,
    reason: &EditReason,
    sessions: &BTreeMap<CanonicalSessionId, SparseGymSession>,
    found: &BTreeMap<CanonicalExerciseId, Found>,
) -> Result<EditId, StoreError> {
    store
        .adapter(SqliteCanonicalGymSessionStore::new)
        .call("SqliteCanonicalGymSessionStore::assert", |adapter| {
            adapter.assert(Timestamp::now(), reason, sessions, found)
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

/// Remove every edit of when one session started and ended.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be written.
pub async fn unedit_bounds(store: &Store, session: CanonicalSessionId) -> Result<(), StoreError> {
    store
        .adapter(SqliteCanonicalGymSessionStore::new)
        .call("SqliteCanonicalGymSessionStore::unedit_bounds", |adapter| {
            adapter.unedit_bounds(session)
        })
        .await
}
