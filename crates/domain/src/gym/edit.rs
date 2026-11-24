//! The operator's edits to the canonical gym layer (§ II.2, #893).
//!
//! **The overlay is a sparse canonical gym session**, applied on top of the
//! canonical layer and keyed by the ids its entities keep across every
//! rebuild ([`super::identity`]). The operator, 2026-11-21: *"I think edits
//! should be applied on top of the canonical layer, that's why I keep saying
//! they are sparse canonical gym sessions that can edit any field in a
//! canonical gym session."* Anchoring one to a source instead meant giving
//! sources fields they never had, such as an effort on a watch's set; on the
//! canonical set it is one more field, whichever source the rest came from.
//!
//! **Sparse.** An edit holds only what it changes, and the rest of the entity
//! stands as derived. A field an edit states is attributed to it
//! ([`crate::canonical::TakenFrom::Edit`]) rather than to a normalised session
//! it never came from.
//!
//! **One assertion, one or many sessions.** "Similar" is how one assertion
//! reaches many exercises: the same field in each session found reading as
//! the same thing, under one [`EditId`] so it is retracted as it was made.
//!
//! **Where a rebuild changes what an edit stands on, the edit stands and the
//! conflict is raised** ([`Conflict`]): a set a source adds to an edited
//! exercise, or an edited entity no rebuild holds any more. The operator chose
//! that over locking or clearing: *"it would be best to deal with these cases
//! on a case by case basis."*

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use jiff::Timestamp;

use crate::canonical::{Attributed, Occurred, TakenFrom};
use crate::landing::LandingStream;
use crate::measure::{Distance, Duration, Kg, PositiveDuration, RepCount};

use super::canonical::{
    CanonicalExercise, CanonicalGymSession, CanonicalSet, Identified, InvalidBounds, between,
};
use super::exercise::Exercise;
use super::identity::{
    CanonicalExerciseId, CanonicalSessionId, CanonicalSetId, ExerciseIds, SessionIds,
};
use super::intensity::Rir;
use super::load::{Load, SignedKg};
use super::outcome::Performed;

pub use crate::canonical::edit::{EditId, EditReason};

/// What a set's load was, as the operator states it.
///
/// **The axis is the set's.** A mass on a set read as bodyweight plus or minus
/// something is added to the body; on any other it is the load. Assistance is
/// a mass taken off, whatever the set read as: a band-assisted front lever
/// logged against a template that records weight added had nowhere to put the
/// band (#453).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatedLoad {
    /// Nothing beyond the body itself.
    Nothing,
    /// A mass, on the set's own axis.
    Mass(Kg),
    /// A mass taken off.
    Assistance(Kg),
}

impl StatedLoad {
    /// How the operator would state `load`.
    pub const fn of(load: Load) -> Self {
        match load {
            Load::Absolute(mass) if mass.is_none() => Self::Nothing,
            Load::Absolute(mass) => Self::Mass(mass),
            Load::Relative(delta) => match delta.as_grams() {
                0 => Self::Nothing,
                grams if grams < 0 => Self::Assistance(Kg::from_grams(grams.unsigned_abs())),
                grams => Self::Mass(Kg::from_grams(grams.unsigned_abs())),
            },
        }
    }

    /// The load this states, on a set that now reads `current`.
    pub fn on(self, current: Option<Load>) -> Load {
        let relative = matches!(current, Some(Load::Relative(_)));
        match self {
            Self::Nothing if relative => Load::BODYWEIGHT,
            Self::Nothing => Load::UNLOADED,
            Self::Mass(mass) if relative => Load::relative(SignedKg::from(mass)),
            Self::Mass(mass) => Load::absolute(mass),
            Self::Assistance(mass) => Load::relative(SignedKg::from(mass).negated()),
        }
    }
}

impl fmt::Display for StatedLoad {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nothing => f.write_str("no load"),
            Self::Mass(mass) => write!(f, "{mass} kg"),
            Self::Assistance(mass) => write!(f, "{mass} kg of assistance"),
        }
    }
}

/// How much of a set was done, in whatever its exercise is counted in: reps,
/// a length of time, or ground covered (#901).
///
/// **One per kind of [`Exercise`]**, so a sparse set holds the measure of the
/// set it edits, and a measure of another kind reaches nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Measure {
    Reps(RepCount),
    Duration(Duration),
    Distance(Distance),
}

impl Measure {
    const fn reps(self) -> Option<RepCount> {
        match self {
            Self::Reps(reps) => Some(reps),
            Self::Duration(_) | Self::Distance(_) => None,
        }
    }

    const fn duration(self) -> Option<Duration> {
        match self {
            Self::Duration(duration) => Some(duration),
            Self::Reps(_) | Self::Distance(_) => None,
        }
    }

    const fn distance(self) -> Option<Distance> {
        match self {
            Self::Distance(distance) => Some(distance),
            Self::Reps(_) | Self::Duration(_) => None,
        }
    }
}

impl fmt::Display for Measure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reps(reps) => write!(f, "{reps} reps"),
            Self::Duration(duration) => write!(f, "{} s", duration.as_seconds()),
            Self::Distance(distance) => write!(f, "{distance}"),
        }
    }
}

/// One set of a canonical gym session, sparse: each field is present only
/// where the operator changed it (#892, #901).
///
/// **The canonical set's own fields**, as [`CanonicalSet`] names them, each
/// optional. **Never empty**: a set the operator changed nothing about is
/// not in the overlay, so [`SparseSet::new`] refuses one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SparseSet {
    measure: Option<Measure>,
    load: Option<Load>,
    intensity: Option<Rir>,
}

/// One field of a set the overlay can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SetField {
    Measure,
    Load,
    Intensity,
}

impl SparseSet {
    /// The fields changed, or [`None`] where none is.
    #[must_use]
    pub const fn new(
        measure: Option<Measure>,
        load: Option<Load>,
        intensity: Option<Rir>,
    ) -> Option<Self> {
        if measure.is_none() && load.is_none() && intensity.is_none() {
            return None;
        }
        Some(Self {
            measure,
            load,
            intensity,
        })
    }

    pub const fn measure(self) -> Option<Measure> {
        self.measure
    }

    pub const fn load(self) -> Option<Load> {
        self.load
    }

    pub const fn intensity(self) -> Option<Rir> {
        self.intensity
    }

    /// The fields it holds.
    fn stated(self) -> impl Iterator<Item = SetField> {
        [
            self.measure.map(|_| SetField::Measure),
            self.load.map(|_| SetField::Load),
            self.intensity.map(|_| SetField::Intensity),
        ]
        .into_iter()
        .flatten()
    }

    /// This set with a later one's fields over it: what the later one
    /// changed, it changes; what it left, this one's still says.
    #[must_use]
    pub const fn then(self, later: Self) -> Self {
        Self {
            measure: match later.measure {
                Some(measure) => Some(measure),
                None => self.measure,
            },
            load: match later.load {
                Some(load) => Some(load),
                None => self.load,
            },
            intensity: match later.intensity {
                Some(intensity) => Some(intensity),
                None => self.intensity,
            },
        }
    }

