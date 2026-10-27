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
//! Nothing states it, because it is what a superset is. A set followed by
//! another member of the same item rests for zero by definition; the workout's
//! rest applies to the set the group ends on. Making that a parameter would let
//! it be authored to something else, which would describe a grouping that is not
//! a superset.
//!
//! ## What is authored, and by whom
//!
//! The workout states its rest at four levels — see [`RestScheme`]. **The primary
//! lift's rest is not the workout's**: its progression owns its sets, so it owns
//! what each kind of set rests for (operator, 2026-10-27), and its sets arrive here
//! already rested.

use crate::measure::{Duration, Spans};

use super::{
    design::{RestScheme, Superset},
    shape::{PrescribedExercise, PrescribedItem, SlotId, WorkoutShape},
    target::{PrescribedSet, Target},
};

/// What follows this exercise, which is what decides its rest.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Ending {
    /// Another member of the same item follows. Go straight on — that is what a
    /// superset is, and it is structural rather than authored.
    StraightOn,
    /// This exercise ends its item, which is a superset if it is in one.
    Ends { within: Option<Superset> },
}

/// Fill in every set's rest but the primary's, from where it sits and how it is
/// grouped.
///
/// A function over the whole shape rather than something each slot's derivation
/// does for itself, because the two things that decide a rest — which section and
/// superset, and whether another member of the item follows — are facts about
/// the assembled session.
#[must_use]
pub fn rested(shape: &WorkoutShape, scheme: &RestScheme, primary: SlotId) -> WorkoutShape {
    let items = shape.items().map_indexed(|_, item| match item {
        PrescribedItem::Exercise { slot, exercise } if *slot == primary => {
            PrescribedItem::Exercise {
                slot: *slot,
                exercise: exercise.clone(),
            }
        }
        PrescribedItem::Exercise { slot, exercise } => PrescribedItem::Exercise {
            slot: *slot,
            exercise: exercise_rested(exercise, *slot, scheme, &Ending::Ends { within: None }),
        },
        PrescribedItem::Superset(superset) => {
            let slots: Vec<SlotId> = superset.members.iter().map(|member| member.slot).collect();
            let grouped = match slots.as_slice() {
                [first, second, rest @ ..] => Some(Superset::of(*first, *second, rest)),
                _ => None,
            };
            let last = superset.members.count().saturating_sub(1);
            let members = superset.members.map_indexed(|index, member| {
                // Every member but the last runs straight into the one after it.
                // Only the member the group ends on carries the superset's rest.
                let ending = if index == last {
                    Ending::Ends {
                        within: grouped.clone(),
                    }
                } else {
                    Ending::StraightOn
                };
                super::shape::SupersetMember {
                    slot: member.slot,
                    exercise: exercise_rested(&member.exercise, member.slot, scheme, &ending),
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
    ending: &Ending,
) -> PrescribedExercise {
    let rest = match ending {
        Ending::StraightOn => Target::Exactly(Duration::ZERO),
        Ending::Ends { within } => scheme.ending(slot, within.as_ref()),
    };
    let into_work = scheme.ending(slot, None).minimum();

    match exercise {
        PrescribedExercise::ForReps {
            exercise,
            sets,
            assisted,
        } => PrescribedExercise::ForReps {
            exercise: *exercise,
            sets: sets_rested(sets, rest, into_work),
            assisted: assisted.clone(),
        },
        PrescribedExercise::ForDuration { exercise, sets } => PrescribedExercise::ForDuration {
            exercise: *exercise,
            sets: sets_rested(sets, rest, into_work),
        },
        PrescribedExercise::ForDistance { exercise, sets } => PrescribedExercise::ForDistance {
            exercise: *exercise,
            sets: sets_rested(sets, rest, into_work),
        },
    }
}

/// **A warm-up ramp instructs no rest, and its last step instructs the least
/// the working set asks for.**
///
/// Changing the plates is the rest between warm-up sets, and nothing prescribes
/// how long that takes. The step *into* the working set is different: it is the
/// first time the ramp asks for something, and what it asks for is the bottom of
/// the working set's range rather than the whole of it.
///
/// Public because the primary's sets are rested by its progression and take the
/// same rule into their first working set.
#[must_use]
pub fn warmup_rested<M: Spans + Copy>(
    sets: &crate::sequence::NonEmpty<PrescribedSet<M>>,
    into_work: Duration,
) -> crate::sequence::NonEmpty<PrescribedSet<M>> {
    let last_warmup = sets
        .iter()
        .enumerate()
        .filter(|(_, set)| set.warmup)
        .map(|(index, _)| index)
        .last();

    sets.map_indexed(|index, set| {
        if !set.warmup {
            return *set;
        }
        PrescribedSet {
            prescription: set.prescription,
            rest_after: (Some(index) == last_warmup).then_some(Target::Exactly(into_work)),
            warmup: true,
        }
    })
}

fn sets_rested<M: Spans + Copy>(
    sets: &crate::sequence::NonEmpty<PrescribedSet<M>>,
    rest: Target<Duration>,
    into_work: Duration,
) -> crate::sequence::NonEmpty<PrescribedSet<M>> {
    let working = sets.map_indexed(|_, set| PrescribedSet {
        prescription: set.prescription,
        rest_after: if set.warmup { None } else { Some(rest) },
        warmup: set.warmup,
    });
    warmup_rested(&working, into_work)
}
