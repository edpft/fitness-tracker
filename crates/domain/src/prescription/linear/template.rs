//! The session template: seventeen slots in five blocks, in two variants.
//!
//! The variants differ in one thing — whether the primary lift is knee-dominant
//! or hip-dominant — and the other lower slot is then the accessory. Everything
//! else is the same session both ways.
//!
//! The template is expressed as types rather than as validation, so what it
//! forbids is unconstructible: [`PrimaryPattern`] has two variants so there is
//! exactly one primary and it is a lower lift; [`SlotFills`] names every slot as
//! a field so a programme missing one does not compile; and [`Position`] fixes
//! which slots superset together, so a pair is always exactly two and always the
//! same two.

use std::fmt;

use crate::{
    gym::{RepCount, exercise::Exercise},
    prescription::{
        schedule::{PerRole, SessionRole},
        shape::SlotId,
    },
};

/// Which lower slot the programme is trying to move.
///
/// The programme states this for the block; it does not vary by session. The
/// primary gets a warm-up ramp and a top-set/back-off scheme, and the other
/// lower slot becomes accessory-style precisely because it is not primary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrimaryPattern {
    KneeDominant,
    HipDominant,
}

impl PrimaryPattern {
    pub const ALL: &'static [Self] = &[Self::KneeDominant, Self::HipDominant];

    /// The stable key. Persisted.
    pub const fn as_str(self) -> &'static str {
        self.slot().as_str()
    }

    pub const fn slot(self) -> SlotId {
        match self {
            Self::KneeDominant => SlotId::KneeDominant,
            Self::HipDominant => SlotId::HipDominant,
        }
    }

    /// The lower slot the primary is not, which is the session's accessory.
    pub const fn accessory(self) -> SlotId {
        match self {
            Self::KneeDominant => SlotId::HipDominant,
            Self::HipDominant => SlotId::KneeDominant,
        }
    }

    /// The session's items, in issued order.
    ///
    /// Fatigue order across blocks, and within the strength block the primary
    /// first, then the supersetted upper pair, then the accessory lower slot.
    /// The one source of truth for both what a session issues and what a
    /// performed workout is projected against.
    pub const fn sequence(self) -> [Position; 11] {
        [
            Position::Single(SlotId::Plyometric),
            Position::Single(SlotId::Power),
            Position::Single(self.slot()),
            Position::Superset(SlotId::UpperPush, SlotId::UpperPull),
            Position::Single(self.accessory()),
            Position::Superset(SlotId::Biceps, SlotId::Triceps),
            Position::Superset(SlotId::WristFlexion, SlotId::WristExtension),
            Position::Single(SlotId::Core),
            Position::Single(SlotId::HandstandHold),
            Position::Single(SlotId::DeadHang),
            Position::Circuit(STRETCHES),
        ]
    }
}

/// The four stretches, performed as one group.
pub const STRETCHES: [SlotId; 4] = [
    SlotId::HipFlexorStretch,
    SlotId::HipExternalRotatorStretch,
    SlotId::HamstringStretch,
    SlotId::GroinStretch,
];

/// One position in the issued sequence.
///
/// A superset is a pair and not a list, because every one the template issues is
/// an antagonist pairing of two named slots — nothing here can express three of
/// them. The stretch circuit is a separate case rather than a widened superset
/// for the same reason: it is one fixed group of four, not a pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Single(SlotId),
    Superset(SlotId, SlotId),
    Circuit([SlotId; 4]),
}

impl Position {
    /// The slots this position fills, in issued order.
    pub fn slots(self) -> impl Iterator<Item = SlotId> {
        let (fixed, circuit) = match self {
            Self::Single(slot) => ([Some(slot), None], None),
            Self::Superset(first, second) => ([Some(first), Some(second)], None),
            Self::Circuit(slots) => ([None, None], Some(slots)),
        };
        fixed
            .into_iter()
            .flatten()
            .chain(circuit.into_iter().flatten())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} does not name a movement pattern")]
pub struct UnknownPattern {
    value: String,
}

impl TryFrom<String> for PrimaryPattern {
    type Error = UnknownPattern;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::ALL
            .iter()
            .find(|pattern| pattern.as_str() == value)
            .copied()
            .ok_or(UnknownPattern { value })
    }
}

impl fmt::Display for PrimaryPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A static slot's whole prescription.
///
/// **Set at the start of the block and never derived.** A static slot does not
/// progress, so there is nothing for history to say about it — and reading the
/// last performance would mean a bad session re-issuing itself. Pogos are three
/// sets of twenty because that is what the programme says, not because that is
/// what happened last time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaticFill {
    pub exercise: Exercise,
    pub sets: RepCount,
    pub reps: RepCount,
}

/// What fills a slot, which may differ by session role.
///
/// The alternating case is why the history projection is unbounded: on any given
/// session the exercise being prescribed was last performed two sessions ago,
/// not one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fill<T> {
    /// The same on both sessions.
    Same(T),
    /// One per session role — Nordic curls one day, back extension the other.
    Alternating(PerRole<T>),
}

impl<T> Fill<T> {
    pub const fn for_role(&self, role: SessionRole) -> &T {
        match self {
            Self::Same(fill) => fill,
            Self::Alternating(per_role) => per_role.get(role),
        }
    }

