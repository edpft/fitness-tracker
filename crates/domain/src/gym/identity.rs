//! Which canonical session, exercise and set a rebuilt one is (#893).
//!
//! **A canonical entity keeps its identity once built** (§ II.4, constitution
//! 11.0.0), because an edit is anchored to it. The operator, 2026-11-21:
//! *"Once they've been built, they have been built, they are a thing."*
//!
//! **What a set is, across a rebuild, is the source sets it was built from.**
//! A rebuild reissues every normalised id (#890), so a canonical set cannot be
//! recognised by the normalised rows it names. What survives is each source's
//! own name for a set — a Hevy set's workout and position, a watch set's start
//! stamp, a sheet set's cell — and a rebuilt set built from any of the same
//! source sets is the same set. The operator, asked whether that is "the same
//! set": *"Yes"*. Its position is not part of it: a set that moves from first
//! to second and is otherwise unchanged has not changed.
//!
//! **An exercise and a session follow their sets.** An exercise is the one its
//! sets were in, and a session the one its exercises were in, so neither needs
//! a source name of its own.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::landing::{LandingStream, SourceRecordId};
use crate::normalised::SourceTerm;

use super::canonical::{CanonicalExercise, CanonicalGymSession};

/// One set as its source names it: the stream, the record holding it, and the
/// source's own term for the set within that record.
///
/// What the term is belongs to the adapter that reads the source (§ 8): a
/// position for a log that numbers nothing, a start stamp for a watch, a cell
/// for a sheet.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceSet {
    pub stream: LandingStream,
    pub record: SourceRecordId,
    pub term: SourceTerm,
}

impl fmt::Display for SourceSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.stream, self.record, self.term)
    }
}

/// Why a stored canonical id could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a canonical id must be positive")]
pub struct InvalidCanonicalId;

macro_rules! canonical_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(i64);

        impl $name {
            pub const fn as_i64(self) -> i64 {
                self.0
            }
        }

        impl TryFrom<i64> for $name {
            type Error = InvalidCanonicalId;

            fn try_from(id: i64) -> Result<Self, Self::Error> {
                if id < 1 {
                    return Err(InvalidCanonicalId);
                }
                Ok(Self(id))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

canonical_id!(
    /// A canonical gym session, kept across every rebuild that derives it.
    CanonicalSessionId
);
canonical_id!(
    /// A canonical exercise, kept across every rebuild that derives it.
    CanonicalExerciseId
);
canonical_id!(
    /// A canonical set, kept across every rebuild that derives it.
    CanonicalSetId
);

/// Where one source set was last placed: its session, exercise and set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub session: CanonicalSessionId,
    pub exercise: CanonicalExerciseId,
    pub set: CanonicalSetId,
}

/// An id a rebuilt entity keeps, or the word that it is new and needs one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issued<I> {
    Kept(I),
    New,
}

/// The ids one rebuilt exercise and its sets take, sets in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExerciseIdentity {
    pub exercise: Issued<CanonicalExerciseId>,
    pub sets: Vec<Issued<CanonicalSetId>>,
}

/// The ids one rebuilt session takes, its exercises in the order
/// [`CanonicalGymSession::exercises`] gives them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionIdentity {
    pub session: Issued<CanonicalSessionId>,
    pub exercises: Vec<ExerciseIdentity>,
}

/// Which ids each of `sessions` keeps, given where every source set was last
/// placed.
///
/// **Each id goes to one entity at most.** Where two rebuilt sets were built
/// from source sets of one old set — an old set that has split — the first in
/// order keeps it and the second is new, and the same for exercises and
/// sessions. Among a set's own sources, the first in their order that names an
/// id not yet taken decides.
pub fn identify(
    sessions: &[CanonicalGymSession],
    placed: &BTreeMap<SourceSet, Placed>,
) -> Vec<SessionIdentity> {
    let mut taken = Taken::default();
    sessions
        .iter()
        .map(|session| {
            let exercises: Vec<ExerciseIdentity> = session
                .exercises()
                .map(|exercise| identify_exercise(exercise, placed, &mut taken))
                .collect();
            let held = session
                .exercises()
                .flat_map(CanonicalExercise::set_sources)
                .flatten()
                .filter_map(|source| placed.get(source))
                .map(|placed| placed.session);
            SessionIdentity {
                session: take(held, &mut taken.sessions),
                exercises,
            }
        })
        .collect()
}

