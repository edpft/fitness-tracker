//! What the operator would consider for each slot.
//!
//! **Preference, not domain fact**, which is why this is here and not in
//! `domain`. That a leg extension is knee-dominant is true of anyone; that these
//! are the three he would pick is true of him. `docs/slot-candidates.md` states
//! them and this is that document in a form the wizard can read — the same
//! standing `catalogue` has for what this build can collect.
//!
//! **Offered, never enforced.** A slot may be filled with anything in the
//! vocabulary: the operator asked to see options he has not done before, and a
//! list that refused them would be a worse list than no list. What ordering by
//! the record buys is that the answer he usually gives is the one already in
//! front of him.
//!
//! Where the document states candidates, these are its candidates. Where it does
//! not — the wrists, the core, the holds — these are what he has been doing,
//! which is the same question answered from the record instead of from him.

use domain::prescription::SlotId;

/// The candidates for one slot, as vocabulary keys.
///
/// Order here is the operator's own, and is the tie-break when the record has
/// nothing to say. The wizard sorts by what has been performed first.
pub const fn for_slot(slot: SlotId) -> &'static [&'static str] {
    match slot {
        // Stated in `docs/slot-candidates.md`. The primary lists and the
        // accessory lists are different, and which applies depends on whether
        // the programme makes the pattern its primary — the wizard knows which
        // and asks with the right one.
        SlotId::KneeDominant => &[
            "squat-barbell",
            "front-squat",
            "bulgarian-split-squat-barbell",
            "leg-extension-machine",
            "bulgarian-split-squat-dumbbell",
            "sissy-squat",
        ],
        SlotId::HipDominant => &[
            "deadlift-barbell",
            "romanian-deadlift-barbell",
            "back-extension-machine",
            "nordic-hamstrings-curls",
            "seated-leg-curl-machine",
            "lying-leg-curl-machine",
        ],
        SlotId::UpperPull => &[
            "neutral-grip-pull-up",
            "pull-up",
            "ring-rows",
            "bent-over-row-barbell",
            "pendlay-row-barbell",
        ],
        SlotId::UpperPush => &["chest-dip", "bench-press-barbell", "overhead-press-barbell"],
        SlotId::Triceps => &[
            "single-arm-tricep-extension-dumbbell",
            "overhead-triceps-extension-cable",
            "skullcrusher-barbell",
        ],
        SlotId::Biceps => &[
            "preacher-curl-barbell",
            "behind-the-back-curl-cable",
            "seated-incline-curl-dumbbell",
        ],

        // Not stated in the document. What the record holds, which for these is
        // the same answer arrived at from the other side.
        SlotId::Plyometric => &["pogo", "box-jump", "burpee"],
        SlotId::Power => &["box-jump", "power-clean", "push-press", "kettlebell-swing"],
        SlotId::WristFlexion => &[
            "wrist-flexion-dumbbell",
            "behind-the-back-wrist-curl-barbell",
        ],
        SlotId::WristExtension => &["wrist-extension-dumbbell", "seated-wrist-extension-barbell"],
        SlotId::Core => &[
            "bent-over-cable-chop",
            "cable-twist-up-to-down",
            "hammer-twists",
            "hanging-knee-raise",
            "dead-bug",
        ],
        SlotId::HandstandHold => &["handstand-hold"],
        SlotId::DeadHang => &["dead-hang"],
        SlotId::HipFlexorStretch => &["couch-stretch"],
        SlotId::HipExternalRotatorStretch => &["ninety-ninety"],
        SlotId::HamstringStretch => &["standing-straddle-fold"],
        SlotId::GroinStretch => &["squatting-groin-stretch", "ninety-ninety"],
    }
}

#[cfg(test)]
mod tests {
    use super::for_slot;
    use domain::{gym::exercise::Exercise, prescription::SlotId};

    /// **Every candidate names a real exercise.**
    ///
    /// These are string keys and nothing else checks them: a typo would offer
    /// the operator an exercise that then fails to author, halfway through a
    /// wizard, after he has answered thirty questions. The vocabulary is the
    /// authority and this asks it.
    #[test]
    fn every_candidate_is_in_the_vocabulary() {
        for slot in SlotId::ALL {
            let offered = for_slot(*slot);
            assert!(!offered.is_empty(), "{slot} offers nothing");

            for key in offered {
                assert!(
                    crate::wizard::exercise_named(key).is_some(),
                    "{slot} offers {key:?}, which is not in the vocabulary"
                );
            }
        }
    }

    /// A slot offers no exercise twice.
    #[test]
    fn no_slot_repeats_a_candidate() {
        for slot in SlotId::ALL {
            let offered = for_slot(*slot);
            let mut seen: Vec<&&str> = offered.iter().collect();
            seen.sort_unstable();
            let before = seen.len();
            seen.dedup();
            assert_eq!(before, seen.len(), "{slot} repeats a candidate");
        }
    }

    /// **A held slot offers only exercises counted in time**, and a slot worked
    /// for repetitions offers only exercises counted in repetitions.
    ///
    /// The measure is fixed by which vocabulary an exercise belongs to, so an
    /// offer that crosses it is a slot that cannot be derived — `NotAHold` or
    /// `NotCountedInReps` — and the wizard would be proposing a dead end.
    #[test]
    fn a_candidate_is_counted_the_way_its_slot_needs() {
        for slot in SlotId::ALL {
            for key in for_slot(*slot) {
                let Some(exercise) = crate::wizard::exercise_named(key) else {
                    panic!("{key} is in the vocabulary")
                };
                let held = matches!(slot.block(), domain::prescription::Block::Mobility);
                match (held, exercise) {
                    (true, Exercise::Duration(_)) | (false, Exercise::Reps(_)) => {}
                    _ => panic!(
                        "{slot} offers {key:?}, which is counted in {}",
                        exercise.measure()
                    ),
                }
            }
        }
    }
}
