//! How many repetitions each step of the ramp is taken for.
//!
//! **The stored ramp is a floor, not the answer** (decision 0030). Its
//! percentages are the ramp — 40, 60, 80, 90 of what the session is working
//! toward — and its repetition counts are what a ramp toward a *low* count runs.
//! Ramping toward a high one takes more, because a warm-up that never asks for
//! more than four repetitions has not rehearsed a set of eight.
//!
//! ```text
//! reps = max(floor, descent)      floor   = the stored ramp, 4, 3, 2, 1
//!                                 descent = n, n, n−2, n−3
//! ```
//!
//! Which resolves, for the four repetition maxima SBS runs:
//!
//! ```text
//! 8RM   8, 8, 6, 5      descent
//! 5RM   5, 5, 3, 2      descent
//! 3RM   4, 3, 2, 1      floor — the descent would reach zero
//! 1RM   4, 3, 2, 1      floor — the descent would go negative
//! ```
//!
//! **The floor is not a special case.** For `n ≥ 4` the descent is at or above
//! the stored ramp at every position, so an element-wise maximum and a branch on
//! `n` agree everywhere; the maximum is written because it is total without one.
//!
//! **Provenance.** Greg Everett publishes the shape for a five-repetition
//! maximum — *"1-2 sets of 5, then sets of 3 and 2 until getting to your 5 rep
//! weight"* — and the operator supplied the eight, `8, 8, 6, 5`. Both are
//! `n, n, n−2, n−3`, and neither was fitted to the other. No source states the
//! general rule: it is an interpolation through two points and should be
//! described that way. Beyond the fourth step the descent simply continues,
//! which is an extension of the rule rather than part of what was stated.
//!
//! **Only a maximal top set gets this.** A percentage day states a submaximal
//! load, so its ramp has nothing to rehearse and keeps the stored counts.

use crate::gym::{RepCount, sequence::NonEmpty};

use super::parameters::WarmupStep;

/// The ramp to run when working up to `top` repetitions.
///
/// `floor` is the stored ramp. Its percentages pass through untouched; only the
/// repetition counts move, and only upward.
#[must_use]
pub fn ramp(floor: &NonEmpty<WarmupStep>, top: RepCount) -> NonEmpty<WarmupStep> {
    floor.map_indexed(|index, step| WarmupStep {
        of_top_set: step.of_top_set,
        reps: descent(top, index).map_or(step.reps, |wanted| wanted.max(step.reps)),
    })
}

/// What the descent asks for at `index`, before the floor is applied.
///
/// The first two steps are taken for the full count and every step after that
/// drops one more — so the third is `n − 2` and the fourth `n − 3`, which is the
/// published shape. `None` once the subtraction leaves the axis, which is what
/// makes the floor take over for a low `top` rather than a branch on it.
fn descent(top: RepCount, index: usize) -> Option<RepCount> {
    if index < 2 {
        return Some(top);
    }
    let index = u32::try_from(index).ok()?;
    RepCount::new(top.as_u32().checked_sub(index)?).ok()
}