#[derive(Default)]
struct Taken {
    sessions: BTreeSet<CanonicalSessionId>,
    exercises: BTreeSet<CanonicalExerciseId>,
    sets: BTreeSet<CanonicalSetId>,
}

fn identify_exercise(
    exercise: &CanonicalExercise,
    placed: &BTreeMap<SourceSet, Placed>,
    taken: &mut Taken,
) -> ExerciseIdentity {
    let sources = exercise.set_sources();
    let sets = sources
        .iter()
        .map(|sources| {
            let held = sources
                .iter()
                .filter_map(|source| placed.get(source))
                .map(|placed| placed.set);
            take(held, &mut taken.sets)
        })
        .collect();
    let held = sources
        .iter()
        .flat_map(|sources| sources.iter())
        .filter_map(|source| placed.get(source))
        .map(|placed| placed.exercise);
    ExerciseIdentity {
        exercise: take(held, &mut taken.exercises),
        sets,
    }
}

/// The first of `held` not yet taken, now taken; or new.
fn take<I: Copy + Ord>(held: impl Iterator<Item = I>, taken: &mut BTreeSet<I>) -> Issued<I> {
    for id in held {
        if taken.insert(id) {
            return Issued::Kept(id);
        }
    }
    Issued::New
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::{Attributed, NormalisedSessionId, Occurred};
    use crate::gym::{
        canonical::{CanonicalItem, CanonicalSet, Identified},
        exercise::RepsExercise,
        outcome::Performed,
    };
    use crate::measure::RepCount;
    use crate::sequence::NonEmpty;

    fn source(term: &str) -> SourceSet {
        SourceSet {
            stream: LandingStream::try_from("hevy.workouts").expect("a stream"),
            record: SourceRecordId::try_from("a-workout".to_owned()).expect("a record"),
            term: SourceTerm::try_from(term.to_owned()).expect("a term"),
        }
    }

    fn set(terms: &[&str]) -> CanonicalSet<RepCount> {
        let normalised = NormalisedSessionId::FIRST;
        CanonicalSet {
            outcome: Attributed::new(
                Performed::Completed(Some(RepCount::new(5).expect("a count"))),
                normalised,
            ),
            load: None,
            began: None,
            intensity: None,
            kind: None,
            rest_after: None,
            sources: terms.iter().map(|term| source(term)).collect(),
        }
    }

    fn session(sets: Vec<CanonicalSet<RepCount>>) -> CanonicalGymSession {
        CanonicalGymSession::new(
            Occurred::On("2026-11-20".parse().expect("a day")),
            NonEmpty::of(
                CanonicalItem::exercise(CanonicalExercise::ForReps {
                    identified: Attributed::new(
                        Identified::Recorded(RepsExercise::BackSquatBarbell),
                        NormalisedSessionId::FIRST,
                    ),
                    sets: NonEmpty::new(sets).expect("sets"),
                }),
                vec![],
            ),
            None,
            None,
        )
    }

    fn ids(session: i64, exercise: i64, set: i64) -> Placed {
        Placed {
            session: CanonicalSessionId::try_from(session).expect("an id"),
            exercise: CanonicalExerciseId::try_from(exercise).expect("an id"),
            set: CanonicalSetId::try_from(set).expect("an id"),
        }
    }

    /// One old set whose two source sets are now two sets: the first keeps the
    /// id and the second is new, so no id names two sets.
    #[test]
    fn a_split_set_keeps_its_id_once() {
        let placed = BTreeMap::from([(source("a"), ids(1, 1, 7)), (source("b"), ids(1, 1, 7))]);
        let identities = identify(&[session(vec![set(&["a"]), set(&["b"])])], &placed);
        let sets = &identities[0].exercises[0].sets;
        assert_eq!(
            sets,
            &vec![
                Issued::Kept(CanonicalSetId::try_from(7).expect("an id")),
                Issued::New
            ]
        );
        assert_eq!(
            identities[0].session,
            Issued::Kept(CanonicalSessionId::try_from(1).expect("an id"))
        );
    }

    /// Two old sets now merged into one: it keeps the first id its sources
    /// name, in their order.
    #[test]
    fn a_merged_set_keeps_the_first_id_its_sources_name() {
        let placed = BTreeMap::from([(source("a"), ids(1, 1, 7)), (source("b"), ids(1, 1, 8))]);
        let identities = identify(&[session(vec![set(&["a", "b"])])], &placed);
        assert_eq!(
            identities[0].exercises[0].sets,
            vec![Issued::Kept(CanonicalSetId::try_from(7).expect("an id"))]
        );
    }
}
