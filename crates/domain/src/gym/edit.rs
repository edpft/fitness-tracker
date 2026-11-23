//! The operator's edits to the canonical gym layer (§ II.2, #893).
//!
//! **An edit is applied on top of the canonical layer**, and anchored to a
//! canonical exercise or set by the id it keeps across every rebuild
//! ([`super::identity`]). The operator, 2026-11-21: *"I think edits should be
//! applied on top of the canonical layer, that's why I keep saying they are
//! sparse canonical gym sessions that can edit any field in a canonical gym
//! session."* Anchoring one to a source instead meant giving sources fields
//! they never had, such as an effort on a watch's set; on the canonical set it
//! is one more field, whichever source the rest came from.
//!
//! **Sparse.** An edit holds only what it changes, and the rest of the entity
//! stands as derived. A field an edit states is attributed to it
//! ([`crate::canonical::TakenFrom::Edit`]) rather than to a normalised session it never came
//! from.
//!
//! **Two kinds so far.** An exercise edit says what an exercise was, and
//! reaches every set it holds; a set edit says what one set's reps, load or
//! effort were. "Similar" is how one assertion reaches many exercises: one
//! exercise edit per exercise found reading as the same thing, under one
//! [`EditId`] so it is retracted as it was made.
//!
//! **Where a rebuild changes what an edit stands on, the edit stands and the
//! conflict is raised** ([`Conflict`]): a set a source adds to an edited
//! exercise, or an edited entity no rebuild holds any more. The operator chose
//! that over locking or clearing: *"it would be best to deal with these cases
//! on a case by case basis."*

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use jiff::Timestamp;

use crate::canonical::Attributed;
use crate::landing::InvalidIdentifier;
use crate::measure::{Kg, RepCount};
use crate::newtype::string_name;
use crate::sequence::NonEmpty;

use super::canonical::{CanonicalExercise, CanonicalGymSession, CanonicalSet, Identified};
use super::exercise::Exercise;
use super::identity::{CanonicalExerciseId, CanonicalSetId, ExerciseIds, SessionIds};
use super::intensity::Rir;
use super::load::{Load, SignedKg};
use super::outcome::Performed;

/// What the store calls one assertion.
///
/// A surrogate, and allowed to be: it identifies the *assertion* so the
/// operator can retract it as he made it, and nothing is anchored by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EditId(i64);

impl From<i64> for EditId {
    fn from(id: i64) -> Self {
        Self(id)
    }
}

impl EditId {
    pub const fn as_i64(self) -> i64 {
        self.0
    }
}

impl fmt::Display for EditId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Why the operator made an edit.
///
/// Required, because § II.2 says an unexplained override is unreadable six
/// months later. Non-empty is the whole of the validation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EditReason(String);

impl TryFrom<String> for EditReason {
    type Error = InvalidIdentifier;

    fn try_from(reason: String) -> Result<Self, Self::Error> {
        if reason.trim().is_empty() {
            return Err(InvalidIdentifier::Empty {
                field: "an edit's reason",
            });
        }
        Ok(Self(reason))
    }
}

string_name!(EditReason, InvalidIdentifier);

/// What one canonical exercise was.
///
/// **With what it read as when edited, and the sets it held.** The first says
/// what was changed; the second is how a set a source adds later is told
/// apart from the sets the operator ruled on, so it is raised rather than
/// silently taken in (§ II.2, *"overrides do not propagate"*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExerciseEdit {
    pub exercise: CanonicalExerciseId,
    pub becomes: Exercise,
    pub read: Exercise,
    pub sets: BTreeSet<CanonicalSetId>,
}

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

/// One field of a set an edit can state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SetField {
    Reps,
    Load,
    Effort,
}

/// What the operator changed about one set, and only that (#892).
///
/// **Never empty**: an edit that changes nothing is not an edit, so
/// [`SetEdit::new`] refuses one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetEdit {
    reps: Option<RepCount>,
    load: Option<Load>,
    effort: Option<Rir>,
}

impl SetEdit {
    /// The fields changed, or [`None`] where none is.
    #[must_use]
    pub const fn new(
        reps: Option<RepCount>,
        load: Option<Load>,
        effort: Option<Rir>,
    ) -> Option<Self> {
        if reps.is_none() && load.is_none() && effort.is_none() {
            return None;
        }
        Some(Self { reps, load, effort })
    }

    pub const fn reps(self) -> Option<RepCount> {
        self.reps
    }

    pub const fn load(self) -> Option<Load> {
        self.load
    }

    pub const fn effort(self) -> Option<Rir> {
        self.effort
    }

    /// The fields it states.
    fn stated(self) -> impl Iterator<Item = SetField> {
        [
            self.reps.map(|_| SetField::Reps),
            self.load.map(|_| SetField::Load),
            self.effort.map(|_| SetField::Effort),
        ]
        .into_iter()
        .flatten()
    }

