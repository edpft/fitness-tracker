//! Stronger By Science's two-day intermediate squat routine, transcribed.
//!
//! **A published chart, not a derivation** (decision 0024). Every other
//! programme in this crate computes its loads: [`linear`](super::linear) from a
//! rate, [`block`](super::block) from a chart of Prilepin's. This one is read
//! off a table somebody else published, and the operator settled that on
//! 2026-09-01 — *"I want to lean more heavily on published programmes instead of
//! trying to build our own."*
//!
//! That is not the reversal it looks like. What 0023 recorded him rejecting was
//! copying **his old gym's record**, which is history rather than intent. SBS is
//! a published programme with named authors, a rationale document and a
//! spreadsheet. Adopting one is a different act from reverse-engineering the
//! other.
//!
//! ```text
//! week 1   5×5 @ 80%          1×8 @ 8RM, then 3×5–6 @ 8RM
//! week 2   4×3 @ 85%          1×5 @ 5RM, then 3×3–4 @ 5RM
//! week 3   3×1 @ 90%          1×3 @ 3RM, then 3×1–2 @ 3RM
//! week 4   3×3 @ 75%          1×1 @ 1RM
//! ```
//!
//! **The operator runs the front squat on both days.** The published routine
//! puts a back squat on day 1 and a front squat on day 2; which lift a
//! programme trains is the programme's business and not this chart's, so
//! nothing here names one.
//!
//! **Week 4 day 1 is his, and it is a transposition rather than an invention.**
//! The published *intermediate* week 4 goes straight to the test. He took the
//! *beginner* sheet's `3×3 @ 70%` — five points below its own week 1 of 75% —
//! and moved it to sit five points below the intermediate's 80%. The taper is
//! the published one; only the reference point changed.
//!
//! **The maximum moves inside the cycle, and that is the whole mechanism.**
//! Every percentage is a share of the maximum *current that week*, and each
//! repetition-maximum day resets it through [`training_max_share`]. So week 4's
//! 75% may be heavier in kilograms than week 1's 80%, and is meant to be.
//!
//! `anchor.rs` says an anchor "is fixed for the block's duration" and that
//! "nothing performed moves it". **That is true of a block and is not a rule
//! about programmes.** The reason it gives — that a value climbing indefinitely
//! leaves a block with no endpoint to be the plan for — does not reach here: an
//! SBS cycle's terminus is a scheduled test on a known week, not a number.

use crate::{
    gym::{Kg, RepCount},
    prescription::{parameters::Percentage, target::Target},
};

/// What one day of the chart prescribes.
///
/// **Three cases, and the middle one is why this is not a percentage
/// programme.** A repetition-maximum day states no load at all: the operator
/// works up to the heaviest set of `reps` he can manage, and the back-offs are
/// at whatever that turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SbsDay {
    /// Sets across at a share of the maximum current this week.
    Percentage {
        sets: RepCount,
        reps: RepCount,
        share: Percentage,
    },
    /// Work up to a repetition maximum, then back off at that load.
    ///
    /// **A count and no load, because the load is discovered.** Decision 0023
    /// reached this shape for the periodised block and decision 0025 for the
    /// cycling FTP test; all three are the same move, and none was written for
    /// the others.
    RepMax {
        /// The set that finds the maximum.
        reps: RepCount,
        /// How many sets follow it, at the same load.
        back_off_sets: RepCount,
        /// What each back-off is taken for — a range in the chart.
        back_off_reps: Target<RepCount>,
    },
    /// The test that closes the cycle and anchors the next.
    Test { reps: RepCount },
}

impl SbsDay {
    /// Whether this day establishes a maximum the next week programmes from.
    #[must_use]
    pub const fn sets_the_maximum(&self) -> Option<RepCount> {
        match self {
            Self::RepMax { reps, .. } | Self::Test { reps } => Some(*reps),
            Self::Percentage { .. } => None,
        }
    }
}

/// A day's position in the week. The chart runs two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SbsSession {
    /// The percentage day. Monday, on the operator's schedule.
    First,
    /// The repetition-maximum day, and in week 4 the test. Friday, which
    /// `programme_weekday` already records as his heavy day.
    Second,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidSbs {
    #[error("the chart runs four weeks, so week {week} is not one of them")]
    NoSuchWeek { week: u32 },
    #[error("a value in the shipped chart will not build — this is a defect in this build")]
    Unbuildable,
}

