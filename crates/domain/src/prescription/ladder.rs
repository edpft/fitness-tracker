//! The plan.
//!
//! **A linear block: intensity ascends across the duration, and the block ends
//! in a test.** Given a number of weeks and a starting 1RM, this generates the
//! primary's whole loading series. That is the generator's entire job, stated in
//! `docs/primary-lift-progression.md`: a programme which, if performed, leaves
//! the tested 1RM at the end higher than it was at the start.
//!
//! ```text
//! climbing weeks = duration - 1          the last week is the test
//! step           = (end - start) / (climbing weeks - 1)
//! heavy(w)       = quantise(anchor × (start + step × (w - 1)))
//! ```
//!
//! **The endpoint is authored and the step is derived**, not the other way
//! round. An endpoint is a claim about how much can be gained in the time
//! available, which personal history and a reference programme can both inform.
//! A weekly step is a number with nothing behind it, and multiplying it by a
//! duration produces an endpoint nobody chose. It also makes duration
//! meaningful: the same endpoint over 8 or 12 weeks is two different plans,
//! where a fixed step over two durations is one plan run for different lengths.
//!
//! **A week that repeats the previous week's load is a legitimate plan, not a
//! defect.** Quantisation collapses two ladder positions onto one bar whenever
//! the derived step is smaller than the plate increment, and how often that
//! happens depends on the anchor, the span and the grid together — it is not a
//! property of any of them alone, and no combination is more correct than
//! another. The load sequence still only rises, which is all the plan promises.
//! Nothing here tries to spread the steps out to avoid it.
//!
//! **This module holds the plan and not the response to it failing.** A stall
//! suspends the ladder and re-climbs from the failed load, and that is a
//! separate mechanism that never touches the anchor: it lives in
//! [`super::progression`]. Conflating the two is a mistake the model has already
//! made once, which is why the two are two modules and not two halves of this
//! one.

use crate::gym::Kg;

use super::{
    parameters::{Percentage, PlateIncrement},
    quantise::quantise_loaded,
    schedule::WeekIndex,
};

/// Why a ladder could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidLadder {
    #[error("a block needs at least one climbing week besides its test")]
    NoClimbingWeeks,
    #[error("a ladder that does not rise is not a plan")]
    DoesNotRise,
}

/// A block's plan: a percentage of a fixed anchor per climbing week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ladder {
    start: Percentage,
    end: Percentage,
    climbing_weeks: u32,
}

impl Ladder {
    /// Build from an authored span and a block duration.
    ///
    /// `duration_weeks` counts the test, so the climbing weeks are one fewer.
    ///
    /// # Errors
    ///
    /// [`InvalidLadder::NoClimbingWeeks`] if the block is too short to climb at
    /// all, and [`InvalidLadder::DoesNotRise`] if the span does not ascend.
    pub const fn new(
        start: Percentage,
        end: Percentage,
        duration_weeks: u32,
    ) -> Result<Self, InvalidLadder> {
        if duration_weeks < 2 {
            return Err(InvalidLadder::NoClimbingWeeks);
        }
        if end.as_basis_points() <= start.as_basis_points() {
            return Err(InvalidLadder::DoesNotRise);
        }
        Ok(Self {
            start,
            end,
            climbing_weeks: duration_weeks - 1,
        })
    }

    pub const fn climbing_weeks(self) -> u32 {
        self.climbing_weeks
    }

    pub const fn start(self) -> Percentage {
        self.start
    }

    pub const fn end(self) -> Percentage {
        self.end
    }

    /// The percentage this climbing week sits at.
    ///
    /// A single-climbing-week block is degenerate rather than invalid: the
    /// ladder is one position, `start`, and there is no step to divide for.
    /// Returns `None` for a week past the block's climbing weeks.
    #[must_use]
    pub fn percentage(self, week: WeekIndex) -> Option<Percentage> {
        let offset = week.as_offset();
        if offset >= self.climbing_weeks {
            return None;
        }
        if self.climbing_weeks == 1 {
            return Some(self.start);
        }

        let span = i64::from(self.end.as_basis_points() - self.start.as_basis_points());
        let steps = i64::from(self.climbing_weeks - 1);
        // Multiply before dividing, so the rounding happens once and a
        // fractional step does not accumulate error across the block.
        let advanced = span
            .checked_mul(i64::from(offset))
            .and_then(|scaled| scaled.checked_div(steps))?;
        let points = i64::from(self.start.as_basis_points()).checked_add(advanced)?;

        // `start` is non-zero and the span is positive, so every position is
        // non-zero too. Going through the checked constructor anyway, because a
        // narrowing conversion that "cannot fail" is exactly the kind that does.
        Percentage::from_basis_points(i32::try_from(points).ok()?).ok()
    }

    /// The heavy session's top set for a climbing week.
    ///
    /// `None` for the block's test week, which is not a ladder position and has
    /// no percentage — the type says so, so a caller cannot ask for a load that
    /// does not exist.
    #[must_use]
    pub fn heavy_top_set(
        self,
        anchor: Kg,
        week: WeekIndex,
        increment: PlateIncrement,
    ) -> Option<Kg> {
        self.percentage(week)
            .map(|percentage| quantise_loaded(percentage.of(anchor), increment))
    }

    /// The light session's top set: a proportion of that week's heavy one.
    ///
    /// Derived from the heavy load rather than from the anchor, so the two roles
    /// move together by construction and one ladder serves both.
    #[must_use]
    pub fn light_top_set(
        self,
        anchor: Kg,
        week: WeekIndex,
        increment: PlateIncrement,
        light_of_heavy: Percentage,
    ) -> Option<Kg> {
        self.heavy_top_set(anchor, week, increment)
            .map(|heavy| quantise_loaded(light_of_heavy.of(heavy), increment))
    }
}
