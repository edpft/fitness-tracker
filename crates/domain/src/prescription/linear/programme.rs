//! A programme that climbs a top-set ladder: one of the two ways of periodising.
//!
//! **Its purpose is to increase the primary exercise's 1RM**, and its whole
//! primary loading series is a function of two authored values: a duration in
//! weeks and a starting 1RM. Everything else on this struct decides which
//! exercises fill which slots, not what the plan is.
//!
//! Fills are inputs rather than choices the programme makes. Generation produces
//! the loading series, never the exercise selection.
//!
//! **It was called `Programme` until 2026-08-22**, when a test became a
//! programme in its own right (decision 0013) and the name had to go to the
//! thing that is either. This is now [`Periodisation::Linear`], and it never
//! includes a test: every week it holds is a climbing week.
//!
//! [`Periodisation::Linear`]: crate::prescription::Periodisation::Linear

use jiff::Timestamp;

use crate::{
    gym::{Kg, exercise::Exercise},
    prescription::{
        anchor::{Anchor, Entry},
        ladder::{InvalidLadder, Ladder, Opening},
        parameters::GenerationParameters,
        programme::{InconsistentProgramme, check_primary},
        schedule::{Calendar, SessionRole, Weekdays},
        steps::LoadSteps,
        succession::{ProgrammeName, ProgrammeWindow},
    },
};

use super::template::{PrimaryPattern, SlotFills};

/// Where a block opens, from what the programme declares and what it anchors on.
///
/// One place, so the check `Linear::new` runs and the ladder `prescribe`
/// builds cannot disagree about which opening is in force.
fn opening_of(entry: Entry, parameters: &GenerationParameters) -> Opening {
    entry.declared_opening().map_or_else(
        || Opening::FromAnchor {
            anchor: entry.anchor(),
            drop: parameters.entry_drop,
        },
        Opening::Declared,
    )
}

fn steps_for(
    exercise: Exercise,
    parameters: &GenerationParameters,
) -> Result<&LoadSteps, InvalidLadder> {
    parameters
        .scales
        .for_exercise(exercise)
        .ok_or_else(|| InvalidLadder::NoScale {
            implement: exercise.implement().as_str(),
        })
}

/// What the programme trains, and which session decides its progression.
///
/// **One argument because they are one decision.** The pattern names a slot,
/// the exercise fills it, and the gating role says which session's top set the
/// ladder reads — and `Linear::check` already validates the three together,
/// because a primary that does not fill its own slot and a gate on a role the
/// programme never runs are the same kind of mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Primary {
    pattern: PrimaryPattern,
    exercise: Exercise,
    gating_role: SessionRole,
}

impl Primary {
    pub const fn new(
        pattern: PrimaryPattern,
        exercise: Exercise,
        gating_role: SessionRole,
    ) -> Self {
        Self {
            pattern,
            exercise,
            gating_role,
        }
    }

    pub const fn pattern(self) -> PrimaryPattern {
        self.pattern
    }

    pub const fn exercise(self) -> Exercise {
        self.exercise
    }

    pub const fn gating_role(self) -> SessionRole {
        self.gating_role
    }
}

/// A rule for generating a series of prescribed workouts, plus its inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Linear {
    /// What identifies this programme across re-authorings (decision 0012).
    /// Two programmes sharing a name are one programme's versions; two that do
    /// not are rivals for the days they cover, and may not overlap.
    name: ProgrammeName,
    primary: Primary,
    fills: SlotFills,
    /// The starting 1RM. Fixed for this block; only its exit test replaces it,
    /// and that replacement anchors the *next* block.
    /// The test that anchors the block, and the opening where the block states
    /// one rather than deriving it from that test.
    entry: Entry,
    calendar: Calendar,
    authored_at: Timestamp,
}

impl Linear {
    /// Build, running the three checks the type system cannot.
    ///
    /// # Errors
    ///
    /// [`InconsistentProgramme`] for a gating role the programme never runs, a
    /// primary that is not counted in repetitions, a primary exercise that does
    /// not fill the slot named as primary, or a climb and duration that do not
    /// make a ladder.
    pub fn new(
        name: ProgrammeName,
        primary: Primary,
        fills: SlotFills,
        entry: Entry,
        calendar: Calendar,
        parameters: &GenerationParameters,
    ) -> Result<Self, InconsistentProgramme> {
        Self::check(
            primary.pattern,
            primary.exercise,
            &fills,
            primary.gating_role,
            calendar.weekdays(),
        )?;

        // 4. The entry test precedes the block it anchors.
        //
        //    Not pedantry about dates: the test session is in the performed
        //    record, so a block that contains its own entry test reads that
        //    session as a gating one *and* opens re-climbing from it — the same
        //    failure counted twice, once as the block's opening and once as a
        //    miss inside it. Refusing here is what makes the opening derivation
        //    safe. See `docs/decisions/0009-a-linear-block-opens-from-its-entry-test.md`.
        if entry.anchor().from() >= calendar.start() {
            return Err(InconsistentProgramme::EntryTestIsNotBeforeTheBlock {
                start: calendar.start(),
                tested: entry.anchor().from(),
            });
        }

        // 5. And the climb has to make a ladder over this duration. Checked here
        //    so an unbuildable plan fails at authoring rather than at the first
        //    `prescribe`. Training weeks, not calendar ones: a block interrupted
        //    by a holiday is the same ladder run over a longer stretch of the
        //    year, not a longer ladder.
        Ladder::new(
            opening_of(entry, parameters),
            parameters.ladder_climb_per_week,
            calendar.duration_weeks(),
            steps_for(primary.exercise, parameters)?,
        )?;

        Ok(Self {
            name,
            primary,
            fills,
            entry,
            calendar,
            authored_at: Timestamp::now(),
        })
    }