/// How many weeks one cycle of the chart runs.
pub const WEEKS: u32 = 4;

fn count(reps: u32) -> Result<RepCount, InvalidSbs> {
    RepCount::new(reps).map_err(|_| InvalidSbs::Unbuildable)
}

fn share(basis_points: i32) -> Result<Percentage, InvalidSbs> {
    Percentage::from_basis_points(basis_points).map_err(|_| InvalidSbs::Unbuildable)
}

/// What the chart prescribes in a given week and session.
///
/// Verified against `Squat 2x Int` in the operator's copy of the workbook, and
/// recorded in decision 0024.
///
/// # Errors
///
/// [`InvalidSbs::NoSuchWeek`] outside weeks one to four.
pub fn day(week: u32, session: SbsSession) -> Result<SbsDay, InvalidSbs> {
    let percentage = |sets: u32, reps: u32, points: i32| -> Result<SbsDay, InvalidSbs> {
        Ok(SbsDay::Percentage {
            sets: count(sets)?,
            reps: count(reps)?,
            share: share(points)?,
        })
    };
    let rep_max = |reps: u32, sets: u32, low: u32, high: u32| -> Result<SbsDay, InvalidSbs> {
        let minimum = count(low)?;
        let back_off_reps =
            Target::between(minimum, count(high)?).ok_or(InvalidSbs::Unbuildable)?;
        Ok(SbsDay::RepMax {
            reps: count(reps)?,
            back_off_sets: count(sets)?,
            back_off_reps,
        })
    };

    match (week, session) {
        (1, SbsSession::First) => percentage(5, 5, 8_000),
        (1, SbsSession::Second) => rep_max(8, 3, 5, 6),
        (2, SbsSession::First) => percentage(4, 3, 8_500),
        (2, SbsSession::Second) => rep_max(5, 3, 3, 4),
        (3, SbsSession::First) => percentage(3, 1, 9_000),
        (3, SbsSession::Second) => rep_max(3, 3, 1, 2),
        // The operator's transposition of the beginner sheet's taper.
        (4, SbsSession::First) => percentage(3, 3, 7_500),
        (4, SbsSession::Second) => Ok(SbsDay::Test { reps: count(1)? }),
        (week, _) => Err(InvalidSbs::NoSuchWeek { week }),
    }
}

/// What a repetition maximum is worth, as a share of the training maximum to
/// programme from next week.
///
/// **SBS's own table, and deliberately not [`rep_max`](super::repmax::rep_max).**
/// The two answer different questions and disagree by up to five points:
///
/// ```text
///        SBS      repmax.rs (RTS)
/// 8RM    80%      82.5%
/// 5RM    85%      90%
/// 3RM    90%      95%
/// ```
///
/// `repmax.rs` estimates a one-rep maximum from what was lifted. This decides
/// what to programme from next week, and **its generosity is the mechanism** —
/// it is how the bar goes up weekly without a test. A 100 kg triple implies a
/// 105.3 kg maximum under the domain's table and a 111.1 kg one under this.
///
/// Settled by the operator on 2026-09-01: *"yes, keep the SBS rep max
/// separate."* The workbook confirms there is no conservative discount hiding
/// anywhere — its `Maxes` sheet holds one number labelled `MAX`, applied
/// undiscounted.
///
/// `None` for a repetition count the published table does not name. It states
/// three and this does not extrapolate: a fourth row would be ours, not SBS's.
#[must_use]
pub fn training_max_share(reps: RepCount) -> Option<Percentage> {
    let points = match reps.as_u32() {
        1 => 10_000,
        3 => 9_000,
        5 => 8_500,
        8 => 8_000,
        _ => return None,
    };
    Percentage::from_basis_points(points).ok()
}

