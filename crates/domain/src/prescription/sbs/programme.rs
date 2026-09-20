//! An authored SBS programme: the chart, bound to a lift and a calendar.
//!
//! **Almost nothing is authored, and that is the point of a published
//! programme.** [`Linear`](crate::prescription::linear::Linear) takes a climb
//! rate and a duration; [`BlockPeriodisation`](crate::prescription::block::BlockPeriodisation)
//! takes a duration that shapes its phases. This takes neither, because the
//! chart states every set, every repetition and every percentage itself
//! ([`chart`](super::chart)). What an operator supplies is which lift, which
//! days, what the opening maximum is, and what fills the other slots.
//!
//! **The duration is not an input either.** A cycle is four weeks because the
//! chart is four weeks. Offering a duration would invite a five-week SBS cycle,
//! which is not a thing that exists.

use crate::{
    gym::exercise::Exercise,
    prescription::{
        linear::{Primary, PrimaryPattern, SlotFills},
        mesocycle::{InconsistentMesocycle, check_primary},
        schedule::Calendar,
    },
    schedule::{Relative, SessionRole},
};

use super::chart::WEEKS;

/// The session whose result moves the maximum.
///
/// **Not an input, because the chart already says.** Every other climbing
/// programme asks which session advances it, because the answer is genuinely the
/// operator's. Here the second session of every week is the repetition-maximum
/// day — and in week 4 the test — so the gating session is decided by the chart
/// and asking would be asking for a number already stated (decisions 0019 and
/// 0020). Which *weekday* that falls on is the calendar's business, and the
/// operator's schedule already records Friday as his heavy day.
pub const GATING: SessionRole = SessionRole::new(Relative::Higher, Relative::Lower);

/// A cycle of the SBS chart, as authored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sbs {
    primary: Primary,
    fills: SlotFills,
    calendar: Calendar,
}

impl Sbs {
    /// Build, running the checks the type system cannot.
    ///
    /// # Errors
    ///
    /// [`InconsistentMesocycle`] for a gating role the programme never runs, a
    /// primary not counted in repetitions, a primary exercise that does not fill
    /// the slot named as primary, or a calendar that is not four weeks.
    pub fn new(
        pattern: PrimaryPattern,
        exercise: Exercise,
        fills: SlotFills,
        calendar: Calendar,
    ) -> Result<Self, InconsistentMesocycle> {
        let primary = Primary::new(pattern, exercise, GATING);

        check_primary(pattern, exercise, &fills, GATING)?;

        // The chart is four weeks. A calendar of any other length is not this
        // programme run longer or shorter — it is a different programme, and
        // there is no rule here for what its extra weeks would prescribe.
        if calendar.duration_weeks() != WEEKS {
            return Err(InconsistentMesocycle::ChartIsFourWeeks {
                given: calendar.duration_weeks(),
            });
        }

        Ok(Self {
            primary,
            fills,
            calendar,
        })
    }

    /// Rebuild one that is already stored, keeping the time it was authored.
    ///
    /// The checks are not re-run: a stored programme passed them when it was
    /// written, and re-refusing it now would make a rule change unreadable data.
    #[must_use]
    pub const fn stored(
        pattern: PrimaryPattern,
        exercise: Exercise,
        fills: SlotFills,
        calendar: Calendar,
    ) -> Self {
        Self {
            primary: Primary::new(pattern, exercise, GATING),
            fills,
            calendar,
        }
    }

    pub const fn fills(&self) -> &SlotFills {
        &self.fills
    }

    pub const fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    pub const fn primary(&self) -> PrimaryPattern {
        self.primary.pattern()
    }

    pub const fn primary_exercise(&self) -> Exercise {
        self.primary.exercise()
    }

    /// The session whose result moves the maximum. Always [`GATING`].
    pub const fn gating_role(&self) -> SessionRole {
        GATING
    }
}