    /// Rebuild a programme that was already authored.
    ///
    /// Runs the three checks that depend on nothing but the programme itself and
    /// **does not re-run the ladder check**.
    ///
    /// The ladder check asks whether a climb makes a ladder over a duration, and
    /// the climb belongs to the parameters rather than to the programme. Re-running
    /// it on read would therefore assert "this programme's duration works with the
    /// climb *currently* in force", which is not a property of the stored
    /// programme — the climb it was authored against may since have been
    /// superseded.
    ///
    /// It could not fail in any case: the two ways `Ladder::new` refuses are a
    /// duration below two and a climb of nothing, and the `programme` and
    /// `generation_parameters` tables both carry a `CHECK` excluding them. So this
    /// is about the check meaning the wrong thing rather than about it failing.
    /// Keeping it out also keeps `ProgrammeStore` able to answer its own question
    /// without another store's data, which is what lets a programme still be
    /// displayed when the parameters are the thing that is broken.
    ///
    /// A stored programme failing one of the three is corrupt rather than
    /// inconsistent, and the store reports it that way.
    ///
    /// # Errors
    ///
    /// [`InconsistentProgramme`] for any of the three parameter-independent
    /// checks.
    pub fn rehydrate(
        name: ProgrammeName,
        primary: Primary,
        fills: SlotFills,
        entry: Entry,
        calendar: Calendar,
        authored_at: Timestamp,
    ) -> Result<Self, InconsistentProgramme> {
        Self::check(
            primary.pattern,
            primary.exercise,
            &fills,
            primary.gating_role,
            calendar.weekdays(),
        )?;
        Ok(Self {
            name,
            primary,
            fills,
            entry,
            calendar,
            authored_at,
        })
    }

    /// The checks that need nothing but the programme.
    fn check(
        primary: PrimaryPattern,
        primary_exercise: Exercise,
        fills: &SlotFills,
        gating_role: SessionRole,
        weekdays: &Weekdays,
    ) -> Result<(), InconsistentProgramme> {
        // A programme gating on a role it never runs would never advance.
        if !weekdays.runs(gating_role) {
            return Err(InconsistentProgramme::GatingRoleNeverRuns {
                gating: gating_role,
            });
        }
        // The other two are the template's rather than this model's, and a block
        // asks them in the same words.
        check_primary(primary, primary_exercise, fills, gating_role)
    }

    pub const fn primary(&self) -> PrimaryPattern {
        self.primary.pattern
    }

    pub const fn primary_exercise(&self) -> Exercise {
        self.primary.exercise
    }

    pub const fn fills(&self) -> &SlotFills {
        &self.fills
    }

    pub const fn anchor(&self) -> Anchor {
        self.entry.anchor()
    }

    /// The entry test and the opening it may be overridden by, together.
    ///
    /// The pair rather than either half: `Periodisation` asks both models for
    /// this, and splitting it is the mistake `Entry` exists to prevent.
    pub const fn entry(&self) -> Entry {
        self.entry
    }

    pub const fn gating_role(&self) -> SessionRole {
        self.primary.gating_role
    }

    pub const fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    pub const fn authored_at(&self) -> Timestamp {
        self.authored_at
    }

    pub const fn name(&self) -> &ProgrammeName {
        &self.name
    }

    /// The days this programme occupies, for the rule that two programmes may
    /// not compete for one of them.
    ///
    /// Calendar weeks rather than training weeks: a block interrupted for a
    /// fortnight still occupies those days, and a programme starting inside
    /// them would be answering for the same dates.
    #[must_use]
    pub fn window(&self) -> ProgrammeWindow {
        ProgrammeWindow::new(
            self.name.clone(),
            self.calendar.start(),
            self.calendar.calendar_weeks(),
        )
    }

    /// The block's plan.
    ///
    /// Rebuilt from the parameters in force rather than stored, so there is one
    /// place the climb becomes a ladder. `Linear::new` has already proved this
    /// succeeds for the duration it holds.
    ///
    /// # Errors
    ///
    /// [`InvalidLadder`] only if the parameters handed in differ from the ones
    /// the programme was authored against.
    pub fn ladder(&self, parameters: &GenerationParameters) -> Result<Ladder, InvalidLadder> {
        Ladder::new(
            self.opening(parameters),
            parameters.ladder_climb_per_week,
            self.calendar.duration_weeks(),
            self.steps(parameters)?,
        )
    }

    /// Where this block's ladder opens — declared if it was, derived otherwise.
    #[must_use]
    pub fn opening(&self, parameters: &GenerationParameters) -> Opening {
        opening_of(self.entry, parameters)
    }

    /// The opening as authored, if it was authored at all. For reporting.
    #[must_use]
    pub const fn declared_opening(&self) -> Option<Kg> {
        self.entry.declared_opening()
    }

    /// The scale the primary is loaded on.
    ///
    /// # Errors
    ///
    /// [`InvalidLadder::NoScale`] where no scale has been authored for the
    /// primary's implement, which makes every load in the block underivable.
    pub fn steps<'a>(
        &self,
        parameters: &'a GenerationParameters,
    ) -> Result<&'a LoadSteps, InvalidLadder> {
        steps_for(self.primary.exercise, parameters)
    }

    /// Whether this slot is the primary one.
    ///
    /// The whole of what "primary" earns is decided by asking this: a warm-up
    /// ramp, a top set from the ladder, and back-offs. Every other slot reads its
    /// own history instead.
    pub fn is_primary(&self, slot: crate::prescription::shape::SlotId) -> bool {
        self.primary.pattern.slot() == slot
    }
}
