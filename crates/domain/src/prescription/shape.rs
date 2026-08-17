//! What to do, separated from where it came from.
//!
//! A [`WorkoutShape`] is the instructional content of a session and nothing
//! else. It is the common currency between a generated prescription and a
//! performed workout projected into prescription's vocabulary — the two have the
//! same structure, which is not a coincidence: the issued grouping is
//! structurally what the performed model calls a `WorkoutItem`.
//!
//! **Keeping the shape apart from the issuance is what protects § 11.** A
//! prescription that was issued carries an anchor, a week, a date and a
//! programme; a shape carries none of those and so cannot be stored as one. Were
//! they the same type, a shape derived from a performance could be handed to the
//! store, and the record would then hold a prescription reverse-engineered from
//! the very performance it exists to be compared against — which makes
//! expectation against reality unrecoverable. See
//! [`super::workout::PrescribedWorkout`].
//!
//! **Blocks do not survive into what is issued.** They are construction-time
//! scaffolding. One thing does survive: every item is slot-tagged, or "same
//! slot, different cycle" stops being answerable — and that comparability was
//! the argument for slots existing at all. Block is derivable from slot.

use std::fmt;

use crate::gym::{
    Distance, Duration, RepCount,
    exercise::{DistanceExercise, DurationExercise, RepsExercise},
    sequence::{AtLeastTwo, NonEmpty},
};

use super::target::PrescribedSet;

/// Which slot of the template an item fills.
///
/// A closed vocabulary rather than a string, so a typo is a compile error and
/// "the same slot across cycles" is a comparison the type supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SlotId {
    Plyometric,
    Power,
    KneeDominant,
    UpperPush,
    UpperPull,
    HipDominant,
    Arms,
    Forearms,
    Core,
    MobilityHold,
    MobilityStretch,
}

impl SlotId {
    /// Every slot, in the order a session issues them.
    ///
    /// Fatigue order across blocks, and within the strength block the primary
    /// first, then the supersetted upper pair, then the remaining lower slot as
    /// the accessory.
    pub const ALL: &'static [Self] = &[
        Self::Plyometric,
        Self::Power,
        Self::KneeDominant,
        Self::UpperPush,
        Self::UpperPull,
        Self::HipDominant,
        Self::Arms,
        Self::Forearms,
        Self::Core,
        Self::MobilityHold,
        Self::MobilityStretch,
    ];

    /// The stable key. Persisted, so it outlives a rename.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Plyometric => "plyometric",
            Self::Power => "power",
            Self::KneeDominant => "knee_dominant",
            Self::UpperPush => "upper_push",
            Self::UpperPull => "upper_pull",
            Self::HipDominant => "hip_dominant",
            Self::Arms => "arms",
            Self::Forearms => "forearms",
            Self::Core => "core",
            Self::MobilityHold => "mobility_hold",
            Self::MobilityStretch => "mobility_stretch",
        }
    }

    /// Which block this slot belongs to. Derived, never stored beside the slot —
    /// a second source of truth is a second thing that can disagree.
    pub const fn block(self) -> Block {
        match self {
            Self::Plyometric => Block::Plyometric,
            Self::Power => Block::Power,
            Self::KneeDominant | Self::UpperPush | Self::UpperPull | Self::HipDominant => {
                Block::Strength
            }
            Self::Arms | Self::Forearms | Self::Core => Block::Hypertrophy,
            Self::MobilityHold | Self::MobilityStretch => Block::Mobility,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} does not name a slot")]
pub struct UnknownSlot {
    value: String,
}

impl TryFrom<String> for SlotId {
    type Error = UnknownSlot;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::ALL
            .iter()
            .find(|slot| slot.as_str() == value)
            .copied()
            .ok_or(UnknownSlot { value })
    }
}

impl fmt::Display for SlotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The five qualities, in fatigue order.
///
/// Cheapest first, then most CNS-demanding, then most technical, and nothing
/// that pre-fatigues what follows. Ordering is a property of the quality, so
/// nothing carries an index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Block {
    Plyometric,
    Power,
    Strength,
    Hypertrophy,
    Mobility,
}

impl Block {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Plyometric => "plyometric",
            Self::Power => "power",
            Self::Strength => "strength",
            Self::Hypertrophy => "hypertrophy",
            Self::Mobility => "mobility",
        }
    }
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One exercise together with the sets prescribed of it.
///
/// The measure partition from the performed side, reused: which variant this is
/// fixes the measure, so a prescribed set and its exercise cannot disagree and
/// nothing validates the pairing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrescribedExercise {
    ForReps {
        exercise: RepsExercise,
        sets: NonEmpty<PrescribedSet<RepCount>>,
    },
    ForDuration {
        exercise: DurationExercise,
        sets: NonEmpty<PrescribedSet<Duration>>,
    },
    ForDistance {
        exercise: DistanceExercise,
        sets: NonEmpty<PrescribedSet<Distance>>,
    },
}