    /// This edit with a later one's fields over it: what the later one
    /// changed, it changes; what it left, this one's still says.
    #[must_use]
    pub const fn then(self, later: Self) -> Self {
        Self {
            reps: match later.reps {
                Some(reps) => Some(reps),
                None => self.reps,
            },
            load: match later.load {
                Some(load) => Some(load),
                None => self.load,
            },
            effort: match later.effort {
                Some(effort) => Some(effort),
                None => self.effort,
            },
        }
    }

    /// `set` with each field this edit states over it, attributed to the
    /// edit. Reps reach only a set counted in reps, which `reps` says how to
    /// hold.
    fn over<M: Clone>(
        self,
        set: &CanonicalSet<M>,
        reps: impl Fn(RepCount) -> Option<M>,
    ) -> CanonicalSet<M> {
        let mut edited = set.clone();
        if let Some(count) = self.reps.and_then(reps) {
            edited.outcome = Attributed::edited(Performed::Completed(Some(count)));
        }
        if let Some(load) = self.load {
            edited.load = Some(Attributed::edited(load));
        }
        if let Some(effort) = self.effort {
            edited.intensity = Some(Attributed::edited(effort));
        }
        edited
    }
}

impl fmt::Display for SetEdit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if let Some(reps) = self.reps {
            parts.push(format!("{reps} reps"));
        }
        if let Some(load) = self.load {
            parts.push(load.to_string());
        }
        if let Some(effort) = self.effort {
            parts.push(format!("{effort} in reserve"));
        }
        f.write_str(&parts.join(", "))
    }
}

/// What one assertion edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edited {
    /// What exercises were: one, or it and the similar ones.
    Exercises(NonEmpty<ExerciseEdit>),
    /// What one set was.
    Set { set: CanonicalSetId, edit: SetEdit },
}

impl Edited {
    /// How many exercises or sets it reaches.
    #[must_use]
    pub const fn reaches(&self) -> usize {
        match self {
            Self::Exercises(exercises) => exercises.count(),
            Self::Set { .. } => 1,
        }
    }
}

/// One assertion by the operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub id: EditId,
    pub asserted_at: Timestamp,
    pub reason: EditReason,
    pub edited: Edited,
}

/// Which of `edits` are in force on a session whose exercises hold `ids`,
/// newest first (#895).
///
/// **In force is what still says something about the session**: the last
/// edit of each of its exercises, and of each field of each of its sets. An
/// edit a later one has wholly overruled is not listed, since undoing it would
/// change nothing; undoing the later one brings it back.
#[must_use]
pub fn in_force<'a>(edits: &'a [Edit], ids: &[ExerciseIds]) -> Vec<&'a Edit> {
    let mut ordered: Vec<&Edit> = edits.iter().collect();
    ordered.sort_by_key(|edit| (edit.asserted_at, edit.id));
    let exercises: BTreeSet<CanonicalExerciseId> = ids.iter().map(|held| held.exercise).collect();
    let sets: BTreeSet<CanonicalSetId> = ids
        .iter()
        .flat_map(|held| held.sets.iter().copied())
        .collect();
    let mut exercise_by: BTreeMap<CanonicalExerciseId, EditId> = BTreeMap::new();
    let mut field_by: BTreeMap<(CanonicalSetId, SetField), EditId> = BTreeMap::new();
    for edit in &ordered {
        match &edit.edited {
            Edited::Exercises(edited) => {
                for exercise in edited {
                    if exercises.contains(&exercise.exercise) {
                        exercise_by.insert(exercise.exercise, edit.id);
                    }
                }
            }
            Edited::Set { set, edit: fields } if sets.contains(set) => {
                for field in fields.stated() {
                    field_by.insert((*set, field), edit.id);
                }
            }
            Edited::Set { .. } => {}
        }
    }
    let standing: BTreeSet<EditId> = exercise_by
        .into_values()
        .chain(field_by.into_values())
        .collect();
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
}

/// Every edit in force, ready for a rebuild to apply.
///
/// **Later wins where two edits name one entity**: the operator changing his
/// mind. A set's edits merge field by field, since each holds only what it
/// changed. Ordering is by when they were asserted, so the result does not
/// depend on the order the store hands them back in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Edits {
    exercises: BTreeMap<CanonicalExerciseId, ExerciseEdit>,
    sets: BTreeMap<CanonicalSetId, SetEdit>,
}

