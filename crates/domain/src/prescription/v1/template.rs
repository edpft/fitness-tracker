//! What `v1` will and will not build.
//!
//! Eugene Teo's hybrid bodybuilding template, minus cardio. Five blocks in
//! fatigue order and eleven slots, and the structural facts below bound what is
//! constructible rather than configuring it.
//!
//! - **Exactly one strength slot is primary.** [`PrimaryPattern`] is an enum, so
//!   two primaries and zero primaries are both unrepresentable.
//! - **The strength block requires all four patterns.** [`SlotFills`] names them
//!   as fields, so a programme missing a hip-dominant fill does not compile.
//! - **The upper pair is supersetted; the lower pair is not.** Not a preference,
//!   and not authored — the antagonist pairing needs no separate expression
//!   because the required pattern set already delivers a push against a pull.
//! - **The hypertrophy block is two supersets and one single slot.** `core` is
//!   typed as one exercise, so it cannot be supersetted. Fifteen consecutive
//!   sessions have it unpaired and last in the block.
//!
//! **`Pattern` is not a field.** If the strength block names its four slots, the
//! field name *is* the pattern; a `pattern:` field beside it would be a second
//! source of truth that can disagree.

use std::fmt;

use crate::{
    gym::{RepCount, exercise::Exercise, sequence::AtLeastTwo},
    prescription::{
        schedule::{PerRole, SessionRole},
        shape::SlotId,
    },
};

/// Which strength slot the programme is trying to move.
///
/// Everything asymmetric derives from this: the primary gets a warm-up ramp and
/// a top-set/back-off scheme, and the other lower slot becomes accessory-style
/// precisely because it is not primary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrimaryPattern {
    KneeDominant,
    HipDominant,
    UpperPush,
    UpperPull,
}

impl PrimaryPattern {
    pub const ALL: &'static [Self] = &[
        Self::KneeDominant,
        Self::HipDominant,
        Self::UpperPush,
        Self::UpperPull,
    ];

    /// The stable key. Persisted.
    pub const fn as_str(self) -> &'static str {
        self.slot().as_str()
    }

    /// The slot this pattern names.
    pub const fn slot(self) -> SlotId {
        match self {
            Self::KneeDominant => SlotId::KneeDominant,
            Self::HipDominant => SlotId::HipDominant,
            Self::UpperPush => SlotId::UpperPush,
            Self::UpperPull => SlotId::UpperPull,
        }
    }

    /// The lower-body pattern this one is not.
    ///
    /// The accessory lower slot is *the lower pattern the primary is not*, which
    /// is a constraint referencing another slot — inexpressible if the block held
    /// exercises directly rather than slots. `None` when the primary is an upper
    /// slot, where neither lower slot is the accessory by this rule.
    pub const fn other_lower(self) -> Option<SlotId> {
        match self {
            Self::KneeDominant => Some(SlotId::HipDominant),
            Self::HipDominant => Some(SlotId::KneeDominant),
            Self::UpperPush | Self::UpperPull => None,
        }
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
/// Two shapes, and which one a slot has is fixed by the template rather than by
/// the fill: `core` is always single and `arms` is always a superset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotContent<'a> {
    Single(&'a Exercise),
    Superset(&'a AtLeastTwo<Exercise>),
    /// A slot the programme prescribes outright.
    Static(&'a StaticFill),
}

/// One fill per slot, total by construction.
///
/// A struct with a named field per slot, so a programme missing a fill is a
/// compile error rather than a runtime one (§ 24) — the same mechanism as the
/// strength block's four patterns, which is what these six fields are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotFills {
    pub plyometric: Fill<StaticFill>,
    pub power: Fill<StaticFill>,
    pub knee_dominant: Fill<Exercise>,
    pub upper_push: Fill<Exercise>,
    pub upper_pull: Fill<Exercise>,
    pub hip_dominant: Fill<Exercise>,
    pub arms: Fill<AtLeastTwo<Exercise>>,
    pub forearms: Fill<AtLeastTwo<Exercise>>,
    pub core: Fill<Exercise>,
    pub mobility_hold: Fill<Exercise>,
    pub mobility_stretch: Fill<AtLeastTwo<Exercise>>,
}

impl SlotFills {
    /// What fills a slot for a role.
    ///
    /// Total over [`SlotId`], so adding a slot to the template is a compile error
    /// here until it is filled — which is the point of the exhaustive match.
    pub const fn content(&self, slot: SlotId, role: SessionRole) -> SlotContent<'_> {
        match slot {
            SlotId::Plyometric => SlotContent::Static(self.plyometric.for_role(role)),
            SlotId::Power => SlotContent::Static(self.power.for_role(role)),
            SlotId::KneeDominant => SlotContent::Single(self.knee_dominant.for_role(role)),
            SlotId::UpperPush => SlotContent::Single(self.upper_push.for_role(role)),
            SlotId::UpperPull => SlotContent::Single(self.upper_pull.for_role(role)),
            SlotId::HipDominant => SlotContent::Single(self.hip_dominant.for_role(role)),
            SlotId::Arms => SlotContent::Superset(self.arms.for_role(role)),
            SlotId::Forearms => SlotContent::Superset(self.forearms.for_role(role)),
            SlotId::Core => SlotContent::Single(self.core.for_role(role)),
            SlotId::MobilityHold => SlotContent::Single(self.mobility_hold.for_role(role)),
            SlotId::MobilityStretch => SlotContent::Superset(self.mobility_stretch.for_role(role)),
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
            &self.core,
            &self.mobility_hold,
        ] {
            exercises.extend(single.all().into_iter().copied());
        }
        for superset in [&self.arms, &self.forearms, &self.mobility_stretch] {
            for members in superset.all() {
                exercises.extend(members.iter().copied());
            }
        }
        exercises.sort_unstable();
        exercises.dedup();
        exercises
    }
}