    /// `set` with each field this holds over it, attributed to the edit. A
    /// measure reaches only a set counted in it, which `counted` picks out.
    fn over<M: Clone>(
        self,
        set: &CanonicalSet<M>,
        counted: impl Fn(Measure) -> Option<M>,
    ) -> CanonicalSet<M> {
        let mut edited = set.clone();
        if let Some(measure) = self.measure.and_then(counted) {
            edited.outcome = Attributed::edited(Performed::Completed(Some(measure)));
        }
        if let Some(load) = self.load {
            edited.load = Some(Attributed::edited(load));
        }
        if let Some(intensity) = self.intensity {
            edited.intensity = Some(Attributed::edited(intensity));
        }
        edited
    }
}

impl fmt::Display for SparseSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if let Some(measure) = self.measure {
            parts.push(measure.to_string());
        }
        if let Some(load) = self.load {
            parts.push(load.to_string());
        }
        if let Some(intensity) = self.intensity {
            parts.push(format!("{intensity} in reserve"));
        }
        f.write_str(&parts.join(", "))
    }
}

/// One exercise of a canonical gym session, sparse: what it was, where the
/// operator changed that, and the sets of it he changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SparseExercise {
    pub exercise: Option<Exercise>,
    pub sets: BTreeMap<CanonicalSetId, SparseSet>,
}

impl SparseExercise {
    /// One that says only what the exercise was.
    #[must_use]
    pub const fn was(exercise: Exercise) -> Self {
        Self {
            exercise: Some(exercise),
            sets: BTreeMap::new(),
        }
    }

    /// One that says only what one of its sets was.
    #[must_use]
    pub fn set(set: CanonicalSetId, fields: SparseSet) -> Self {
        Self {
            exercise: None,
            sets: BTreeMap::from([(set, fields)]),
        }
    }

    /// This exercise with a later one over it, field by field and set by set.
    fn then(&mut self, later: &Self) {
        if let Some(exercise) = later.exercise {
            self.exercise = Some(exercise);
        }
        for (set, fields) in &later.sets {
            let merged = self
                .sets
                .get(set)
                .map_or(*fields, |held| held.then(*fields));
            self.sets.insert(*set, merged);
        }
    }
}

/// A canonical gym session, sparse: a field is present only where the
/// operator changed it, and its presence is the edit.
///
/// The operator, 2026-11-21: *"the edit overlay is a sparse canonical gym
/// session. Any field in a canonical gym session can be edited. It's presence
/// in the edit overlay is the signal that it's a correctness. Fields that
/// aren't included aren't edited."* Keyed by the ids each entity keeps
/// across a rebuild ([`super::identity`]), since what the layer holds in
/// order the overlay holds by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SparseGymSession {
    /// When it started.
    pub started: Option<Taken<Occurred>>,
    /// How long it lasted: until a source's end, or for as long as the
    /// operator says. A start and a duration, never an end (operator,
    /// 2026-11-24); see [`moved`].
    pub lasted: Option<Taken<PositiveDuration>>,
    pub exercises: BTreeMap<CanonicalExerciseId, SparseExercise>,
}

impl SparseGymSession {
    /// One that says something about one exercise and nothing else.
    #[must_use]
    pub fn exercise(exercise: CanonicalExerciseId, sparse: SparseExercise) -> Self {
        Self {
            exercises: BTreeMap::from([(exercise, sparse)]),
            ..Self::default()
        }
    }

    /// One that says when it started or how long it lasted, and nothing
    /// else.
    #[must_use]
    pub const fn bounds(
        started: Option<Taken<Occurred>>,
        lasted: Option<Taken<PositiveDuration>>,
    ) -> Self {
        Self {
            started,
            lasted,
            exercises: BTreeMap::new(),
        }
    }

    /// This session with a later one over it, field by field.
    fn then(&mut self, later: &Self) {
        if let Some(started) = &later.started {
            self.started = Some(started.clone());
        }
        if let Some(lasted) = &later.lasted {
            self.lasted = Some(lasted.clone());
        }
        for (exercise, sparse) in &later.exercises {
            self.exercises.entry(*exercise).or_default().then(sparse);
        }
    }
}

/// Where a field's value is taken from: one source's account of it, or the
/// operator (#894).
///
/// The operator, 2026-11-23: *"choose the source of the start or end time or
/// provide my own."* **A source is named by its stream**, since a rebuild
/// reissues every normalised id (#890) and "the watch" has to mean the watch
/// after one. **On a length, a source's is its end**: what he chooses is
/// where the session ended, and the length is from the start in force to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Taken<T> {
    From(LandingStream),
    Stated(T),
}

impl<T> Taken<T> {
    /// The source it is taken from, where it is a source's.
    pub const fn source(&self) -> Option<&LandingStream> {
        match self {
            Self::From(stream) => Some(stream),
            Self::Stated(_) => None,
        }
    }
}

impl<T: fmt::Display> fmt::Display for Taken<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::From(stream) => write!(f, "{stream}'s"),
            Self::Stated(value) => write!(f, "{value}"),
        }
    }
}

/// An exercise as an edit of what it was found it: what it read as, and the
/// sets it held.
///
/// **Bookkeeping, not a field of the session.** The first says what was
/// changed; the second is how a set a source adds later is told apart from
/// the sets the operator ruled on, so it is raised rather than silently taken
/// in (§ II.2, *"overrides do not propagate"*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub read: Exercise,
    pub sets: BTreeSet<CanonicalSetId>,
}

/// One assertion by the operator: when, why, and what it says the sessions
/// were.
///
/// **Usually one session.** "Similar ones too" is several, under one
/// [`EditId`], so it is retracted as it was made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub id: EditId,
    pub asserted_at: Timestamp,
    pub reason: EditReason,
    pub sessions: BTreeMap<CanonicalSessionId, SparseGymSession>,
    /// How each exercise whose name it states was found.
    pub found: BTreeMap<CanonicalExerciseId, Found>,
}

impl Edit {
    /// How many sessions it reaches.
    #[must_use]
    pub fn reaches(&self) -> usize {
        self.sessions.len()
    }

    /// Every exercise whose name it states, with what it says it was.
    pub fn exercises(&self) -> impl Iterator<Item = (CanonicalExerciseId, Exercise)> + '_ {
        self.sessions.values().flat_map(|session| {
            session
                .exercises
                .iter()
                .filter_map(|(id, sparse)| sparse.exercise.map(|exercise| (*id, exercise)))
        })
    }

    /// When it says a session started and how long it lasted, where it says
    /// either.
    pub fn bounds(
        &self,
    ) -> impl Iterator<
        Item = (
            CanonicalSessionId,
            Option<&Taken<Occurred>>,
            Option<&Taken<PositiveDuration>>,
        ),
    > + '_ {
        self.sessions
            .iter()
            .filter(|(_, session)| session.started.is_some() || session.lasted.is_some())
            .map(|(id, session)| (*id, session.started.as_ref(), session.lasted.as_ref()))
    }

    /// Every set it says something about, with what.
    pub fn sets(&self) -> impl Iterator<Item = (CanonicalSetId, SparseSet)> + '_ {
        self.sessions.values().flat_map(|session| {
            session
                .exercises
                .values()
                .flat_map(|exercise| exercise.sets.iter().map(|(id, set)| (*id, *set)))
        })
    }
}

