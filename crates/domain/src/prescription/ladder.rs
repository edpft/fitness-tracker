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
//! opening        = the entry test's failed load, or its completed load + climb
//! heavy(w)       = quantise(opening + climb × (w - 1))
//! ```
//!
//! **The opening is derived from the entry test, not authored.** A test that
//! found the ceiling failed a load, and that failed load is where the block
//! opens — the block gets back to it through the drop-and-re-climb protocol,
//! which is also what regulates every later failure. A test that failed nothing
//! did not find the ceiling, so the block opens one increment above what it did
//! reach. Either way nobody chooses a percentage. See
//! `docs/decisions/0009-a-linear-block-opens-from-its-entry-test.md`.
//!
//! **So the anchor seeds the ladder and then does nothing else.** Warm-ups and
//! back-offs are shares of their own session's top set, and the light session is
//! a share of the heavy one, so no load anywhere is a percentage of the anchor.
//! [`Ladder::implied_percentage`] divides one back out for reporting and is
//! consumed by nothing.
//!
//! **The climb is a rate, and the block has no authored endpoint.** An earlier
//! model authored the endpoint and derived the weekly step from it, on the
//! grounds that a step multiplied by a duration produces an endpoint nobody
//! chose. That argument assumed the plan is what regulates the climb. It is
//! not: the plan attempts a fixed increment every week, and what regulates it is
//! the drop-and-re-climb protocol in [`super::progression`] — which is why the
//! endpoint could be left unstated for as long as it was without anything
//! downstream noticing. See `docs/decisions/0008-the-linear-ladder-climbs-at-a-rate.md`.
//!
//! That is also what separates this template from `block`. Here duration says
//! how long the climb runs and nothing else, so an interrupted eight weeks is
//! the same plan as an uninterrupted twelve, stopped earlier. In `block`,
//! duration shapes the plan — it sets the rung count and the phase split — and a
//! different duration really is a different programme.
//!
//! **A week that repeats the previous week's load is a legitimate plan, not a
//! defect.** It cannot happen at a climb of one plate or more, but a smaller
//! authored rate quantises two positions onto one bar. The load sequence still
//! only rises, which is all the plan promises. Nothing here tries to spread the
//! steps out to avoid it.
//!
//! **This module holds the plan and not the response to it failing.** A stall
//! suspends the ladder and re-climbs from the failed load, and that is a
//! separate mechanism that never touches the anchor: it lives in
//! [`super::progression`]. Conflating the two is a mistake the model has already
//! made once, which is why the two are two modules and not two halves of this
//! one. The two rates are deliberately alike in kind — `climb_per_week` here and
//! `reclimb_per_week` there — because a reset is the same climb run at a
//! different rate off a lower start.

use crate::gym::Kg;

use super::{
    anchor::Anchor,
    parameters::{Percentage, PlateIncrement},
    quantise::quantise_loaded,
    schedule::WeekIndex,
};

/// Where a block opens, from the test that anchors it.
///
/// **The failed load, if the test found the ceiling.** The block does not open
/// *below* it and work up as a plan — it opens there, and the drop-and-re-climb
/// protocol is how it gets back, which is [`super::progression`]'s job rather
/// than this one's.
///
/// **Otherwise one climb above what was completed.** A test that failed nothing
/// did not find a ceiling, so its completed load is a floor and the block starts
/// by beating it.
fn opening(anchor: Anchor, climb_per_week: Kg, increment: PlateIncrement) -> Kg {
    let load = anchor.failed().unwrap_or_else(|| {
        Kg::from_grams(
            anchor
                .load()
                .as_grams()
                .saturating_add(climb_per_week.as_grams()),
        )
    });
    quantise_loaded(load, increment)
}

/// Why a ladder could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidLadder {
    #[error("a block needs at least one climbing week besides its test")]
    NoClimbingWeeks,
    #[error("a ladder that does not rise is not a plan")]
    DoesNotRise,
}

/// A block's plan: where it opens, and what it adds each week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ladder {
    opening: Kg,
    climb_per_week: Kg,
    climbing_weeks: u32,
}

impl Ladder {
    /// Build from an entry test, a weekly climb and a block duration.
    ///
    /// `duration_weeks` counts the exit test, so the climbing weeks are one
    /// fewer. The entry test is not one of them: it is the week before the block
    /// and is what the anchor records.
    ///
    /// # Errors
    ///
    /// [`InvalidLadder::NoClimbingWeeks`] if the block is too short to climb at
    /// all, and [`InvalidLadder::DoesNotRise`] if the climb is nothing.
    pub fn new(
        anchor: Anchor,
        climb_per_week: Kg,
        duration_weeks: u32,
        increment: PlateIncrement,
    ) -> Result<Self, InvalidLadder> {
        if duration_weeks < 2 {
            return Err(InvalidLadder::NoClimbingWeeks);
        }
        if climb_per_week.as_grams() == 0 {
            return Err(InvalidLadder::DoesNotRise);
        }
        Ok(Self {
            opening: opening(anchor, climb_per_week, increment),
            climb_per_week,
            climbing_weeks: duration_weeks - 1,
        })
    }

    pub const fn climbing_weeks(self) -> u32 {
        self.climbing_weeks
    }

    /// The load the first climbing week asks for.
    pub const fn opening(self) -> Kg {
        self.opening
    }

    pub const fn climb_per_week(self) -> Kg {
        self.climb_per_week
    }

    /// The heavy session's top set for a climbing week.
    ///
    /// `None` for the block's test week, which is not a ladder position and has
    /// no load — the type says so, so a caller cannot ask for a load that does
    /// not exist.
    #[must_use]
    pub fn heavy_top_set(self, week: WeekIndex, increment: PlateIncrement) -> Option<Kg> {
        let offset = week.as_offset();
        if offset >= self.climbing_weeks {
            return None;
        }

        let climbed = u64::from(offset).checked_mul(self.climb_per_week.as_grams())?;
        let grams = self.opening.as_grams().checked_add(climbed)?;

        Some(quantise_loaded(Kg::from_grams(grams), increment))
    }

    /// The light session's top set: a proportion of that week's heavy one.
    ///
    /// Derived from the heavy load rather than from the anchor, so the two roles
    /// move together by construction and one ladder serves both.
    #[must_use]
    pub fn light_top_set(
        self,
        week: WeekIndex,
        increment: PlateIncrement,
        light_of_heavy: Percentage,
    ) -> Option<Kg> {
        self.heavy_top_set(week, increment)
            .map(|heavy| quantise_loaded(light_of_heavy.of(heavy), increment))
    }

    /// Where a climbing week sits relative to the anchor, for reporting.
    ///
    /// **A reading of the plan, not the plan.** The load is what is prescribed;
    /// this divides it back out so an operator can see the climb pass 100% of
    /// the max it started from. Nothing derives a load from it.
    ///
    /// `None` for the test week, and for an anchor of nothing — which is not a
    /// block anyone is running, but is a division.
    #[must_use]
    pub fn implied_percentage(
        self,
        anchor: Kg,
        week: WeekIndex,
        increment: PlateIncrement,
    ) -> Option<Percentage> {
        let load = self.heavy_top_set(week, increment)?;
        let whole = i64::from(Percentage::WHOLE.as_basis_points());

        let points = i64::try_from(load.as_grams())
            .ok()?
            .checked_mul(whole)?
            .checked_div(i64::try_from(anchor.as_grams()).ok()?)?;

        Percentage::from_basis_points(i32::try_from(points).ok()?).ok()
    }
}
