//! The workout's shape, which is one shape and does not vary.
//!
//! **These are not parameters, and a value with one value is not a knob.** They
//! were fields on [`GenerationParameters`] until they were read back as a
//! question — what would a second set of them describe? — and the answer was
//! nothing: the operator trains one ramp, one pair of back-off patterns, one
//! pair of top sets, one accessory scheme per block and one reset protocol
//! twice over. A settings table holding a value nobody will ever set again
//! offers a degree of freedom the training does not have.
//!
//! **What remains a parameter is what would still be true if no programme were
//! being authored**: the load steps each implement moves in, which is what is on
//! the rack whoever is training on it. That stayed in
//! [`GenerationParameters`](super::GenerationParameters).
//!
//! **Built rather than `const`, for the same reason the seed is.** Every value
//! below is validated on the way in — a repetition count is non-zero, a
//! percentage lands on a hundredth — and a `const` cannot run a check without
//! panicking on failure, which is forbidden here. So this is a function
//! returning a `Result` nothing can make fail, and [`pinned`](self) is the test
//! that keeps it that way.
//!
//! [`GenerationParameters`]: super::GenerationParameters

use std::fmt;

use crate::{
    measure::{Kg, RepCount},
    prescription::{PerRole, Percentage, Target},
    sequence::NonEmpty,
};

/// How many repetitions a top set is prescribed for.
///
/// A count and not a range: the primary's top set is executed as written, which
/// is what makes it pass or fail rather than scored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TopSetReps(RepCount);

impl TopSetReps {
    pub const fn new(reps: RepCount) -> Self {
        Self(reps)
    }

    pub const fn as_rep_count(self) -> RepCount {
        self.0
    }
}

impl fmt::Display for TopSetReps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One step of the ramp before a top set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WarmupStep {
    /// Of the session's own top set, never of the anchor.
    pub of_top_set: Percentage,
    pub reps: RepCount,
}

/// What a stall costs, and how the ground is re-covered.
///
/// The drop and the increment are chosen as a pair so both land on the plate
/// grid and both cost the same four weeks — so a stall has a fixed price
/// whichever reset is in play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResetProtocol {
    /// Negative. Taken from the failed load, never from the anchor.
    pub drop: Percentage,
    pub reclimb_per_week: Kg,
}

/// The primary's back-off sets, for one session role.
///
/// **Its own numbers, per role.** These used to be read off the strength
/// block's [`AccessoryScheme`] on the grounds that the primary is a strength
/// slot and nobody had stated otherwise — which issued the light session's
/// three sets of six on the heavy day. The operator stated it on 2026-08-20:
/// heavy is `1 @ x, 2 × 4`, light is `3 @ x, 3 × 6`, and the record agrees on
/// every session since the July test.
///
/// The percentage lives here rather than beside it because that is how the two
/// patterns were stated — as patterns, each complete. Both are 85% today and
/// nothing requires them to stay equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackOff {
    pub sets: RepCount,
    pub reps: RepCount,
    /// Of this session's own top set, never of the anchor.
    pub of_top_set: Percentage,
}

/// The double-progression scheme one block's slots run.
///
/// Work the range, and when the top of it is reached at every working set, add an
/// increment and start again at the bottom.
///
/// **One scheme per block, not one per slot and not one for everything.** The
/// slots within a block are prescribed alike — every non-primary strength slot
/// shares a scheme, and so does every hypertrophy slot — while the two blocks
/// differ from each other. A per-slot scheme is a larger authored surface that
/// nothing yet needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessoryScheme {
    /// The rep target, as a target. **Not two loose bounds**: a `low` and a
    /// `high` beside each other can be written down inverted, and the check that
    /// would catch it belongs in the type rather than at every call site
    /// (§ 24). See [`Target`].
    pub reps: Target<RepCount>,
    pub sets: RepCount,
}

/// The whole of it, in force for every prescription this build issues.
///
/// **Held together rather than reached for one at a time** because it is one
/// statement about how the operator trains, and a derivation that took the ramp
/// from here and the back-offs from somewhere else would be describing two
/// programmes at once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Programming {
    /// The ramp before a top set. A floor: a ramp toward a high repetition count
    /// takes more repetitions than this, which [`warmup_ramp`] works out.
    ///
    /// [`warmup_ramp`]: super::warmup_ramp
    pub warmup: NonEmpty<WarmupStep>,
    /// The primary's back-off sets. Per role, because the two roles differ.
    pub back_off: PerRole<BackOff>,
    pub top_set_reps: PerRole<TopSetReps>,
    /// Every non-primary strength slot.
    pub strength: AccessoryScheme,
    /// Every hypertrophy slot.
    pub hypertrophy: AccessoryScheme,
    pub first_reset: ResetProtocol,
    pub second_reset: ResetProtocol,
}

/// A value in this file that will not build.
///
/// Every one of them is written a few lines above, so this is a defect in the
/// build rather than anything an operator did — but panicking is forbidden and
/// so is pretending it cannot happen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "the {what} this build programmes to will not build ({detail}) — this is a defect in this build"
)]
pub struct InvalidProgramming {
    what: String,
    detail: String,
}

fn wrong(what: &str, error: impl fmt::Display) -> InvalidProgramming {
    InvalidProgramming {
        what: what.to_owned(),
        detail: error.to_string(),
    }
}

fn percentage(what: &str, stated: &str) -> Result<Percentage, InvalidProgramming> {
    Percentage::try_from(stated.to_owned()).map_err(|error| wrong(what, error))
}

fn mass(what: &str, kilos: &str) -> Result<Kg, InvalidProgramming> {
    Kg::try_from(kilos.to_owned()).map_err(|error| wrong(what, error))
}