/// One field of a session the overlay can hold, named by the entity it is
/// on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Field {
    Started(CanonicalSessionId),
    Lasted(CanonicalSessionId),
    Exercise(CanonicalExerciseId),
    Set(CanonicalSetId, SetField),
}

impl Edit {
    /// How many fields it holds, which narrowing it lowers.
    #[must_use]
    pub fn holds(&self) -> usize {
        self.fields().count()
    }

    /// Each field it holds.
    fn fields(&self) -> impl Iterator<Item = Field> + '_ {
        self.sessions
            .iter()
            .flat_map(|(id, session)| {
                [
                    session.started.as_ref().map(|_| Field::Started(*id)),
                    session.lasted.as_ref().map(|_| Field::Lasted(*id)),
                ]
            })
            .flatten()
            .chain(
                self.exercises()
                    .map(|(exercise, _)| Field::Exercise(exercise)),
            )
            .chain(
                self.sets()
                    .flat_map(|(set, fields)| fields.stated().map(move |f| Field::Set(set, f))),
            )
    }
}

/// Which of `edits` are in force on a session written under `ids`, newest
/// first (#895).
///
/// **In force is what still says something about the session**: an edit is
/// listed while it is the latest to hold at least one of the session's
/// fields. One a later edit has wholly overruled is not listed, since undoing
/// it would change nothing; undoing the later one brings it back.
#[must_use]
pub fn in_force<'a>(edits: &'a [Edit], ids: &SessionIds) -> Vec<&'a Edit> {
    let mut ordered: Vec<&Edit> = edits.iter().collect();
    ordered.sort_by_key(|edit| (edit.asserted_at, edit.id));
    let exercises: BTreeSet<CanonicalExerciseId> =
        ids.exercises.iter().map(|held| held.exercise).collect();
    let sets: BTreeSet<CanonicalSetId> = ids
        .exercises
        .iter()
        .flat_map(|held| held.sets.iter().copied())
        .collect();
    let on_session = |field: &Field| match field {
        Field::Started(session) | Field::Lasted(session) => *session == ids.session,
        Field::Exercise(exercise) => exercises.contains(exercise),
        Field::Set(set, _) => sets.contains(set),
    };
    let mut latest: BTreeMap<Field, EditId> = BTreeMap::new();
    for edit in &ordered {
        for field in edit.fields().filter(on_session) {
            latest.insert(field, edit.id);
        }
    }
    let standing: BTreeSet<EditId> = latest.into_values().collect();
    ordered
        .into_iter()
        .rev()
        .filter(|edit| standing.contains(&edit.id))
        .collect()
}

/// What a rebuild changed under an edit, for the operator to settle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Conflict {
    /// A set a source added to an exercise after it was edited: it reads as
    /// the edit says, and he has not said it should.
    Added {
        exercise: CanonicalExerciseId,
        set: CanonicalSetId,
    },
    /// An edited exercise no session holds any more.
    ExerciseGone(CanonicalExerciseId),
    /// An edited set no exercise holds any more.
    SetGone(CanonicalSetId),
    /// A session whose start or end was edited, which no rebuild holds any
    /// more.
    SessionGone(CanonicalSessionId),
    /// A session whose start or end was taken from a source that no longer
    /// accounts for it.
    SourceGone(CanonicalSessionId),
}

/// Every edit in force, folded into one sparse session each, ready for a
/// rebuild to apply.
///
/// **Later wins field by field**: the operator changing his mind, and each
/// edit holding only what it changed. Ordering is by when they were asserted,
/// so the result does not depend on the order the store hands them back in.
///
/// **Applied by the ids the entities keep**, whichever session holds them
/// now: an exercise follows its sets (§ [`super::identity`]), so the session
/// an edit names is where it was, which is where it is unless a rebuild moved
/// it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Edits {
    bounds: BTreeMap<CanonicalSessionId, Bounds>,
    exercises: BTreeMap<CanonicalExerciseId, (Exercise, Found)>,
    sets: BTreeMap<CanonicalSetId, SparseSet>,
}

/// When the edits say a session started and how long it lasted, where they
/// say either.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bounds {
    pub started: Option<Taken<Occurred>>,
    pub lasted: Option<Taken<PositiveDuration>>,
}