    /// Every distinct fill, whichever role it serves.
    ///
    /// What a history projection asks for: it needs the last performance of each
    /// exercise the programme can prescribe, not just this session's.
    pub fn all(&self) -> Vec<&T> {
        match self {
            Self::Same(fill) => vec![fill],
            Self::Alternating(per_role) => vec![&per_role.light, &per_role.heavy],
        }
    }
}

/// What a slot holds once a role is chosen.
///
/// Which shape a slot has is fixed by the template: the two static slots carry
/// their own sets and reps, and every other slot names one exercise and takes
/// its prescription from the block's parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotContent<'a> {
    Single(&'a Exercise),
    Static(&'a StaticFill),
}

/// One fill per slot, total by construction.
///
/// A struct with a named field per slot, so a programme missing a fill is a
/// compile error rather than a runtime one (§ 24).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotFills {
    pub plyometric: Fill<StaticFill>,
    pub power: Fill<StaticFill>,
    pub knee_dominant: Fill<Exercise>,
    pub upper_push: Fill<Exercise>,
    pub upper_pull: Fill<Exercise>,
    pub hip_dominant: Fill<Exercise>,
    pub biceps: Fill<Exercise>,
    pub triceps: Fill<Exercise>,
    pub wrist_flexion: Fill<Exercise>,
    pub wrist_extension: Fill<Exercise>,
    pub core: Fill<Exercise>,
    pub handstand_hold: Fill<Exercise>,
    pub dead_hang: Fill<Exercise>,
    pub hip_flexor_stretch: Fill<Exercise>,
    pub hip_external_rotator_stretch: Fill<Exercise>,
    pub hamstring_stretch: Fill<Exercise>,
    pub groin_stretch: Fill<Exercise>,
}

impl SlotFills {
    /// What fills a slot for a role.
    ///
    /// Total over [`SlotId`], so adding a slot to the template is a compile error
    /// here until it is filled.
    pub const fn content(&self, slot: SlotId, role: SessionRole) -> SlotContent<'_> {
        match slot {
            SlotId::Plyometric => SlotContent::Static(self.plyometric.for_role(role)),
            SlotId::Power => SlotContent::Static(self.power.for_role(role)),
            SlotId::KneeDominant => SlotContent::Single(self.knee_dominant.for_role(role)),
            SlotId::UpperPush => SlotContent::Single(self.upper_push.for_role(role)),
            SlotId::UpperPull => SlotContent::Single(self.upper_pull.for_role(role)),
            SlotId::HipDominant => SlotContent::Single(self.hip_dominant.for_role(role)),
            SlotId::Biceps => SlotContent::Single(self.biceps.for_role(role)),
            SlotId::Triceps => SlotContent::Single(self.triceps.for_role(role)),
            SlotId::WristFlexion => SlotContent::Single(self.wrist_flexion.for_role(role)),
            SlotId::WristExtension => SlotContent::Single(self.wrist_extension.for_role(role)),
            SlotId::Core => SlotContent::Single(self.core.for_role(role)),
            SlotId::HandstandHold => SlotContent::Single(self.handstand_hold.for_role(role)),
            SlotId::DeadHang => SlotContent::Single(self.dead_hang.for_role(role)),
            SlotId::HipFlexorStretch => SlotContent::Single(self.hip_flexor_stretch.for_role(role)),
            SlotId::HipExternalRotatorStretch => {
                SlotContent::Single(self.hip_external_rotator_stretch.for_role(role))
            }
            SlotId::HamstringStretch => SlotContent::Single(self.hamstring_stretch.for_role(role)),
            SlotId::GroinStretch => SlotContent::Single(self.groin_stretch.for_role(role)),
        }
    }

    /// What fills the primary slot.
    ///
    /// Separate from [`Self::content`] because the primary is always one of the
    /// two lower slots and both are single — so this is total in `Exercise`, and
    /// the caller has no impossible case to handle.
    pub const fn primary(&self, pattern: PrimaryPattern, role: SessionRole) -> &Exercise {
        match pattern {
            PrimaryPattern::KneeDominant => self.knee_dominant.for_role(role),
            PrimaryPattern::HipDominant => self.hip_dominant.for_role(role),
        }
    }

    /// Every exercise any slot can prescribe, whatever the role.
    ///
    /// What the history projection is asked for in one batch. Includes both sides
    /// of every alternating fill, because a session prescribes one and needs the
    /// other's history next time.
    pub fn every_exercise(&self) -> Vec<Exercise> {
        let mut exercises = Vec::new();
        for statics in [&self.plyometric, &self.power] {
            exercises.extend(statics.all().into_iter().map(|fill| fill.exercise));
        }
        for single in [
            &self.knee_dominant,
            &self.upper_push,
            &self.upper_pull,
            &self.hip_dominant,
            &self.biceps,
            &self.triceps,
            &self.wrist_flexion,
            &self.wrist_extension,
            &self.core,
            &self.handstand_hold,
            &self.dead_hang,
            &self.hip_flexor_stretch,
            &self.hip_external_rotator_stretch,
            &self.hamstring_stretch,
            &self.groin_stretch,
        ] {
            exercises.extend(single.all().into_iter().copied());
        }
        exercises.sort_unstable();
        exercises.dedup();
        exercises
    }
}
