//! A workout, as the operator designs it (#192).
//!
//! **One gym session of the microcycle**, from Eugene Teo's template and the
//! operator's preferences: the sets and repetitions each slot runs under double
//! progression, how long each set rests, and how long a static hold is held.
//! There are two, one for each gym session of the week, and each is edited on its
//! own (operator, 2026-10-27). The primary lift's sets, repetitions, loads and
//! rest are its progression's — [`LinearProgression`] or [`SbsProgression`] —
//! and are not here.
//!
//! **Set when the workout is designed, never asked when a plan is made** (§ 14.1).
//! It is kept as dated versions, and a plan is generated from the version in force
//! when it was planned, so a later version reaches only the plans made after it.
//!
//! **Reps and rest are stated at four levels**: for the workout, for a section,
//! for a superset within it, and for an exercise within that. The narrowest
//! stated stands (operator, 2026-10-26 and 2026-10-27).
//!
//! [`LinearProgression`]: super::linear::LinearProgression
//! [`SbsProgression`]: super::sbs::SbsProgression

use std::{collections::BTreeMap, fmt};

use crate::{
    measure::{Duration, RepCount},
    prescription::{Section, SlotId, Target},
};

/// The double-progression scheme a slot runs.
///
/// Work the range, and when the top of it is reached at every working set, add an
/// increment and start again at the bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessoryScheme {
    /// The rep target, as a target. **Not two loose bounds**: a `low` and a
    /// `high` beside each other can be written down inverted, and the check that
    /// would catch it belongs in the type rather than at every call site
    /// (§ 24). See [`Target`].
    pub reps: Target<RepCount>,
    pub sets: RepCount,
}

/// Exercises a workout runs back to back, named by their slots in the order
/// they are issued.
///
/// **Keyed by its slots**, because which slots are grouped is the template's
/// and stays the same whatever fills them. Two or more: a pair, or a circuit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Superset(Vec<SlotId>);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidSuperset {
    #[error("a superset groups two exercises or more")]
    TooFew,
    #[error(transparent)]
    UnknownSlot(#[from] super::shape::UnknownSlot),
}

impl Superset {
    /// The exercises of one superset, in issued order.
    pub fn of(first: SlotId, second: SlotId, rest: &[SlotId]) -> Self {
        Self(
            [first, second]
                .into_iter()
                .chain(rest.iter().copied())
                .collect(),
        )
    }

    pub fn slots(&self) -> &[SlotId] {
        &self.0
    }

    /// Every superset the template issues, whichever lift is primary: what a
    /// workout can state reps or rest for.
    pub fn issued() -> Vec<Self> {
        let mut issued: Vec<Self> = Vec::new();
        for pattern in super::linear::PrimaryPattern::ALL {
            for position in pattern.sequence() {
                let slots: Vec<SlotId> = position.slots().collect();
                if let [first, second, rest @ ..] = slots.as_slice() {
                    let superset = Self::of(*first, *second, rest);
                    if !issued.contains(&superset) {
                        issued.push(superset);
                    }
                }
            }
        }
        issued
    }
}

/// `knee_dominant+upper_push`, which is also how it is stored.
impl fmt::Display for Superset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for slot in &self.0 {
            if !first {
                f.write_str("+")?;
            }
            first = false;
            f.write_str(slot.as_str())?;
        }
        Ok(())
    }
}

impl TryFrom<String> for Superset {
    type Error = InvalidSuperset;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let slots = value
            .split('+')
            .map(|slot| SlotId::try_from(slot.trim().to_owned()))
            .collect::<Result<Vec<_>, _>>()?;
        if slots.len() < 2 {
            return Err(InvalidSuperset::TooFew);
        }
        Ok(Self(slots))
    }
}

/// A value stated for the workout, and wherever narrower it differs.
///
/// **The narrowest stated stands**: an exercise's own, then its superset's, then
/// its section's, then the workout's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Levels<T> {
    /// What every slot runs unless something narrower says otherwise.
    pub workout: T,
    pub sections: BTreeMap<Section, T>,
    pub supersets: BTreeMap<Superset, T>,
    pub exercises: BTreeMap<SlotId, T>,
}

impl<T> Levels<T> {
    /// One value for everything.
    pub const fn only(workout: T) -> Self {
        Self {
            workout,
            sections: BTreeMap::new(),
            supersets: BTreeMap::new(),
            exercises: BTreeMap::new(),
        }
    }

    /// What a slot runs, within the superset it is issued in if it is in one.
    pub fn for_slot(&self, slot: SlotId, within: Option<&Superset>) -> &T {
        self.exercises
            .get(&slot)
            .or_else(|| within.and_then(|superset| self.supersets.get(superset)))
            .or_else(|| self.sections.get(&slot.section()))
            .unwrap_or(&self.workout)
    }
}

/// How long to rest, for a workout or for a section.
///
/// **Two numbers, because a section rests differently once a superset ends.**
/// Going straight on between the members of a superset is not here: it is what a
/// superset is, and making it a value would let it be authored to something that
/// described another grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rest {
    /// After a set of an exercise that stands on its own.
    pub between_sets: Target<Duration>,
    /// After the set a superset ends on.
    ///
    /// `None` where the section rests the same however it is grouped. Optional
    /// rather than defaulted to `between_sets` because "not stated" and "stated
    /// to be the same" are different, and only the first should silently follow
    /// a later change to `between_sets`.
    pub after_superset: Option<Target<Duration>>,
}