impl Edits {
    #[must_use]
    pub fn of(edits: &[Edit]) -> Self {
        let mut ordered: Vec<&Edit> = edits.iter().collect();
        ordered.sort_by_key(|edit| (edit.asserted_at, edit.id));
        let mut folded: BTreeMap<CanonicalSessionId, SparseGymSession> = BTreeMap::new();
        let mut found: BTreeMap<CanonicalExerciseId, Found> = BTreeMap::new();
        for edit in ordered {
            for (session, sparse) in &edit.sessions {
                folded.entry(*session).or_default().then(sparse);
            }
            for (exercise, as_found) in &edit.found {
                found.insert(*exercise, as_found.clone());
            }
        }
        let mut bounds = BTreeMap::new();
        let mut exercises = BTreeMap::new();
        let mut sets = BTreeMap::new();
        for (id, session) in folded {
            if session.started.is_some() || session.lasted.is_some() {
                bounds.insert(
                    id,
                    Bounds {
                        started: session.started,
                        lasted: session.lasted,
                    },
                );
            }
            for (id, exercise) in session.exercises {
                if let (Some(becomes), Some(as_found)) = (exercise.exercise, found.get(&id)) {
                    exercises.insert(id, (becomes, as_found.clone()));
                }
                sets.extend(exercise.sets);
            }
        }
        Self {
            bounds,
            exercises,
            sets,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bounds.is_empty() && self.exercises.is_empty() && self.sets.is_empty()
    }

    /// When the edits say one session started and ended, where they say
    /// either.
    #[must_use]
    pub fn bounds(&self, session: CanonicalSessionId) -> Option<&Bounds> {
        self.bounds.get(&session)
    }

    /// What one exercise was, and how the edit found it, where there is an
    /// edit of it.
    #[must_use]
    pub fn exercise(&self, exercise: CanonicalExerciseId) -> Option<(Exercise, &Found)> {
        self.exercises
            .get(&exercise)
            .map(|(becomes, found)| (*becomes, found))
    }

    /// What one set was, where there is an edit of it.
    #[must_use]
    pub fn set(&self, set: CanonicalSetId) -> Option<SparseSet> {
        self.sets.get(&set).copied()
    }

    /// Whether any edit reaches a session written under `ids`.
    #[must_use]
    pub fn reach(&self, ids: &SessionIds) -> bool {
        self.bounds.contains_key(&ids.session)
            || ids.exercises.iter().any(|exercise| {
                self.exercises.contains_key(&exercise.exercise)
                    || exercise.sets.iter().any(|set| self.sets.contains_key(set))
            })
    }

    /// `session`, written under `ids`, as the edits say it was.
    ///
    /// **A start the edits move, with no length of their own, keeps the
    /// end**: a late press on the watch moves when it started and nothing
    /// else.
    ///
    /// **A start or end the session cannot take is left as derived** — one
    /// taken from a source that no longer accounts for it, or bounds a
    /// rebuild has made impossible — and [`Self::conflicts`] raises the
    /// first; the pen refuses the second before it is ever asserted.
    #[must_use]
    pub fn apply(&self, session: &CanonicalGymSession, ids: &SessionIds) -> CanonicalGymSession {
        let mut exercise_ids = ids.exercises.iter();
        let edited = session.map_exercises(|exercise| {
            exercise_ids.next().map_or_else(
                || exercise.clone(),
                |held| self.exercise_over(exercise, held),
            )
        });
        let Some(bounds) = self.bounds.get(&ids.session) else {
            return edited;
        };
        let started = bounds
            .started
            .as_ref()
            .and_then(|taken| started(&edited, taken))
            .unwrap_or_else(|| edited.started().clone());
        let lasted = bounds
            .lasted
            .as_ref()
            .and_then(|taken| lasted(&edited, started.value(), taken))
            .or_else(|| {
                let end = edited.ended()?;
                let start = started.value().instant()?.instant();
                Some(Attributed::new(
                    between(start, end.copied())?,
                    end.taken_from(),
                ))
            });
        edited.bounded(started, lasted).unwrap_or(edited)
    }

    fn exercise_over(&self, exercise: &CanonicalExercise, ids: &ExerciseIds) -> CanonicalExercise {
        let becomes = self
            .exercises
            .get(&ids.exercise)
            .map(|(becomes, _)| *becomes);
        let set = |index: usize| {
            ids.sets
                .get(index)
                .and_then(|id| self.sets.get(id))
                .copied()
        };
        match exercise {
            CanonicalExercise::ForReps { identified, sets } => CanonicalExercise::ForReps {
                identified: match becomes {
                    Some(Exercise::Reps(named)) => Attributed::edited(Identified::Recorded(named)),
                    _ => *identified,
                },
                sets: sets.map_indexed(|index, held| {
                    set(index).map_or_else(|| held.clone(), |edit| edit.over(held, Measure::reps))
                }),
            },
            CanonicalExercise::ForDuration {
                exercise: named,
                sets,
            } => CanonicalExercise::ForDuration {
                exercise: match becomes {
                    Some(Exercise::Duration(other)) => Attributed::edited(other),
                    _ => *named,
                },
                sets: sets.map_indexed(|index, held| {
                    set(index)
                        .map_or_else(|| held.clone(), |edit| edit.over(held, Measure::duration))
                }),
            },
            CanonicalExercise::ForDistance {
                exercise: named,
                sets,
            } => CanonicalExercise::ForDistance {
                exercise: match becomes {
                    Some(Exercise::Distance(other)) => Attributed::edited(other),
                    _ => *named,
                },
                sets: sets.map_indexed(|index, held| {
                    set(index)
                        .map_or_else(|| held.clone(), |edit| edit.over(held, Measure::distance))
                }),
            },
        }
    }

    /// What the layer holding `held` — each exercise's id and its sets' —
    /// and `sessions` — each session's id and the streams accounting for it
    /// — leaves for the operator to settle.
    #[must_use]
    pub fn conflicts(
        &self,
        held: &BTreeMap<CanonicalExerciseId, BTreeSet<CanonicalSetId>>,
        sessions: &BTreeMap<CanonicalSessionId, BTreeSet<LandingStream>>,
    ) -> Vec<Conflict> {
        let mut conflicts = Vec::new();
        for (session, bounds) in &self.bounds {
            let Some(streams) = sessions.get(session) else {
                conflicts.push(Conflict::SessionGone(*session));
                continue;
            };
            let gone =
                |stream: Option<&LandingStream>| stream.is_some_and(|s| !streams.contains(s));
            if gone(bounds.started.as_ref().and_then(Taken::source))
                || gone(bounds.lasted.as_ref().and_then(Taken::source))
            {
                conflicts.push(Conflict::SourceGone(*session));
            }
        }
        for (exercise, (_, found)) in &self.exercises {
            match held.get(exercise) {
                None => conflicts.push(Conflict::ExerciseGone(*exercise)),
                Some(sets) => {
                    conflicts.extend(sets.difference(&found.sets).map(|set| Conflict::Added {
                        exercise: *exercise,
                        set: *set,
                    }));
                }
            }
        }
        let every: BTreeSet<CanonicalSetId> = held.values().flatten().copied().collect();
        conflicts.extend(
            self.sets
                .keys()
                .filter(|set| !every.contains(set))
                .map(|set| Conflict::SetGone(*set)),
        );
        conflicts
    }
}

/// The start `taken` gives `session`: a source's own, or the operator's.
/// [`None`] where no account of that source has one.
#[must_use]
pub fn started(
    session: &CanonicalGymSession,
    taken: &Taken<Occurred>,
) -> Option<Attributed<Occurred, TakenFrom>> {
    match taken {
        Taken::Stated(occurred) => Some(Attributed::edited(occurred.clone())),
        Taken::From(stream) => session
            .accounts()
            .iter()
            .find(|account| account.stream == *stream && account.occurred.instant().is_some())
            .map(|account| Attributed::new(account.occurred.clone(), account.normalised)),
    }
}

/// The end `taken` gives `session`: a source's own, or the operator's.
/// [`None`] where no account of that source has one.
#[must_use]
pub fn ended(
    session: &CanonicalGymSession,
    taken: &Taken<Timestamp>,
) -> Option<Attributed<Timestamp, TakenFrom>> {
    match taken {
        Taken::Stated(end) => Some(Attributed::edited(*end)),
        Taken::From(stream) => session
            .accounts()
            .iter()
            .filter(|account| account.stream == *stream)
            .find_map(|account| Some(Attributed::new(account.ended()?, account.normalised))),
    }
}

/// How long `session`, started at `start`, lasted by `taken`: until a
/// source's end, or as long as the operator said. [`None`] where no account
/// of that source has an end after it.
fn lasted(
    session: &CanonicalGymSession,
    start: &Occurred,
    taken: &Taken<PositiveDuration>,
) -> Option<Attributed<PositiveDuration, TakenFrom>> {
    match taken {
        Taken::Stated(length) => Some(Attributed::edited(*length)),
        Taken::From(stream) => {
            let end = ended(session, &Taken::From(stream.clone()))?;
            let length = between(start.instant()?.instant(), end.copied())?;
            Some(Attributed::new(length, end.taken_from()))
        }
    }
}

/// Why a change to when a session started and ended is not made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Unmoved {
    #[error("that source does not say when it started")]
    NoStart,
    #[error("that source does not say when it ended")]
    NoEnd,
    #[error("nothing was changed")]
    Unchanged,
    #[error("Not changed: {0}.")]
    Invalid(#[from] InvalidBounds),
}

/// What the operator asked of when `session` started and ended, as an edit
/// holds it: each bound that differs from what the session reads as now,
/// the end as how long it lasted.
///
/// **He deals in times, the edit in a start and a length.** An end he types
/// is a length from the start it will have. A start he moves alone keeps an
/// end he typed before, by stating the length again; an end taken from a
/// source, or derived, follows by itself.
///
/// # Errors
///
/// [`Unmoved`]: a source that says nothing of the bound chosen, nothing
/// changed, or bounds the session cannot take.
pub fn moved(
    session: &CanonicalGymSession,
    started: Option<Taken<Occurred>>,
    ended_at: Option<Taken<Timestamp>>,
) -> Result<SparseGymSession, Unmoved> {
    let start = match &started {
        Some(taken) => self::started(session, taken).ok_or(Unmoved::NoStart)?,
        None => session.started().clone(),
    };
    let end = match &ended_at {
        Some(taken) => Some(ended(session, taken).ok_or(Unmoved::NoEnd)?),
        None => session.ended(),
    };
    let length = match (&end, start.value().instant()) {
        (Some(end), Some(at)) => Some(Attributed::new(
            between(at.instant(), end.copied()).ok_or(InvalidBounds::EndsBeforeItStarts)?,
            end.taken_from(),
        )),
        (Some(_), None) => return Err(InvalidBounds::EndsWithoutAStart.into()),
        (None, _) => None,
    };
    session.bounded(start.clone(), length)?;
    let moves = start.value() != session.occurred();
    let ends = end.map(|end| end.copied()) != session.ended().map(|end| end.copied());
    let started = started.filter(|_| moves);
    let lasted = match (ended_at, length) {
        (Some(Taken::From(stream)), _) if ends => Some(Taken::From(stream)),
        (Some(Taken::Stated(_)), Some(length)) if ends => Some(Taken::Stated(length.copied())),
        _ if moves && session.lasted().is_some_and(Attributed::is_edited) => {
            length.map(|length| Taken::Stated(length.copied()))
        }
        _ => None,
    };
    if started.is_none() && lasted.is_none() {
        return Err(Unmoved::Unchanged);
    }
    Ok(SparseGymSession::bounds(started, lasted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::NormalisedSessionId;
    use crate::gym::canonical::CanonicalItem;
    use crate::gym::exercise::{DurationExercise, RepsExercise};
    use crate::sequence::NonEmpty;

    fn exercise_id(id: i64) -> CanonicalExerciseId {
        CanonicalExerciseId::try_from(id).expect("an id")
    }

    fn set_id(id: i64) -> CanonicalSetId {
        CanonicalSetId::try_from(id).expect("an id")
    }

    fn reps(count: u32) -> RepCount {
        RepCount::new(count).expect("a count")
    }

    fn set(count: u32, kilos: u64) -> CanonicalSet<RepCount> {
        let sheet = NormalisedSessionId::FIRST;
        CanonicalSet {
            outcome: Attributed::new(Performed::Completed(Some(reps(count))), sheet),
            load: Some(Attributed::new(
                Load::absolute(Kg::from_grams(kilos * 1_000)),
                sheet,
            )),
            began: None,
            intensity: None,
            kind: None,
            rest_after: None,
            sources: BTreeSet::new(),
        }
    }

    /// 2020-10-09's deadlift, as a sheet recorded it: two sets, no effort.
    fn session() -> CanonicalGymSession {
        CanonicalGymSession::new(
            crate::canonical::Attributed::new(
                Occurred::On("2020-10-09".parse().expect("a day")),
                crate::canonical::NormalisedSessionId::FIRST,
            ),
            NonEmpty::of(
                CanonicalItem::exercise(CanonicalExercise::ForReps {
                    identified: Attributed::new(
                        Identified::Recorded(RepsExercise::DeadliftBarbell),
                        NormalisedSessionId::FIRST,
                    ),
                    sets: NonEmpty::of(set(4, 63), vec![set(4, 65)]),
                }),
                vec![],
            ),
            None,
            None,
            Vec::new(),
        )
    }

    fn ids() -> SessionIds {
        SessionIds {
            session: session_id(),
            exercises: vec![ExerciseIds {
                exercise: exercise_id(10),
                sets: vec![set_id(100), set_id(101)],
            }],
        }
    }

    fn at(instant: &str) -> Timestamp {
        instant.parse().expect("an instant")
    }

    fn reason() -> EditReason {
        EditReason::try_from("forgot to rate it".to_owned()).expect("a reason")
    }

    fn session_id() -> CanonicalSessionId {
        CanonicalSessionId::try_from(1).expect("an id")
    }

    /// An edit of one set of exercise 10 of session 1, or of exercise 99
    /// where the set is not one of exercise 10's.
    fn set_edit(id: i64, when: &str, set: i64, fields: SparseSet) -> Edit {
        let exercise = if set < 200 { 10 } else { 99 };
        Edit {
            id: EditId::from(id),
            asserted_at: at(when),
            reason: reason(),
            sessions: BTreeMap::from([(
                session_id(),
                SparseGymSession::exercise(
                    exercise_id(exercise),
                    SparseExercise::set(set_id(set), fields),
                ),
            )]),
            found: BTreeMap::new(),
        }
    }

    /// An edit of what exercises were, each in a session of its own, found
    /// holding `sets`.
    fn exercise_edit(
        id: i64,
        when: &str,
        becomes: RepsExercise,
        exercises: &[(i64, &[i64])],
    ) -> Edit {
        let mut sessions = BTreeMap::new();
        let mut found = BTreeMap::new();
        for (session, (exercise, sets)) in (1..).zip(exercises) {
            sessions.insert(
                CanonicalSessionId::try_from(session).expect("an id"),
                SparseGymSession::exercise(
                    exercise_id(*exercise),
                    SparseExercise::was(Exercise::Reps(becomes)),
                ),
            );
            found.insert(
                exercise_id(*exercise),
                Found {
                    read: Exercise::Reps(RepsExercise::DeadliftBarbell),
                    sets: sets.iter().map(|set| set_id(*set)).collect(),
                },
            );
        }
        Edit {
            id: EditId::from(id),
            asserted_at: at(when),
            reason: reason(),
            sessions,
            found,
        }
    }

    /// **A set a sheet never rated is given an effort**, and the effort says
    /// it came from the edit; the rest of the set is the sheet's still.
    #[test]
    fn an_effort_reaches_a_set_whatever_source_recorded_it() {
        let rated = SparseSet::new(None, None, Some(Rir::Two)).expect("an effort");
        let edits = Edits::of(&[set_edit(1, "2026-11-22T09:00:00Z", 101, rated)]);

        let edited = edits.apply(&session(), &ids());
        let CanonicalExercise::ForReps { sets, .. } = edited.exercises().next().expect("one")
        else {
            panic!("a reps exercise");
        };
        let second = sets.iter().nth(1).expect("a second set");
        assert_eq!(
            second
                .intensity
                .map(|effort| (effort.copied(), effort.is_edited())),
            Some((Rir::Two, true))
        );
        assert_eq!(
            second.outcome.normalised(),
            Some(NormalisedSessionId::FIRST)
        );
        assert_eq!(sets.first().intensity, None);
    }

    /// **A hold is given a length and a load** (#901): 25 Sep's 40 s dead
    /// hang becomes 45 s with 10 kg, each attributed to the edit, while a
    /// count of reps stated of it reaches nothing.
    #[test]
    fn a_timed_set_takes_a_length_and_a_load() {
        let sheet = NormalisedSessionId::FIRST;
        let hang = CanonicalGymSession::new(
            crate::canonical::Attributed::new(
                Occurred::On("2026-09-25".parse().expect("a day")),
                crate::canonical::NormalisedSessionId::FIRST,
            ),
            NonEmpty::of(
                CanonicalItem::exercise(CanonicalExercise::ForDuration {
                    exercise: Attributed::new(DurationExercise::DeadHang, sheet),
                    sets: NonEmpty::of(
                        CanonicalSet {
                            outcome: Attributed::new(
                                Performed::Completed(Some(Duration::from_seconds(40))),
                                sheet,
                            ),
                            load: Some(Attributed::new(Load::UNLOADED, sheet)),
                            began: None,
                            intensity: None,
                            kind: None,
                            rest_after: None,
                            sources: BTreeSet::new(),
                        },
                        vec![],
                    ),
                }),
                vec![],
            ),
            None,
            None,
            Vec::new(),
        );
        let ids = SessionIds {
            session: session_id(),
            exercises: vec![ExerciseIds {
                exercise: exercise_id(10),
                sets: vec![set_id(100)],
            }],
        };
        let length = |stated| {
            let edited =
                Edits::of(&[set_edit(1, "2026-11-23T09:00:00Z", 100, stated)]).apply(&hang, &ids);
            let CanonicalExercise::ForDuration { sets, .. } =
                edited.exercises().next().expect("one")
            else {
                panic!("a timed exercise");
            };
            let set = sets.first().clone();
            (
                set.outcome.value().completed().copied().flatten(),
                set.outcome.is_edited(),
                set.load.map(|load| load.copied()),
            )
        };

        let longer = SparseSet::new(
            Some(Measure::Duration(Duration::from_seconds(45))),
            Some(Load::absolute(Kg::from_grams(10_000))),
            None,
        )
        .expect("two fields");
        assert_eq!(
            length(longer),
            (
                Some(Duration::from_seconds(45)),
                true,
                Some(Load::absolute(Kg::from_grams(10_000)))
            )
        );
        assert_eq!(longer.to_string(), "45 s, 10 kg");

        let counted = SparseSet::new(Some(Measure::Reps(reps(3))), None, None).expect("a count");
        assert_eq!(
            length(counted),
            (
                Some(Duration::from_seconds(40)),
                false,
                Some(Load::UNLOADED)
            )
        );
    }

    /// **An exercise edit reaches every set it holds**, and names the
    /// exercise as the edit's.
    #[test]
    fn an_exercise_edit_renames_the_exercise() {
        let edits = Edits::of(&[exercise_edit(
            1,
            "2026-11-22T09:00:00Z",
            RepsExercise::RomanianDeadliftBarbell,
            &[(10, &[100, 101])],
        )]);

        let edited = edits.apply(&session(), &ids());
        let exercise = edited.exercises().next().expect("one");
        assert_eq!(
            exercise.exercise_key(),
            Some(RepsExercise::RomanianDeadliftBarbell.as_str())
        );
        assert_eq!(exercise.identified_by(), None);
        assert!(edits.conflicts(&held(), &BTreeMap::new()).is_empty());
    }

    /// **A session lists what still says something about it, newest first**:
    /// an edit of a set elsewhere is not listed, and an earlier edit of a
    /// set's effort that a later one restated is not either, while one whose
    /// count still stands is.
    #[test]
    fn a_session_lists_the_edits_still_saying_something_about_it() {
        let rated = |rir| SparseSet::new(None, None, Some(rir)).expect("an effort");
        let both =
            SparseSet::new(Some(Measure::Reps(reps(3))), None, Some(Rir::Two)).expect("two fields");
        let edits = [
            set_edit(1, "2026-11-21T09:00:00Z", 100, rated(Rir::One)),
            set_edit(2, "2026-11-21T09:30:00Z", 101, both),
            set_edit(3, "2026-11-21T10:00:00Z", 100, rated(Rir::Two)),
            set_edit(4, "2026-11-21T10:30:00Z", 101, rated(Rir::Three)),
            set_edit(5, "2026-11-21T11:00:00Z", 999, rated(Rir::Two)),
        ];

        let listed: Vec<EditId> = in_force(&edits, &ids())
            .iter()
            .map(|edit| edit.id)
            .collect();
        assert_eq!(listed, [4, 3, 2].map(EditId::from));
    }

    /// **An exercise edit stands until a later one restates the exercise**,
    /// and an edit of the similar ones is listed on each session it reaches.
    #[test]
    fn an_exercise_edit_is_listed_until_a_later_one_restates_it() {
        let renamed =
            |id, when, becomes| exercise_edit(id, when, becomes, &[(10, &[100, 101]), (20, &[])]);
        let first = renamed(
            1,
            "2026-11-21T09:00:00Z",
            RepsExercise::RomanianDeadliftBarbell,
        );
        let second = renamed(2, "2026-11-21T10:00:00Z", RepsExercise::DeadliftBarbell);

        let listed = |edits: &[Edit]| -> Vec<EditId> {
            in_force(edits, &ids()).iter().map(|edit| edit.id).collect()
        };
        assert_eq!(listed(std::slice::from_ref(&first)), [EditId::from(1)]);
        assert_eq!(listed(&[second, first]), [EditId::from(2)]);
    }

    fn held() -> BTreeMap<CanonicalExerciseId, BTreeSet<CanonicalSetId>> {
        BTreeMap::from([(exercise_id(10), BTreeSet::from([set_id(100), set_id(101)]))])
    }

    /// **A later edit of a set keeps what an earlier one changed and it did
    /// not**, whichever order the store hands them back in.
    #[test]
    fn a_later_edit_of_a_set_keeps_the_fields_it_left() {
        let rated = SparseSet::new(None, None, Some(Rir::One)).expect("an effort");
        let recounted = SparseSet::new(Some(Measure::Reps(reps(2))), None, None).expect("a count");
        let earlier = set_edit(1, "2026-11-21T09:00:00Z", 100, rated);
        let later = set_edit(2, "2026-11-21T10:00:00Z", 100, recounted);

        for edits in [
            Edits::of(&[earlier.clone(), later.clone()]),
            Edits::of(&[later, earlier]),
        ] {
            let edit = edits.set(set_id(100)).expect("an edit");
            assert_eq!(edit.intensity(), Some(Rir::One));
            assert_eq!(edit.measure(), Some(Measure::Reps(reps(2))));
        }
        assert_eq!(SparseSet::new(None, None, None), None);
    }

    /// **What a rebuild changed under an edit is raised**: a set added to an
    /// edited exercise, and an edited set no exercise holds.
    #[test]
    fn a_rebuild_that_moves_what_an_edit_stands_on_is_raised() {
        let edits = Edits::of(&[
            exercise_edit(
                1,
                "2026-11-22T09:00:00Z",
                RepsExercise::RomanianDeadliftBarbell,
                &[(10, &[100])],
            ),
            set_edit(
                2,
                "2026-11-22T09:01:00Z",
                999,
                SparseSet::new(Some(Measure::Reps(reps(6))), None, None).expect("a count"),
            ),
        ]);

        assert_eq!(
            edits.conflicts(&held(), &BTreeMap::new()),
            vec![
                Conflict::Added {
                    exercise: exercise_id(10),
                    set: set_id(101)
                },
                Conflict::SetGone(set_id(999)),
            ]
        );
    }

    fn instant(at: &str) -> Timestamp {
        at.parse().expect("an instant")
    }

    fn london(at: &str) -> Occurred {
        Occurred::At(crate::normalised::StartedAt::new(
            instant(at),
            crate::normalised::OperatorZone::try_from("Europe/London".to_owned()).expect("a zone"),
        ))
    }

    fn stream(name: &str) -> LandingStream {
        name.parse().expect("a stream")
    }

    fn normalised(id: i64) -> NormalisedSessionId {
        NormalisedSessionId::try_from(id).expect("an id")
    }

    /// 2025-03-31 as Hevy and the watch told it: Hevy from 18:00:53 to
    /// 19:00:54, the watch from 18:47:33 to 19:00:01, with a heart rate the
    /// watch read every ten minutes from its own start, and one set it timed
    /// at 18:50.
    fn bracketed() -> CanonicalGymSession {
        let (hevy, watch) = (normalised(1), normalised(2));
        let readings =
            [(0, 90), (600, 120), (720, 80)].map(|(at, beats)| crate::measure::HeartRateSample {
                at: Duration::from_seconds(at),
                beats_per_minute: crate::measure::BeatsPerMinute::new(beats).expect("a rate"),
            });
        let heart_rate = crate::measure::MeasuredHeartRate::new(
            crate::measure::HeartRateSummary::new(
                crate::measure::BeatsPerMinute::new(100).expect("a rate"),
                crate::measure::BeatsPerMinute::new(120).expect("a rate"),
            ),
            Some(crate::measure::HeartRateSeries::new(NonEmpty::of(
                readings[0],
                readings[1..].to_vec(),
            ))),
        );
        let mut timed = set(5, 60);
        timed.began = Some(Attributed::new(
            crate::normalised::StartedAt::new(
                instant("2025-03-31T17:50:00Z"),
                crate::normalised::OperatorZone::try_from("Europe/London".to_owned())
                    .expect("a zone"),
            ),
            watch,
        ));
        CanonicalGymSession::new(
            Attributed::new(london("2025-03-31T17:00:53Z"), hevy),
            NonEmpty::of(
                CanonicalItem::exercise(CanonicalExercise::ForReps {
                    identified: Attributed::new(
                        Identified::Recorded(RepsExercise::DeadliftBarbell),
                        hevy,
                    ),
                    sets: NonEmpty::of(timed, vec![]),
                }),
                vec![],
            ),
            Some(Attributed::new(heart_rate, watch)),
            Some(Attributed::new(seconds(3601), hevy)),
            vec![
                crate::gym::Account {
                    stream: stream("hevy.workouts"),
                    normalised: hevy,
                    occurred: london("2025-03-31T17:00:53Z"),
                    duration: Some(seconds(3601)),
                },
                crate::gym::Account {
                    stream: stream("garmin.activities"),
                    normalised: watch,
                    occurred: london("2025-03-31T17:47:33Z"),
                    duration: Some(seconds(748)),
                },
            ],
        )
    }

    fn seconds(count: u64) -> PositiveDuration {
        PositiveDuration::from_seconds(count).expect("a length")
    }

    fn bounds_edit(
        started: Option<Taken<Occurred>>,
        lasted: Option<Taken<PositiveDuration>>,
    ) -> Edit {
        sparse_edit(1, SparseGymSession::bounds(started, lasted))
    }

    fn sparse_edit(id: i64, sparse: SparseGymSession) -> Edit {
        Edit {
            id: EditId::from(id),
            asserted_at: at("2026-11-24T09:00:00Z")
                .checked_add(jiff::SignedDuration::from_secs(id))
                .expect("an instant"),
            reason: reason(),
            sessions: BTreeMap::from([(session_id(), sparse)]),
            found: BTreeMap::new(),
        }
    }

    /// **A start taken from the watch reads as the watch's**, attributed to
    /// its account, and the end stays Hevy's: each bound is its own field.
    #[test]
    fn a_start_is_taken_from_the_source_chosen() {
        let edited = Edits::of(&[bounds_edit(
            Some(Taken::From(stream("garmin.activities"))),
            None,
        )])
        .apply(&bracketed(), &ids());
        assert_eq!(edited.occurred(), &london("2025-03-31T17:47:33Z"));
        assert_eq!(edited.started().normalised(), Some(normalised(2)));
        assert_eq!(
            edited.ended().and_then(|ended| ended.normalised()),
            Some(normalised(1))
        );
        assert_eq!(
            edited
                .length()
                .map(crate::measure::PositiveDuration::as_seconds),
            Some(801)
        );
        assert_eq!(
            edited
                .heart_rate()
                .map(|heart_rate| heart_rate.value().clone()),
            bracketed()
                .heart_rate()
                .map(|heart_rate| heart_rate.value().clone()),
            "the watch's own start and an end past its own leave its figures standing"
        );
    }

    /// **An end the operator states is the edit's**, and the heart rate is
    /// read only up to it: the reading at twelve minutes is after it, so the
    /// summary is read again from the two before.
    #[test]
    fn a_stated_end_cuts_the_heart_rate_at_it() {
        let sparse = moved(
            &bracketed(),
            Some(Taken::From(stream("garmin.activities"))),
            Some(Taken::Stated(instant("2025-03-31T17:58:00Z"))),
        )
        .expect("bounds it can take");
        assert_eq!(
            sparse.lasted,
            Some(Taken::Stated(seconds(627))),
            "the end is held as the length from the watch's start"
        );
        let edited = Edits::of(&[sparse_edit(1, sparse)]).apply(&bracketed(), &ids());
        assert!(edited.ended().is_some_and(|ended| ended.is_edited()));
        assert_eq!(
            edited.ended().map(|ended| ended.copied()),
            Some(instant("2025-03-31T17:58:00Z"))
        );
        let heart_rate = edited.heart_rate().expect("a heart rate").value();
        assert_eq!(
            heart_rate.series().map(|series| series.samples().count()),
            Some(2)
        );
        assert_eq!(heart_rate.stated().highest().as_u32(), 120);
        assert_eq!(
            heart_rate.stated().average().as_u32(),
            90,
            "90 held for ten minutes and 120 for the one second the last reading counts"
        );
    }

    /// **Bounds that cannot be one session's are refused**: an end before
    /// the start, and a start after a set the watch timed.
    #[test]
    fn impossible_bounds_are_refused() {
        let session = bracketed();
        let start = |at| Attributed::new(london(at), normalised(1));
        assert_eq!(
            moved(
                &session,
                None,
                Some(Taken::Stated(instant("2025-03-31T16:00:00Z")))
            ),
            Err(Unmoved::Invalid(
                crate::gym::InvalidBounds::EndsBeforeItStarts
            ))
        );
        assert_eq!(
            moved(
                &session,
                None,
                Some(Taken::Stated(instant("2025-03-31T17:00:53Z")))
            ),
            Err(Unmoved::Invalid(
                crate::gym::InvalidBounds::EndsBeforeItStarts
            )),
            "an end at the start is no session"
        );
        assert_eq!(
            session.bounded(start("2025-03-31T17:55:00Z"), session.lasted().copied()),
            Err(crate::gym::InvalidBounds::LeavesOutASet)
        );
        assert_eq!(
            session.bounded(start("2025-04-01T17:00:00Z"), None),
            Err(crate::gym::InvalidBounds::MovesDay)
        );
    }

    /// **A start moved alone keeps the end**: one derived stays where the
    /// merge put it, and one he typed before is stated again as the length
    /// from the new start, so the late press moves nothing else.
    #[test]
    fn a_start_moved_alone_keeps_the_end() {
        let session = bracketed();
        let later = Edits::of(&[bounds_edit(
            Some(Taken::Stated(london("2025-03-31T17:30:00Z"))),
            None,
        )])
        .apply(&session, &ids());
        assert_eq!(
            later.ended().map(|ended| ended.copied()),
            Some(instant("2025-03-31T18:00:54Z"))
        );
        assert_eq!(later.length(), Some(seconds(1854)));

        let ended = moved(
            &session,
            None,
            Some(Taken::Stated(instant("2025-03-31T17:58:00Z"))),
        )
        .expect("an end it can take");
        let first = sparse_edit(1, ended);
        let typed_end = Edits::of(std::slice::from_ref(&first)).apply(&session, &ids());
        let started = moved(
            &typed_end,
            Some(Taken::Stated(london("2025-03-31T17:30:00Z"))),
            None,
        )
        .expect("a start it can take");
        assert_eq!(started.lasted, Some(Taken::Stated(seconds(1680))));
        let both = Edits::of(&[first, sparse_edit(2, started)]).apply(&session, &ids());
        assert_eq!(both.occurred(), &london("2025-03-31T17:30:00Z"));
        assert_eq!(
            both.ended().map(|ended| ended.copied()),
            Some(instant("2025-03-31T17:58:00Z"))
        );
    }

    /// **Nothing changed is not an edit.**
    #[test]
    fn bounds_as_they_read_are_no_change() {
        let session = bracketed();
        assert_eq!(
            moved(
                &session,
                Some(Taken::From(stream("hevy.workouts"))),
                Some(Taken::From(stream("hevy.workouts")))
            ),
            Err(Unmoved::Unchanged)
        );
    }

    /// **A source that no longer accounts for the session is raised**, and
    /// the session reads as derived meanwhile; one the layer no longer holds
    /// is raised as gone.
    #[test]
    fn a_source_that_is_gone_is_raised() {
        let edits = Edits::of(&[bounds_edit(Some(Taken::From(stream("btwb.exports"))), None)]);
        let session = bracketed();
        assert_eq!(edits.apply(&session, &ids()).occurred(), session.occurred());
        let held = BTreeMap::from([(
            session_id(),
            BTreeSet::from([stream("hevy.workouts"), stream("garmin.activities")]),
        )]);
        assert_eq!(
            edits.conflicts(&BTreeMap::new(), &held),
            vec![Conflict::SourceGone(session_id())]
        );
        assert_eq!(
            edits.conflicts(&BTreeMap::new(), &BTreeMap::new()),
            vec![Conflict::SessionGone(session_id())]
        );
    }

    /// **An edit of when a session started is listed on it** until a later
    /// one restates the start.
    #[test]
    fn an_edit_of_the_start_is_listed_until_restated() {
        let first = bounds_edit(Some(Taken::From(stream("garmin.activities"))), None);
        let mut second = bounds_edit(Some(Taken::Stated(london("2025-03-31T17:30:00Z"))), None);
        second.id = EditId::from(2);
        second.asserted_at = at("2026-11-24T10:00:00Z");
        let listed = |edits: &[Edit]| -> Vec<EditId> {
            in_force(edits, &ids()).iter().map(|edit| edit.id).collect()
        };
        assert_eq!(listed(std::slice::from_ref(&first)), [EditId::from(1)]);
        assert_eq!(listed(&[first, second]), [EditId::from(2)]);
    }

    /// The axis a stated mass is read on is the set's: added to the body on a
    /// pull-up, the load on a squat, and assistance taken off either.
    #[test]
    fn a_stated_load_is_read_on_the_set_s_axis() {
        let twenty = Kg::from_grams(20_000);
        let pull_up = Some(Load::BODYWEIGHT);
        let squat = Some(Load::absolute(Kg::from_grams(100_000)));

        assert_eq!(
            StatedLoad::Mass(twenty).on(pull_up),
            Load::relative(SignedKg::from_grams(20_000))
        );
        assert_eq!(StatedLoad::Mass(twenty).on(squat), Load::absolute(twenty));
        assert_eq!(
            StatedLoad::Assistance(twenty).on(squat),
            Load::relative(SignedKg::from_grams(-20_000))
        );
        assert_eq!(StatedLoad::Nothing.on(pull_up), Load::BODYWEIGHT);
        assert_eq!(StatedLoad::Nothing.on(None), Load::UNLOADED);
        assert_eq!(
            StatedLoad::of(Load::relative(SignedKg::from_grams(-21_000))),
            StatedLoad::Assistance(Kg::from_grams(21_000))
        );
    }
}
