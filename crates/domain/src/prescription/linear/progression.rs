//! Linear progression: how the primary lift climbs, and what a stall costs
//! (#192).
//!
//! **One of the two progressions the primary lift runs**, and it owns the
//! primary's sets, repetitions and loads: the ramp, the top sets, the back-offs,
//! the ladder's rate and opening, the light session's share of the heavy one, and
//! the two resets. Set when the programme is designed and kept as dated versions
//! (§ 14.1, amended 2026-10-26); a plan is generated from the version in force
//! when it was planned.

use std::fmt;

use crate::{
    measure::{Kg, RepCount},
    prescription::{ByIntensity, Percentage, warmup::WarmupStep},
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
/// section's scheme on the grounds that the primary is a strength slot and
/// nobody had stated otherwise — which issued the light session's three sets of
/// six on the heavy day. The operator stated it on 2026-08-20: heavy is
/// `1 @ x, 2 × 4`, light is `3 @ x, 3 × 6`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackOff {
    pub sets: RepCount,
    pub reps: RepCount,
    /// Of this session's own top set, never of the anchor.
    pub of_top_set: Percentage,
}

/// One version of linear progression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinearProgression {
    /// The ramp before a top set. A floor: a ramp toward a high repetition count
    /// takes more repetitions than this, which [`warmup_ramp`] works out.
    ///
    /// [`warmup_ramp`]: crate::prescription::warmup_ramp
    pub warmup: NonEmpty<WarmupStep>,
    pub top_set_reps: ByIntensity<TopSetReps>,
    /// The primary's back-off sets. Per role, because the two roles differ.
    pub back_off: ByIntensity<BackOff>,
    /// The light session's top set, as a proportion of that week's heavy one.
    /// Deriving it from the heavy load rather than from the anchor is what makes
    /// the two roles move together by construction.
    pub light_of_heavy: Percentage,
    /// What the plan adds each climbing week. There is no authored endpoint:
    /// the climb runs until the calendar stops it, and what regulates it is the
    /// reset protocol rather than a stated top. Same kind as
    /// [`ResetProtocol::reclimb_per_week`], because a reset is this climb run at
    /// a different rate off a lower start.
    ///
    /// **There is no opening percentage beside it.** Where the ladder opens is
    /// derived from the entry test the anchor records, not authored — see
    /// `docs/decisions/0009-a-linear-block-opens-from-its-entry-test.md`.
    pub ladder_climb_per_week: Kg,
    /// Negative. What a block's opening drops off the load its entry test
    /// failed, where the opening is derived rather than declared.
    ///
    /// **Stated, not borrowed from a reset.** It lands on the same −10% the
    /// first reset drops by, and it is stated separately anyway: the two agree
    /// today by decision rather than by derivation.
    pub entry_drop: Percentage,
    pub first_reset: ResetProtocol,
    pub second_reset: ResetProtocol,
}
