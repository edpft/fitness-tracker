//! A recovery mesocycle: the gym's half of the transition (#336).
//!
//! **The provider's recovery session, twice a week, and neither is
//! essential.** The operator, 2026-11-12: *"Mesocycle providers, like SBS and
//! Peak need to be able to provide recovery weeks … For SBS it's the session
//! before the test, because that's the easiest session in the programme."* And
//! on how many: *"We'd programme it as two sessions with the assumption that 1
//! may be dropped and we don't know which one."* So both sessions of a week are
//! the same session, whichever role the slot rides, and one of them done is the
//! week done.
//!
//! **What the session is, is the chart's**, through
//! [`SbsChart::recovery`](crate::prescription::SbsChart::recovery), and its
//! load is a share of the maximum the term's last test measured. Nothing here
//! names a day or a percentage, so a chart that changes keeps answering.
//!
//! **No test, no gating role, and no interruptions.** It climbs nothing, so
//! nothing gates; it measures nothing, so the next macrocycle opens with its
//! own entry test; and its weeks are the holiday's, which a closed gym does not
//! push into the next term.

use crate::{
    gym::exercise::Exercise,
    planner::{Relative, SessionRole},
    prescription::{
        linear::{PrimaryPattern, SlotFills},
        mesocycle::{InconsistentMesocycle, check_primary},
        schedule::Calendar,
    },
    provider::ExternalProgramme,
};

/// Both sessions of every week of it: the two roles a gym week rides.
const ROLES: [SessionRole; 2] = [
    SessionRole::new(Relative::Higher, Relative::Lower),
    SessionRole::new(Relative::Lower, Relative::Higher),
];

/// The gym's recovery mesocycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovery {
    pattern: PrimaryPattern,
    exercise: Exercise,
    fills: SlotFills,
    calendar: Calendar,
    /// Whose recovery session it runs: the programme in force when the term
    /// ended.
    programme: ExternalProgramme,
}

impl Recovery {
    /// Build, running the checks the type system cannot.
    ///
    /// # Errors
    ///
    /// [`InconsistentMesocycle`] for a primary not counted in repetitions, or
    /// one that does not fill the slot named as primary on either session.
    pub fn new(
        pattern: PrimaryPattern,
        exercise: Exercise,
        fills: SlotFills,
        calendar: Calendar,
        programme: ExternalProgramme,
    ) -> Result<Self, InconsistentMesocycle> {
        for role in ROLES {
            check_primary(pattern, exercise, &fills, role)?;
        }
        Ok(Self::stored(pattern, exercise, fills, calendar, programme))
    }

    /// Rebuild one that is already stored. The checks are not re-run.
    #[must_use]
    pub const fn stored(
        pattern: PrimaryPattern,
        exercise: Exercise,
        fills: SlotFills,
        calendar: Calendar,
        programme: ExternalProgramme,
    ) -> Self {
        Self {
            pattern,
            exercise,
            fills,
            calendar,
            programme,
        }
    }

    /// The same mesocycle on another calendar of the same length (#177).
    pub(crate) fn recalendared(&self, calendar: Calendar) -> Self {
        Self {
            calendar,
            ..self.clone()
        }
    }

    pub const fn primary(&self) -> PrimaryPattern {
        self.pattern
    }

    pub const fn primary_exercise(&self) -> Exercise {
        self.exercise
    }

    pub const fn fills(&self) -> &SlotFills {
        &self.fills
    }

    pub(crate) const fn fills_mut(&mut self) -> &mut SlotFills {
        &mut self.fills
    }

    pub const fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    pub const fn programme(&self) -> &ExternalProgramme {
        &self.programme
    }
}
