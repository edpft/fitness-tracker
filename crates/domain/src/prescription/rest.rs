//! How long to rest, and where that is decided.
//!
//! **Rest is a property of the transition after a set, not of a set.** It is
//! already on [`PrescribedSet::rest_after`]; what is here is the rule that fills
//! it in, and the three states that rule uses:
//!
//! - **absent** — no instruction was given. The warm-up ramp, where the rest is
//!   however long it takes to change the plates.
//! - **zero** — an instruction to go straight on. Not the same fact as absent,
//!   and the difference is load-bearing: a superset *tells* you not to rest.
//! - **a number, or a range** — the ordinary case.
//!
//! ## Zero between the members of a group is structural, not authored
//!
//! Nothing below states it, because it is what a superset *is*. A set followed
//! by another member of the same item rests for zero by definition; the block's
//! rest applies to the set the group ends on. Making that a parameter would let
//! it be authored to something else, which would describe a grouping that is not
//! a superset.
//!
//! ## What is authored
//!
//! One rest per block, and — where the block supersets at a different length —
//! one for the end of a group. Two blocks state both; the others state one,
//! because the operator rests the same however their work is grouped.

use crate::gym::{Duration, Spans};

use super::{
    shape::{Block, PrescribedExercise, PrescribedItem, SlotId, WorkoutShape},
    target::{PrescribedSet, Target},
};

/// What to rest, for one block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockRest {
    /// After a set of an exercise that stands on its own.
    pub between_sets: Target<Duration>,
    /// After the set a supersetted group ends on.
    ///
    /// `None` where the block rests the same however it is grouped — which is
    /// most of them. It is optional rather than defaulted to `between_sets`
    /// because "not stated" and "stated to be the same" are different, and only
    /// the first should silently follow a later change to `between_sets`.
    pub after_superset: Option<Target<Duration>>,
}

impl BlockRest {
    /// The rest this block instructs at the end of an item.
    const fn ending(self, supersetted: bool) -> Target<Duration> {
        match (supersetted, self.after_superset) {
            (true, Some(grouped)) => grouped,
            _ => self.between_sets,
        }
    }
}

/// What follows this exercise, which is what decides its rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    /// Another member of the same item follows. Go straight on — that is what a
    /// superset is, and it is structural rather than authored.
    StraightOn,
    /// This exercise ends its item. The block decides, knowing how it was
    /// grouped.
    Ends { supersetted: bool },
}

/// What to rest, block by block.
///
/// Per block rather than per slot: the slots within a block are worked alike,
/// and it is the blocks that differ from each other — the same reason
/// [`super::parameters::AccessoryScheme`] is stated per block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestScheme {
    pub plyometric: BlockRest,
    pub power: BlockRest,
    pub strength: BlockRest,
    pub hypertrophy: BlockRest,
    pub mobility: BlockRest,
}

impl RestScheme {
    pub const fn of(&self, block: Block) -> BlockRest {
        match block {
            Block::Plyometric => self.plyometric,
            Block::Power => self.power,
            Block::Strength => self.strength,
            Block::Hypertrophy => self.hypertrophy,
            Block::Mobility => self.mobility,
        }
    }
}

/// Fill in every set's rest, from the block it sits in and how it is grouped.
///
/// A function over the whole shape rather than something each slot's derivation
/// does for itself, because the two things that decide a rest — which block, and
/// whether another member of the item follows — are facts about the assembled
/// session. A slot being derived does not yet know whether it will be
/// supersetted.
#[must_use]
pub fn rested(shape: &WorkoutShape, scheme: &RestScheme) -> WorkoutShape {
    let items = shape.items().map_indexed(|_, item| match item {
        PrescribedItem::Exercise { slot, exercise } => PrescribedItem::Exercise {
            slot: *slot,
            exercise: exercise_rested(exercise, *slot, scheme, Ending::Ends { supersetted: false }),
        },
        PrescribedItem::Superset(superset) => {
            let last = superset.members.count().saturating_sub(1);
            let members = superset.members.map_indexed(|index, member| {
                // Every member but the last runs straight into the one after it.
                // Only the member the group ends on carries the block's rest.
                let ending = if index == last {
                    Ending::Ends { supersetted: true }
                } else {
                    Ending::StraightOn
                };
                super::shape::SupersetMember {
                    slot: member.slot,
                    exercise: exercise_rested(&member.exercise, member.slot, scheme, ending),
                }
            });
            PrescribedItem::Superset(super::shape::PrescribedSuperset { members })
        }
    });

    WorkoutShape::new(items)
}

/// One exercise's sets, rested.
fn exercise_rested(
    exercise: &PrescribedExercise,
    slot: SlotId,
    scheme: &RestScheme,
    ending: Ending,
) -> PrescribedExercise {
    let block = scheme.of(slot.block());
    let rest = match ending {
        Ending::StraightOn => Target::Exactly(Duration::ZERO),
        Ending::Ends { supersetted } => block.ending(supersetted),
    };

    match exercise {
        PrescribedExercise::ForReps { exercise, sets } => PrescribedExercise::ForReps {
            exercise: *exercise,
            sets: sets_rested(sets, rest, block),
        },
        PrescribedExercise::ForDuration { exercise, sets } => PrescribedExercise::ForDuration {
            exercise: *exercise,
            sets: sets_rested(sets, rest, block),
        },
        PrescribedExercise::ForDistance { exercise, sets } => PrescribedExercise::ForDistance {
            exercise: *exercise,
            sets: sets_rested(sets, rest, block),
        },
    }
}

/// **The warm-up ramp instructs no rest, and its last step instructs the least
/// the block asks for.**
///
/// Changing the plates is the rest between warm-up sets, and nothing prescribes
/// how long that takes. The step *into* the working set is different: it is the
/// first time the ramp asks for something, and what it asks for is the bottom of
/// the block's range rather than the whole of it.
fn sets_rested<M: Spans + Copy>(
    sets: &crate::gym::NonEmpty<PrescribedSet<M>>,
    rest: Target<Duration>,
    block: BlockRest,
) -> crate::gym::NonEmpty<PrescribedSet<M>> {
    let last_warmup = sets
        .iter()
        .enumerate()
        .filter(|(_, set)| set.warmup)
        .map(|(index, _)| index)
        .last();

    sets.map_indexed(|index, set| {
        let rest_after = if set.warmup {
            if Some(index) == last_warmup {
                Some(Target::Exactly(block.between_sets.minimum()))
            } else {
                None
            }
        } else {
            Some(rest)
        };

        PrescribedSet {
            prescription: set.prescription,
            rest_after,
            warmup: set.warmup,
        }
    })
}
