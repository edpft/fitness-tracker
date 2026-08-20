//! What a repetition maximum is worth, as a share of the one-rep maximum.
//!
//! The RPE/RIR grid published by Reactive Training Systems reduces exactly to
//! one expression, which is what makes it code rather than a table anyone has to
//! maintain:
//!
//! ```text
//! %1RM = 100 − 2.5 × (reps − 1) − 5 × RIR
//! ```
//!
//! **Only the `RIR = 0` column lives here**, because that column is the one that
//! means something on its own: a set of `n` repetitions at zero in reserve *is*
//! an `n`-rep maximum, so the expression converts between the two units the
//! prescribed side speaks — a repetition count and a share of a maximum. See
//! research D10.
//!
//! **The rest of the grid is not modelled, and the primary lift's block contains
//! no repetitions in reserve at all.** The operator settled that on 2026-08-18:
//! a percentage-based plan states percentages, and a plan that reaches a
//! percentage by subtracting a number of repetitions in reserve from a maximum
//! has an RIR parameter in it however the arithmetic is presented. What the block
//! uses from here is the reps axis — [`PER_REPETITION`], the cost of one more
//! repetition — and the `RIR = 0` line itself, which is what a repetition maximum
//! *is*. `RIR` as an *observation* is retained on performed sets and feeds no
//! derivation (`docs/primary-lift-progression.md`).
//!
//! **This is a rounded presentation of real data, not a measurement.** Every
//! extra repetition costs 2.5 points and the underlying RTS data is not
//! perfectly linear. It is accurate enough to prescribe from and should not be
//! mistaken for physiology.

use crate::gym::RepCount;

use super::parameters::Percentage;

/// What one extra repetition costs, in basis points.
///
/// The table's slope along the repetitions axis, and the only slope the block
/// uses. The grid's other coefficient — five points per repetition in reserve —
/// is deliberately absent: it had a constant here until 2026-08-18, and the
/// constant was how RIR got into a percentage-based plan.
pub const PER_REPETITION: i32 = 250;

/// The share of a one-rep maximum that an `n`-rep maximum represents.
///
/// A one-rep maximum is the whole. Returns `None` for a repetition count so high
/// the expression leaves the axis — 41 repetitions and beyond, where the line
/// reaches zero. That is not a limit worth apologising for: the grid is
/// published to 12 and the block ladders here run to single figures.
#[must_use]
pub fn rep_max(reps: RepCount) -> Option<Percentage> {
    let steps = i32::try_from(reps.as_u32().checked_sub(1)?).ok()?;
    let points = Percentage::WHOLE
        .as_basis_points()
        .checked_sub(PER_REPETITION.checked_mul(steps)?)?;
    if points <= 0 {
        return None;
    }
    Percentage::from_basis_points(points).ok()
}