impl Rest {
    /// The rest at the end of an item, knowing how it was grouped.
    pub const fn ending(self, supersetted: bool) -> Target<Duration> {
        match (supersetted, self.after_superset) {
            (true, Some(grouped)) => grouped,
            _ => self.between_sets,
        }
    }
}

/// How long a workout rests, at each of its levels.
///
/// **Not a [`Levels`] of one type**, because what is stated differs by level: a
/// workout or a section states a rest for an exercise on its own and one for the
/// end of a superset, a superset states only the rest after its round, and an
/// exercise only the rest between its own sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestScheme {
    pub workout: Rest,
    pub sections: BTreeMap<Section, Rest>,
    /// After the set a superset's round ends on.
    pub supersets: BTreeMap<Superset, Target<Duration>>,
    /// Between the sets of an exercise that stands on its own.
    pub exercises: BTreeMap<SlotId, Target<Duration>>,
}

impl RestScheme {
    /// The rest after a set that ends its item: an exercise standing on its own,
    /// or the last member of a superset.
    pub fn ending(&self, slot: SlotId, within: Option<&Superset>) -> Target<Duration> {
        let section = self
            .sections
            .get(&slot.section())
            .copied()
            .unwrap_or(self.workout);
        within.map_or_else(
            || {
                self.exercises
                    .get(&slot)
                    .copied()
                    .unwrap_or(section.between_sets)
            },
            |superset| {
                self.supersets
                    .get(superset)
                    .copied()
                    .unwrap_or_else(|| section.ending(true))
            },
        )
    }
}

/// One version of one workout.
///
/// **Held together rather than reached for one value at a time** because it is
/// one statement about a session, and a derivation that took the reps from one
/// version and the rest from another would be describing two workouts at once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workout {
    /// What each non-primary strength and hypertrophy slot runs.
    pub reps: Levels<AccessoryScheme>,
    /// How long to rest after each set that is not the primary's.
    pub rest: RestScheme,
    /// How long a static hold is held for.
    ///
    /// The mobility work does not progress — it is held, and the same length
    /// every time — so its prescription comes from here rather than from
    /// history. One duration for every static slot.
    pub static_hold: Duration,
}

impl Workout {
    /// This workout with one slot's sets and reps, and the rest that ends it,
    /// as stated for that slot (#771).
    ///
    /// **Stated at the narrowest level that reaches it, and only where it
    /// differs**: sets and reps for the slot itself, and the rest for the
    /// slot, or for its superset's round where it is in one, since a member
    /// does not rest before the next. A value it already runs is not stated
    /// again, so restating a slot as it stands leaves the workout as it was.
    #[must_use]
    pub fn restated(
        mut self,
        slot: SlotId,
        within: Option<&Superset>,
        scheme: Option<AccessoryScheme>,
        rest: Option<Target<Duration>>,
    ) -> Self {
        if let Some(scheme) = scheme
            && *self.reps.for_slot(slot, within) != scheme
        {
            self.reps.exercises.insert(slot, scheme);
        }
        if let Some(rest) = rest
            && self.rest.ending(slot, within) != rest
        {
            match within {
                Some(superset) => {
                    self.rest.supersets.insert(superset.clone(), rest);
                }
                None => {
                    self.rest.exercises.insert(slot, rest);
                }
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        measure::{Duration, RepCount},
        prescription::{SlotId, Target, seed},
    };

    use super::{AccessoryScheme, Superset};

    /// **A slot restated as it stands leaves the workout as it was**, so a
    /// form saved unchanged adds nothing narrower.
    #[test]
    fn restating_a_slot_as_it_stands_changes_nothing() {
        let workout = seed::workout().expect("the seed builds");
        let scheme = *workout.reps.for_slot(SlotId::Core, None);
        let rest = workout.rest.ending(SlotId::Core, None);

        let restated = workout
            .clone()
            .restated(SlotId::Core, None, Some(scheme), Some(rest));

        assert_eq!(restated, workout);
    }

    /// **A member's rest is its superset's round**, and its reps its own.
    #[test]
    fn a_members_rest_is_its_rounds() {
        let workout = seed::workout().expect("the seed builds");
        let pair = Superset::of(SlotId::Biceps, SlotId::Triceps, &[]);
        let scheme = AccessoryScheme {
            reps: Target::Exactly(RepCount::new(15).expect("a count")),
            sets: RepCount::new(2).expect("a count"),
        };
        let rest = Target::Exactly(Duration::from_seconds(45));

        let restated = workout.restated(SlotId::Biceps, Some(&pair), Some(scheme), Some(rest));

        assert_eq!(*restated.reps.for_slot(SlotId::Biceps, Some(&pair)), scheme);
        assert_ne!(
            *restated.reps.for_slot(SlotId::Triceps, Some(&pair)),
            scheme
        );
        assert_eq!(restated.rest.ending(SlotId::Triceps, Some(&pair)), rest);
        assert!(restated.rest.exercises.is_empty());
    }
}
