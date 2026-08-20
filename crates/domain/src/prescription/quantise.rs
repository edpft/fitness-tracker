//! Putting a derived load on the bar.
//!
//! **Nearest multiple of the increment; an exact tie resolves down.** Settled in
//! `specs/003-prescribed-workout-generation`, and it is one rule rather than a
//! property of any particular derivation.
//!
//! That generality is the point. Three derivations produce off-grid loads — a
//! back-off at 85% of a top set, a warm-up step at 40%, a reset drop at −10% —
//! and if quantisation lived on the back-off the other two would need their own
//! rules and the three could disagree. A −10% drop from 87.5 is 78.75, exactly
//! halfway between 77.5 and 80, and it is the case that made the generalisation
//! necessary rather than tidy.
//!
//! The increment is data (§ 14, a fact about the gym's plates); the rounding
//! direction is code (§ 9, a deterministic derivation). Nothing here reads a
//! parameter that could make it round the other way.

use crate::gym::Kg;

use super::parameters::PlateIncrement;

/// The nearest loadable weight, ties resolving down.
///
/// Integer throughout: no float touches a load on the way to a bar, for the
/// same reason [`Kg`] holds grams.
#[must_use]
pub const fn quantise(load: Kg, increment: PlateIncrement) -> Kg {
    let step = increment.as_kg().as_grams();
    let grams = load.as_grams();

    let below = grams / step * step;
    let remainder = grams - below;

    // `2 * remainder > step` rather than `>=`, which is the whole of "ties
    // resolve down": at exactly half a step the comparison is false and the
    // lower candidate stands.
    if remainder * 2 > step {
        Kg::from_grams(below + step)
    } else {
        Kg::from_grams(below)
    }
}

/// Quantise, and never return nothing.
///
/// A derived load below half an increment quantises to zero, which is a real
/// answer for an unloaded movement and a wrong one for a barbell. Callers
/// prescribing a loaded slot use this; callers prescribing mobility work do not
/// go through the quantiser at all.
///
/// Deliberately not folded into [`quantise`]: "round to the grid" and "never
/// prescribe an empty bar" are two rules, and one function doing both would
/// hide the second from anyone reading the first.
#[must_use]
pub const fn quantise_loaded(load: Kg, increment: PlateIncrement) -> Kg {
    let quantised = quantise(load, increment);
    if quantised.as_grams() == 0 {
        increment.as_kg()
    } else {
        quantised
    }
}