impl Edits {
    #[must_use]
    pub fn of(edits: &[Edit]) -> Self {
        let mut ordered: Vec<&Edit> = edits.iter().collect();
        ordered.sort_by_key(|edit| (edit.asserted_at, edit.id));
        let mut exercises = BTreeMap::new();
        let mut sets: BTreeMap<CanonicalSetId, SetEdit> = BTreeMap::new();
        for edit in ordered {
            match &edit.edited {
                Edited::Exercises(edited) => {
                    for exercise in edited {
                        exercises.insert(exercise.exercise, exercise.clone());
                    }
                }
                Edited::Set { set, edit } => {
                    let merged = sets.get(set).map_or(*edit, |held| held.then(*edit));
                    sets.insert(*set, merged);
                }
            }
        }
        Self { exercises, sets }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.exercises.is_empty() && self.sets.is_empty()
    }

    /// The edit of one exercise, where there is one.
    #[must_use]
    pub fn exercise(&self, exercise: CanonicalExerciseId) -> Option<&ExerciseEdit> {
        self.exercises.get(&exercise)
    }

    /// The edit of one set, where there is one.
    #[must_use]
    pub fn set(&self, set: CanonicalSetId) -> Option<SetEdit> {
        self.sets.get(&set).copied()
    }

    /// Whether any edit reaches a session written under `ids`.
    #[must_use]
    pub fn reach(&self, ids: &SessionIds) -> bool {
        ids.exercises.iter().any(|exercise| {
            self.exercises.contains_key(&exercise.exercise)
                || exercise.sets.iter().any(|set| self.sets.contains_key(set))
        })
    }

    /// `session`, written under `ids`, as the edits say it was.
    #[must_use]
    pub fn apply(&self, session: &CanonicalGymSession, ids: &SessionIds) -> CanonicalGymSession {
        let mut ids = ids.exercises.iter();
        session.map_exercises(|exercise| {
            ids.next().map_or_else(
                || exercise.clone(),
                |held| self.exercise_over(exercise, held),
            )
        })
    }

