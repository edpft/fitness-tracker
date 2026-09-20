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
//! **It was called `Mesocycle` until 2026-08-22**, when a test became a
//! programme in its own right (decision 0013) and the name had to go to the
//! thing that is either. This is now [`Progression::Linear`], and it never
//! includes a test: every week it holds is a climbing week.
//!
//! [`Progression::Linear`]: crate::prescription::Progression::Linear

use crate::{
    gym::exercise::Exercise,
    prescription::{
        anchor::Anchor,
        ladder::{InvalidLadder, Ladder, Opening},
        mesocycle::{InconsistentMesocycle, check_primary},
        parameters::GenerationParameters,
        schedule::{Calendar, SessionRole, Weekdays},
        steps::LoadSteps,
    },
};

use super::template::{PrimaryPattern, SlotFills};

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
    primary: Primary,
    fills: SlotFills,
    calendar: Calendar,
}

impl Linear {
    /// Build, running the three checks the type system cannot.
    ///
    /// # Errors
    ///
    /// [`InconsistentMesocycle`] for a gating role the programme never runs, a
    /// primary that is not counted in repetitions, a primary exercise that does
    /// not fill the slot named as primary, or a climb and duration that do not
    /// make a ladder.
    pub fn new(
        primary: Primary,
        fills: SlotFills,
        calendar: Calendar,
        parameters: &GenerationParameters,
    ) -> Result<Self, InconsistentMesocycle> {
        Self::check(
            primary.pattern,
            primary.exercise,
            &fills,
            primary.gating_role,
            calendar.weekdays(),
        )?;

        // 4. And the climb has to make a ladder over this duration. Checked
        //    here so an unbuildable plan fails at authoring rather than at the
        //    first `prescribe`. Training weeks, not calendar ones: a block
        //    interrupted by a holiday is the same ladder run over a longer
        //    stretch of the year, not a longer ladder.
        //
        //    **The ladder itself is not built.** It opens from the maximum in
        //    force when a session is asked for, which is not knowable now — so
        //    what is checked is the part that depends on neither.
        steps_for(primary.exercise, parameters)?;
        Ladder::rises(parameters.ladder_climb_per_week, calendar.duration_weeks())?;

        Ok(Self {
            primary,
            fills,
            calendar,
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
    /// Keeping it out also keeps `MesocycleStore` able to answer its own question
    /// without another store's data, which is what lets a programme still be
    /// displayed when the parameters are the thing that is broken.
    ///
    /// A stored programme failing one of the three is corrupt rather than
    /// inconsistent, and the store reports it that way.
    ///
    /// # Errors
    ///
    /// [`InconsistentMesocycle`] for any of the three parameter-independent
    /// checks.
    pub fn rehydrate(
        primary: Primary,
        fills: SlotFills,
        calendar: Calendar,
    ) -> Result<Self, InconsistentMesocycle> {
        Self::check(
            primary.pattern,
            primary.exercise,
            &fills,
            primary.gating_role,
            calendar.weekdays(),
        )?;
        Ok(Self {
            primary,
            fills,
            calendar,
        })
    }

    /// The checks that need nothing but the programme.
    fn check(
        primary: PrimaryPattern,
        primary_exercise: Exercise,
        fills: &SlotFills,
        gating_role: SessionRole,
        weekdays: &Weekdays,
    ) -> Result<(), InconsistentMesocycle> {
        // A programme gating on a role it never runs would never advance.
        if !weekdays.runs(gating_role) {
            return Err(InconsistentMesocycle::GatingRoleNeverRuns {
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

    pub const fn gating_role(&self) -> SessionRole {
        self.primary.gating_role
    }

    pub const fn calendar(&self) -> &Calendar {
        &self.calendar
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
    pub fn ladder(
        &self,
        maximum: Anchor,
        parameters: &GenerationParameters,
    ) -> Result<Ladder, InvalidLadder> {
        Ladder::new(
            Opening {
                maximum,
                drop: parameters.entry_drop,
            },
            parameters.ladder_climb_per_week,
            self.calendar.duration_weeks(),
            self.steps(parameters)?,
        )
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