/// The training maximum to programme from next week, given what was lifted.
///
/// **Raised to the increment, and this departs from the programme notes.** They
/// say to round the inference down:
///
/// > "if you have a training max of 300 going into the week, and you get 235 for
/// > a 10RM, increase your max to 235/.75=313. When it's between two 5-pound
/// > increments, round down. So in this case, you'd use 310."
///
/// Their example works only because 300 is a round number. The operator's
/// counterexample, 2026-09-04: *"295 × 0.75 = 221.25, which rounds down to 220.
/// 220 ÷ 0.75 = 293.33, which rounds down to 290."* Five pounds a week, lost on
/// a **perfect** execution.
///
/// The maximum makes a round trip through the plate grid every repetition-maximum
/// day — down through [`working_load`] to a load, back up through here to a
/// maximum — and rounding down at both ends loses at both ends, amplified by the
/// division. Flooring both, this chart ran the operator from 92.5kg to 70kg over
/// the autumn's three cycles while he did exactly what it asked.
///
/// **So the two roundings are deliberately opposite, because they are different
/// acts.** [`working_load`] floors, because it names a load to put on the bar and
/// must never ask for more than has been shown. This raises, because it infers a
/// maximum from a load that was *already floored* — rounding down again would
/// compound the instruction's own rounding into the model.
///
/// A cycle where every target is met now holds the maximum where it was; one
/// where a target is beaten raises it; one where a target is missed lowers it.
/// None of those was true before.
///
/// `None` where the repetition count is not one the table names, or where the
/// arithmetic will not fit.
#[must_use]
pub fn advance(achieved: Kg, reps: RepCount, increment: Kg) -> Option<Kg> {
    let share = training_max_share(reps)?;
    let points = i64::from(share.as_basis_points());
    if points <= 0 {
        return None;
    }
    let grams = i64::try_from(achieved.as_grams()).ok()?;
    let raw = grams.checked_mul(10_000)?.checked_div(points)?;
    let step = i64::try_from(increment.as_grams()).ok()?;
    if step <= 0 {
        return None;
    }
    let remainder = raw.rem_euclid(step);
    let raised = if remainder == 0 {
        raw
    } else {
        raw.checked_add(step.checked_sub(remainder)?)?
    };
    u64::try_from(raised).ok().map(Kg::from_grams)
}

/// The load a percentage day calls for, floored to the increment.
///
/// **Floored, and never rounded up.** The workbook floors, and the difference is
/// not cosmetic: rounding up would prescribe a load the operator has not shown he
/// can hold, every week, compounding. [`advance`] rounds the other way on
/// purpose, and its documentation says why.
#[must_use]
pub fn working_load(maximum: Kg, share: Percentage, increment: Kg) -> Option<Kg> {
    let raw = i64::try_from(share.of(maximum).as_grams()).ok()?;
    let step = i64::try_from(increment.as_grams()).ok()?;
    if step <= 0 {
        return None;
    }
    let floored = raw - raw.rem_euclid(step);
    u64::try_from(floored).ok().map(Kg::from_grams)
}

/// The maximum a cycle is programming from, after the repetition-maximum days it
/// has already run.
///
/// **Where the chart's mechanism actually lives.** Each entry is a week and what
/// was lifted on that week's second session; the chart says what repetition
/// count that session was taken at, and [`training_max_share`] turns the result
/// into what the next week programmes from. Applying them in order is the whole
/// of the progression.
///
/// Entries are applied in the order given, because each advance is a share of
/// the one before it. A week the chart does not name, or one whose session sets
/// no maximum, is skipped — a week nobody trained leaves the maximum where it
/// was, which is right rather than merely convenient.
///
/// **This is the reason SBS reads the record at all**, and it is the ordinary
/// case rather than a special one: a linear ladder reads it too. What differs is
/// what each asks. A ladder asks whether the top set was completed; this asks
/// what it weighed.
#[must_use]
pub fn maximum_after(opening: Kg, performed: &[(u32, Kg)], increment: Kg) -> Kg {
    let mut maximum = opening;
    for (week, achieved) in performed {
        let Ok(day) = day(*week, SbsSession::Second) else {
            continue;
        };
        let Some(reps) = day.sets_the_maximum() else {
            continue;
        };
        if let Some(next) = advance(*achieved, reps, increment) {
            maximum = next;
        }
    }
    maximum
}