fn count(what: &str, reps: u32) -> Result<RepCount, InvalidProgramming> {
    RepCount::new(reps).map_err(|error| wrong(what, error))
}

/// The double-progression scheme a block's non-primary slots run.
fn scheme(what: &str, reps: (u32, u32), sets: u32) -> Result<AccessoryScheme, InvalidProgramming> {
    Ok(AccessoryScheme {
        reps: Target::between(count(what, reps.0)?, count(what, reps.1)?)
            .ok_or_else(|| wrong(what, "a rep range runs low-high and must span"))?,
        sets: count(what, sets)?,
    })
}

/// How the operator trains.
///
/// # Errors
///
/// [`InvalidProgramming`] if a value written here does not build, which is a
/// defect in this build rather than anything the operator can correct.
pub fn programming() -> Result<Programming, InvalidProgramming> {
    // The operator's own ramp: 4 at 40%, 3 at 60%, 2 at 80%, 1 at 90%, all of
    // the top set rather than of the anchor. It is a floor rather than the
    // answer — a work-up toward an eight-repetition maximum runs 8, 8, 6, 5 —
    // and `warmup::ramp` is where that is applied.
    let mut warmup = Vec::with_capacity(4);
    for (of_top_set, reps) in [("40%", 4), ("60%", 3), ("80%", 2), ("90%", 1)] {
        warmup.push(WarmupStep {
            of_top_set: percentage("warm-up ramp", of_top_set)?,
            reps: count("warm-up ramp", reps)?,
        });
    }
    let warmup = NonEmpty::new(warmup).map_err(|_| wrong("warm-up ramp", "a ramp needs a step"))?;

    Ok(Programming {
        warmup,

        // **The primary's back-off sets, per session role — its own pattern,
        // not the strength block's accessory scheme.** They used to be read off
        // the accessory scheme on the grounds that the primary is a strength
        // slot and nobody had stated otherwise, which issued the light
        // session's three sets of six on the heavy day. Stated by the operator
        // on 2026-08-20; the record agrees on every session since the July
        // test.
        back_off: PerRole {
            heavy: BackOff {
                sets: count("heavy back-off", 2)?,
                reps: count("heavy back-off", 4)?,
                of_top_set: percentage("heavy back-off", "85%")?,
            },
            light: BackOff {
                sets: count("light back-off", 3)?,
                reps: count("light back-off", 6)?,
                of_top_set: percentage("light back-off", "85%")?,
            },
        },

        // INFERRED. The primary's top set, per session role, read off every
        // session since the July test. Well evidenced — they have not varied
        // within a role — and still not stated.
        //
        // Constant within a block either way: descending reps across the block,
        // fives then threes then singles, is the textbook linear variant and is
        // deferred.
        top_set_reps: PerRole {
            light: TopSetReps::new(count("light top set", 3)?),
            heavy: TopSetReps::new(count("heavy top set", 1)?),
        },

        // INFERRED. The ranges were eyeballed from pull-ups at six, curls around
        // four to six and wrist work at six, and are unconfirmed. One scheme per
        // block rather than one per slot: the slots within a block are
        // prescribed alike, and the two blocks differ from each other.
        strength: scheme("strength scheme", (4, 6), 3)?,
        hypertrophy: scheme("hypertrophy scheme", (4, 6), 3)?,

        // From docs/primary-lift-progression.md. The drop and the increment are
        // chosen as a pair so both land on the plate grid and both cost four
        // weeks — so a stall has a fixed price whichever reset is in play.
        first_reset: ResetProtocol {
            drop: percentage("first reset", "-10%")?,
            reclimb_per_week: mass("first reset", "5")?,
        },
        second_reset: ResetProtocol {
            drop: percentage("second reset", "-5%")?,
            reclimb_per_week: mass("second reset", "2.5")?,
        },
    })
}

/// The numbers, pinned where they are written.
///
/// **A composed default is invisible to every other test.** The contract suites
/// and the store suites all take their shape from a fixture, so a value written
/// wrong here would prescribe wrong loads on a real machine and pass everywhere
/// else — the same class of fault as a base URL that already ended in `/v1`.
#[cfg(test)]
mod pinned {
    use super::programming;

    #[test]
    fn the_shipped_programming_builds() {
        let shipped = programming().expect("the shipped programming builds");

        assert_eq!(shipped.first_reset.drop.to_string(), "-10%");
        assert_eq!(shipped.first_reset.reclimb_per_week.to_string(), "5");
        assert_eq!(shipped.second_reset.drop.to_string(), "-5%");
        assert_eq!(shipped.second_reset.reclimb_per_week.to_string(), "2.5");

        let ramp: Vec<_> = shipped
            .warmup
            .iter()
            .map(|step| (step.of_top_set.to_string(), step.reps.as_u32()))
            .collect();
        assert_eq!(
            ramp,
            vec![
                ("40%".to_owned(), 4),
                ("60%".to_owned(), 3),
                ("80%".to_owned(), 2),
                ("90%".to_owned(), 1),
            ]
        );

        assert_eq!(shipped.back_off.heavy.sets.as_u32(), 2);
        assert_eq!(shipped.back_off.heavy.reps.as_u32(), 4);
        assert_eq!(shipped.top_set_reps.heavy.as_rep_count().as_u32(), 1);
        assert_eq!(shipped.back_off.light.sets.as_u32(), 3);
        assert_eq!(shipped.back_off.light.reps.as_u32(), 6);
        assert_eq!(shipped.top_set_reps.light.as_rep_count().as_u32(), 3);
    }
}