    fn exercise_over(&self, exercise: &CanonicalExercise, ids: &ExerciseIds) -> CanonicalExercise {
        let becomes = self.exercises.get(&ids.exercise).map(|edit| edit.becomes);
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
                    set(index).map_or_else(|| held.clone(), |edit| edit.over(held, Some))
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
                    set(index).map_or_else(|| held.clone(), |edit| edit.over(held, |_| None))
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
                    set(index).map_or_else(|| held.clone(), |edit| edit.over(held, |_| None))
                }),
            },
        }
    }

    /// What the layer holding `held` — each exercise's id and its sets' —
    /// leaves for the operator to settle.
    #[must_use]
    pub fn conflicts(
        &self,
        held: &BTreeMap<CanonicalExerciseId, BTreeSet<CanonicalSetId>>,
    ) -> Vec<Conflict> {
        let mut conflicts = Vec::new();
        for (exercise, edit) in &self.exercises {
            match held.get(exercise) {
                None => conflicts.push(Conflict::ExerciseGone(*exercise)),
                Some(sets) => {
                    conflicts.extend(sets.difference(&edit.sets).map(|set| Conflict::Added {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::{NormalisedSessionId, Occurred};
    use crate::gym::canonical::CanonicalItem;
    use crate::gym::exercise::RepsExercise;

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
            Occurred::On("2020-10-09".parse().expect("a day")),
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
        )
    }

    fn ids() -> SessionIds {
        SessionIds {
            session: crate::gym::identity::CanonicalSessionId::try_from(1).expect("an id"),
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

    fn set_edit(id: i64, when: &str, set: i64, edit: SetEdit) -> Edit {
        Edit {
            id: EditId::from(id),
            asserted_at: at(when),
            reason: reason(),
            edited: Edited::Set {
                set: set_id(set),
                edit,
            },
        }
    }

    /// **A set a sheet never rated is given an effort**, and the effort says
    /// it came from the edit; the rest of the set is the sheet's still.
    #[test]
    fn an_effort_reaches_a_set_whatever_source_recorded_it() {
        let rated = SetEdit::new(None, None, Some(Rir::Two)).expect("an effort");
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

    /// **An exercise edit reaches every set it holds**, and names the
    /// exercise as the edit's.
    #[test]
    fn an_exercise_edit_renames_the_exercise() {
        let edits = Edits::of(&[Edit {
            id: EditId::from(1),
            asserted_at: at("2026-11-22T09:00:00Z"),
            reason: reason(),
            edited: Edited::Exercises(NonEmpty::of(
                ExerciseEdit {
                    exercise: exercise_id(10),
                    becomes: Exercise::Reps(RepsExercise::RomanianDeadliftBarbell),
                    read: Exercise::Reps(RepsExercise::DeadliftBarbell),
                    sets: BTreeSet::from([set_id(100), set_id(101)]),
                },
                vec![],
            )),
        }]);

        let edited = edits.apply(&session(), &ids());
        let exercise = edited.exercises().next().expect("one");
        assert_eq!(
            exercise.exercise_key(),
            Some(RepsExercise::RomanianDeadliftBarbell.as_str())
        );
        assert_eq!(exercise.identified_by(), None);
        assert!(edits.conflicts(&held()).is_empty());
    }

    /// **A session lists what still says something about it, newest first**:
    /// an edit of a set elsewhere is not listed, and an earlier edit of a
    /// set's effort that a later one restated is not either, while one whose
    /// count still stands is.
    #[test]
    fn a_session_lists_the_edits_still_saying_something_about_it() {
        let rated = |rir| SetEdit::new(None, None, Some(rir)).expect("an effort");
        let both = SetEdit::new(Some(reps(3)), None, Some(Rir::Two)).expect("two fields");
        let edits = [
            set_edit(1, "2026-11-21T09:00:00Z", 100, rated(Rir::One)),
            set_edit(2, "2026-11-21T09:30:00Z", 101, both),
            set_edit(3, "2026-11-21T10:00:00Z", 100, rated(Rir::Two)),
            set_edit(4, "2026-11-21T10:30:00Z", 101, rated(Rir::Three)),
            set_edit(5, "2026-11-21T11:00:00Z", 999, rated(Rir::Two)),
        ];

        let listed: Vec<EditId> = in_force(&edits, &ids().exercises)
            .iter()
            .map(|edit| edit.id)
            .collect();
        assert_eq!(listed, [4, 3, 2].map(EditId::from));
    }

    /// **An exercise edit stands until a later one restates the exercise**,
    /// and an edit of the similar ones is listed on each session it reaches.
    #[test]
    fn an_exercise_edit_is_listed_until_a_later_one_restates_it() {
        let renamed = |id, when, becomes| Edit {
            id: EditId::from(id),
            asserted_at: at(when),
            reason: reason(),
            edited: Edited::Exercises(NonEmpty::of(
                ExerciseEdit {
                    exercise: exercise_id(10),
                    becomes: Exercise::Reps(becomes),
                    read: Exercise::Reps(RepsExercise::DeadliftBarbell),
                    sets: BTreeSet::from([set_id(100), set_id(101)]),
                },
                vec![ExerciseEdit {
                    exercise: exercise_id(20),
                    becomes: Exercise::Reps(becomes),
                    read: Exercise::Reps(RepsExercise::DeadliftBarbell),
                    sets: BTreeSet::new(),
                }],
            )),
        };
        let first = renamed(
            1,
            "2026-11-21T09:00:00Z",
            RepsExercise::RomanianDeadliftBarbell,
        );
        let second = renamed(2, "2026-11-21T10:00:00Z", RepsExercise::DeadliftBarbell);

        let listed = |edits: &[Edit]| -> Vec<EditId> {
            in_force(edits, &ids().exercises)
                .iter()
                .map(|edit| edit.id)
                .collect()
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
        let rated = SetEdit::new(None, None, Some(Rir::One)).expect("an effort");
        let recounted = SetEdit::new(Some(reps(2)), None, None).expect("a count");
        let earlier = set_edit(1, "2026-11-21T09:00:00Z", 100, rated);
        let later = set_edit(2, "2026-11-21T10:00:00Z", 100, recounted);

        for edits in [
            Edits::of(&[earlier.clone(), later.clone()]),
            Edits::of(&[later, earlier]),
        ] {
            let edit = edits.set(set_id(100)).expect("an edit");
            assert_eq!(edit.effort(), Some(Rir::One));
            assert_eq!(edit.reps(), Some(reps(2)));
        }
        assert_eq!(SetEdit::new(None, None, None), None);
    }

    /// **What a rebuild changed under an edit is raised**: a set added to an
    /// edited exercise, and an edited set no exercise holds.
    #[test]
    fn a_rebuild_that_moves_what_an_edit_stands_on_is_raised() {
        let edits = Edits::of(&[
            Edit {
                id: EditId::from(1),
                asserted_at: at("2026-11-22T09:00:00Z"),
                reason: reason(),
                edited: Edited::Exercises(NonEmpty::of(
                    ExerciseEdit {
                        exercise: exercise_id(10),
                        becomes: Exercise::Reps(RepsExercise::RomanianDeadliftBarbell),
                        read: Exercise::Reps(RepsExercise::DeadliftBarbell),
                        sets: BTreeSet::from([set_id(100)]),
                    },
                    vec![],
                )),
            },
            set_edit(
                2,
                "2026-11-22T09:01:00Z",
                999,
                SetEdit::new(Some(reps(6)), None, None).expect("a count"),
            ),
        ]);

        assert_eq!(
            edits.conflicts(&held()),
            vec![
                Conflict::Added {
                    exercise: exercise_id(10),
                    set: set_id(101)
                },
                Conflict::SetGone(set_id(999)),
            ]
        );
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