impl PrescribedExercise {
    pub const fn exercise_key(&self) -> &'static str {
        match self {
            Self::ForReps { exercise, .. } => exercise.as_str(),
            Self::ForDuration { exercise, .. } => exercise.as_str(),
            Self::ForDistance { exercise, .. } => exercise.as_str(),
        }
    }

    pub const fn measure(&self) -> &'static str {
        match self {
            Self::ForReps { .. } => "reps",
            Self::ForDuration { .. } => "duration",
            Self::ForDistance { .. } => "distance",
        }
    }

    pub const fn set_count(&self) -> usize {
        match self {
            Self::ForReps { sets, .. } => sets.count(),
            Self::ForDuration { sets, .. } => sets.count(),
            Self::ForDistance { sets, .. } => sets.count(),
        }
    }

    /// How many of the sets are working sets. What a volume count is over.
    pub fn working_set_count(&self) -> usize {
        match self {
            Self::ForReps { sets, .. } => sets.iter().filter(|set| !set.warmup).count(),
            Self::ForDuration { sets, .. } => sets.iter().filter(|set| !set.warmup).count(),
            Self::ForDistance { sets, .. } => sets.iter().filter(|set| !set.warmup).count(),
        }
    }
}

impl fmt::Display for PrescribedExercise {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} × {}", self.exercise_key(), self.set_count())
    }
}

/// One member of a superset, and the slot it fills.
///
/// **The slot is per member, not per item**, because the two cases a superset
/// covers are genuinely different. The upper strength pair is two slots
/// supersetted together — push against pull. The arms superset is *one* slot with
/// two members, biceps and triceps. Tagging the item would collapse the second
/// case, and tagging with a list of slots would make it a list of one where
/// `AtLeastTwo` demands two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupersetMember {
    pub slot: SlotId,
    pub exercise: PrescribedExercise,
}

/// Exercises prescribed back to back.
///
/// At least two, by construction — a superset of one is a contradiction, and the
/// performed side already refuses the same thing on the way in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrescribedSuperset {
    pub members: AtLeastTwo<SupersetMember>,
}

/// One position in the issued sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrescribedItem {
    Exercise {
        slot: SlotId,
        exercise: PrescribedExercise,
    },
    Superset(PrescribedSuperset),
}

impl PrescribedItem {
    /// Every prescribed exercise in this item, in order.
    pub fn exercises(&self) -> Box<dyn Iterator<Item = &PrescribedExercise> + Send + '_> {
        match self {
            Self::Exercise { exercise, .. } => Box::new(std::iter::once(exercise)),
            Self::Superset(superset) => {
                Box::new(superset.members.iter().map(|member| &member.exercise))
            }
        }
    }

    /// Every slot this item fills, in member order.
    ///
    /// May repeat: the arms superset fills one slot twice.
    pub fn slots(&self) -> Box<dyn Iterator<Item = SlotId> + Send + '_> {
        match self {
            Self::Exercise { slot, .. } => Box::new(std::iter::once(*slot)),
            Self::Superset(superset) => Box::new(superset.members.iter().map(|member| member.slot)),
        }
    }

    pub const fn is_superset(&self) -> bool {
        matches!(self, Self::Superset { .. })
    }
}

/// The instructional content of a session.
///
/// What to do, and nothing about where it came from. A generated prescription
/// holds one of these; so does a performance projected into prescription's
/// vocabulary. Neither can be mistaken for the other, because only the first is
/// wrapped in the issuance facts that make it a prescription.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkoutShape {
    items: NonEmpty<PrescribedItem>,
}

impl WorkoutShape {
    pub const fn new(items: NonEmpty<PrescribedItem>) -> Self {
        Self { items }
    }

    pub const fn items(&self) -> &NonEmpty<PrescribedItem> {
        &self.items
    }

    /// Every prescribed exercise, flattened across items in issued order.
    pub fn exercises(&self) -> impl Iterator<Item = &PrescribedExercise> {
        self.items.iter().flat_map(PrescribedItem::exercises)
    }

    /// The item filling a given slot, if the session has one.
    pub fn item_for(&self, slot: SlotId) -> Option<&PrescribedItem> {
        self.items
            .iter()
            .find(|item| item.slots().any(|filled| filled == slot))
    }

    pub fn set_count(&self) -> usize {
        self.exercises()
            .map(PrescribedExercise::set_count)
            .sum::<usize>()
    }
}

impl fmt::Display for WorkoutShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} items, {} sets", self.items.count(), self.set_count())
    }
}
