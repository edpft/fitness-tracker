//! An authored SBS programme: the chart, bound to a lift and a calendar.
//!
//! **Almost nothing is authored, and that is the point of a published
//! programme.** [`Linear`](crate::prescription::linear::Linear) takes a climb
//! rate and a duration; [`Periodised`](crate::prescription::block::Periodised)
//! takes a duration that shapes its phases. This takes neither, because the
//! chart states every set, every repetition and every percentage itself
//! ([`chart`](super::chart)). What an operator supplies is which lift, which
//! days, what the opening maximum is, and what fills the other slots.
//!
//! **The duration is not an input either.** A cycle is four weeks because the
//! chart is four weeks. Offering a duration would invite a five-week SBS cycle,
//! which is not a thing that exists.

use jiff::Timestamp;

use crate::{
    gym::exercise::Exercise,
    prescription::{
        anchor::Entry,
        linear::{Primary, PrimaryPattern, SlotFills},
        programme::{InconsistentProgramme, check_primary},
        schedule::{Calendar, SessionRole},
        succession::{ProgrammeName, ProgrammeWindow},
    },
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
pub const GATING: SessionRole = SessionRole::Heavy;

/// A cycle of the SBS chart, as authored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sbs {
    /// What identifies this programme across re-authorings (decision 0012).
    name: ProgrammeName,
    primary: Primary,
    fills: SlotFills,
    /// The maximum week 1 programmes from.
    ///
    /// **The opening only.** Unlike every other programme here, this number does
    /// not stand for the whole cycle: each repetition-maximum day resets it
    /// through [`training_max_share`](super::chart::training_max_share), so what
    /// week 3 is a share of was established in week 2. The anchor is where the
    /// cycle *starts*, and is the last number in it that was not derived from a
    /// performance.
    entry: Entry,
    calendar: Calendar,
    authored_at: Timestamp,
}

impl Sbs {
    /// Build, running the checks the type system cannot.
    ///
    /// # Errors
    ///
    /// [`InconsistentProgramme`] for a gating role the programme never runs, a
    /// primary not counted in repetitions, a primary exercise that does not fill
    /// the slot named as primary, a calendar that is not four weeks, or a test
    /// that does not precede the cycle it anchors.
    pub fn new(
        name: ProgrammeName,
        pattern: PrimaryPattern,
        exercise: Exercise,
        fills: SlotFills,
        entry: Entry,
        calendar: Calendar,
    ) -> Result<Self, InconsistentProgramme> {
        let primary = Primary::new(pattern, exercise, GATING);

        // A cycle that never runs its gating session would never advance — and
        // here that is worse than a stalled ladder, because the gating day is
        // where the maximum is *set*. A cycle without one would prescribe every
        // week off the opening maximum for ever.
        if !calendar.weekdays().runs(GATING) {
            return Err(InconsistentProgramme::GatingRoleNeverRuns { gating: GATING });
        }
        check_primary(pattern, exercise, &fills, GATING)?;

        // The chart is four weeks. A calendar of any other length is not this
        // programme run longer or shorter — it is a different programme, and
        // there is no rule here for what its extra weeks would prescribe.
        if calendar.duration_weeks() != WEEKS {
            return Err(InconsistentProgramme::ChartIsFourWeeks {
                given: calendar.duration_weeks(),
            });
        }

        // The same rule the linear template applies, for the same reason: a
        // cycle containing the test that anchors it would read that session
        // twice, once as its own opening and once as work inside it.
        if entry.anchor().from() >= calendar.start() {
            return Err(InconsistentProgramme::EntryTestIsNotBeforeTheBlock {
                start: calendar.start(),
                tested: entry.anchor().from(),
            });
        }

        Ok(Self {
            name,
            primary,
            fills,
            entry,
            calendar,
            authored_at: Timestamp::now(),
        })
    }

    /// Rebuild one that is already stored, keeping the time it was authored.
    ///
    /// The checks are not re-run: a stored programme passed them when it was
    /// written, and re-refusing it now would make a rule change unreadable data.
    #[must_use]
    pub const fn stored(
        name: ProgrammeName,
        pattern: PrimaryPattern,
        exercise: Exercise,
        fills: SlotFills,
        entry: Entry,
        calendar: Calendar,
        authored_at: Timestamp,
    ) -> Self {
        Self {
            name,
            primary: Primary::new(pattern, exercise, GATING),
            fills,
            entry,
            calendar,
            authored_at,
        }
    }

    pub const fn name(&self) -> &ProgrammeName {
        &self.name
    }

    pub const fn fills(&self) -> &SlotFills {
        &self.fills
    }

    pub const fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    pub const fn authored_at(&self) -> Timestamp {
        self.authored_at
    }

    pub const fn entry(&self) -> Entry {
        self.entry
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

    /// The days this cycle occupies, for the rule that two programmes may not
    /// compete for one of them.
    #[must_use]
    pub fn window(&self) -> ProgrammeWindow {
        ProgrammeWindow::new(
            self.name.clone(),
            self.calendar.start(),
            self.calendar.calendar_weeks(),
        )
    }
}
