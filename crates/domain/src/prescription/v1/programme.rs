//! A programme written against `v1`.
//!
//! **Its purpose is to increase the primary exercise's 1RM**, and its whole
//! primary loading series is a function of two authored values: a duration in
//! weeks and a starting 1RM. Everything else on this struct decides which
//! exercises fill which slots, not what the plan is.
//!
//! Fills are inputs rather than choices the programme makes. Generation produces
//! the loading series, never the exercise selection.

use jiff::{Timestamp, civil::Date, tz::TimeZone};

use crate::{
    gym::exercise::Exercise,
    prescription::{
        anchor::Anchor,
        ladder::{InvalidLadder, Ladder},
        parameters::GenerationParameters,
        schedule::{Calendar, SessionRole, Weekdays},
    },
};

use super::template::{PrimaryPattern, SlotFills};

/// What the types could not catch.
///
/// Most of a programme's validity is structural — [`SlotFills`] is total,
/// `PerRole` has both roles, a range must span. These three are not, and each is
/// a way to author a programme that compiles and then cannot work.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InconsistentProgramme {
    #[error(
        "this programme gates on the {gating} session but never runs one, \
         so its ladder would never advance"
    )]
    GatingRoleNeverRuns { gating: SessionRole },
    #[error(
        "the primary exercise {primary} is counted in {measure}, and a top set \
         needs repetitions"
    )]
    PrimaryIsNotCountedInReps {
        primary: &'static str,
        measure: &'static str,
    },
    #[error(
        "this programme names {pattern} as primary but fills that slot with \
         {fill} rather than the primary exercise {primary}"
    )]
    PrimaryDoesNotFillItsSlot {
        pattern: PrimaryPattern,
        primary: &'static str,
        fill: &'static str,
    },
    #[error("the ladder is not a plan: {0}")]
    Ladder(#[from] InvalidLadder),
}

/// A rule for generating a series of prescribed workouts, plus its inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Programme {
    primary: PrimaryPattern,
    primary_exercise: Exercise,
    fills: SlotFills,
    /// The starting 1RM. Fixed for this block; only its exit test replaces it,
    /// and that replacement anchors the *next* block.
    anchor: Anchor,
    gating_role: SessionRole,
    calendar: Calendar,
    authored_at: Timestamp,
}

impl Programme {
    /// Build, running the three checks the type system cannot.
    ///
    /// # Errors
    ///
    /// [`InconsistentProgramme`] for a gating role the programme never runs, a
    /// primary that is not counted in repetitions, a primary exercise that does
    /// not fill the slot named as primary, or a span and duration that do not
    /// make a ladder.
    #[expect(
        clippy::too_many_arguments,
        reason = "an authored programme genuinely has this many inputs, and a \
                  builder would let a caller stop halfway — which is what the \
                  totality of SlotFills exists to prevent"
    )]
    pub fn new(
        primary: PrimaryPattern,
        primary_exercise: Exercise,
        fills: SlotFills,
        anchor: Anchor,
        gating_role: SessionRole,
        start: Date,
        duration_weeks: u32,
        weekdays: Weekdays,
        zone: TimeZone,
        parameters: &GenerationParameters,
    ) -> Result<Self, InconsistentProgramme> {
        Self::check(primary, primary_exercise, &fills, gating_role, &weekdays)?;

        // 4. And the span has to make a ladder over this duration. Checked here
        //    so an unbuildable plan fails at authoring rather than at the first
        //    `prescribe`.
        Ladder::new(
            parameters.ladder_start,
            parameters.ladder_end,
            duration_weeks,
        )?;

        Ok(Self {
            primary,
            primary_exercise,
            fills,
            anchor,
            gating_role,
            calendar: Calendar::new(start, duration_weeks, weekdays, zone),
            authored_at: Timestamp::now(),
        })
    }

    /// Rebuild a programme that was already authored.
    ///
    /// Runs the three checks that depend on nothing but the programme itself and
    /// **does not re-run the ladder check**. That is deliberate: the ladder was
    /// proved buildable against the parameters in force when this was authored,
    /// and those may since have been superseded. Re-checking against today's
    /// parameters would ask a different question and could refuse a programme
    /// that was valid when written — which § 12 says is a durable record, not a
    /// thing to be re-litigated on every read.
    ///
    /// A stored programme failing one of the three is corrupt rather than
    /// inconsistent, and the store reports it that way.
    ///
    /// # Errors
    ///
    /// [`InconsistentProgramme`] for any of the three parameter-independent
    /// checks.
    #[expect(
        clippy::too_many_arguments,
        reason = "the same list as `new`, minus the parameters it does not need"
    )]
    pub fn rehydrate(
        primary: PrimaryPattern,
        primary_exercise: Exercise,
        fills: SlotFills,
        anchor: Anchor,
        gating_role: SessionRole,
        start: Date,
        duration_weeks: u32,
        weekdays: Weekdays,
        zone: TimeZone,
        authored_at: Timestamp,
    ) -> Result<Self, InconsistentProgramme> {
        Self::check(primary, primary_exercise, &fills, gating_role, &weekdays)?;
        Ok(Self {
            primary,
            primary_exercise,
            fills,
            anchor,
            gating_role,
            calendar: Calendar::new(start, duration_weeks, weekdays, zone),
            authored_at,
        })
    }

    /// The three checks that need nothing but the programme.
    fn check(
        primary: PrimaryPattern,
        primary_exercise: Exercise,
        fills: &SlotFills,
        gating_role: SessionRole,
        weekdays: &Weekdays,
    ) -> Result<(), InconsistentProgramme> {
        // 1. A programme gating on a role it never runs would never advance.
        if !weekdays.runs(gating_role) {
            return Err(InconsistentProgramme::GatingRoleNeverRuns {
                gating: gating_role,
            });
        }

        // 2. A top set is a number of repetitions, so a duration or distance
        //    exercise cannot be the primary.
        if !matches!(primary_exercise, Exercise::Reps(_)) {
            return Err(InconsistentProgramme::PrimaryIsNotCountedInReps {
                primary: primary_exercise.as_str(),
                measure: primary_exercise.measure(),
            });
        }

        // 3. Otherwise the programme names one exercise as primary and
        //    prescribes another in the slot it named.
        let filled = match fills.content(primary.slot(), gating_role) {
            super::template::SlotContent::Single(exercise) => *exercise,
            // Unreachable by construction: `PrimaryPattern::slot` returns only
            // the four strength slots, and all four are single.
            super::template::SlotContent::Superset(_) => primary_exercise,
        };
        if filled != primary_exercise {
            return Err(InconsistentProgramme::PrimaryDoesNotFillItsSlot {
                pattern: primary,
                primary: primary_exercise.as_str(),
                fill: filled.as_str(),
            });
        }
        Ok(())
    }

    pub const fn primary(&self) -> PrimaryPattern {
        self.primary
    }

    pub const fn primary_exercise(&self) -> Exercise {
        self.primary_exercise
    }

    pub const fn fills(&self) -> &SlotFills {
        &self.fills
    }

    pub const fn anchor(&self) -> Anchor {
        self.anchor
    }

    pub const fn gating_role(&self) -> SessionRole {
        self.gating_role
    }

    pub const fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    pub const fn authored_at(&self) -> Timestamp {
        self.authored_at
    }

    /// The block's plan.
    ///
    /// Rebuilt from the parameters in force rather than stored, so there is one
    /// place the span becomes a ladder. `Programme::new` has already proved this
    /// succeeds for the duration it holds.
    ///
    /// # Errors
    ///
    /// [`InvalidLadder`] only if the parameters handed in differ from the ones
    /// the programme was authored against.
    pub const fn ladder(&self, parameters: &GenerationParameters) -> Result<Ladder, InvalidLadder> {
        Ladder::new(
            parameters.ladder_start,
            parameters.ladder_end,
            self.calendar.duration_weeks(),
        )
    }

    /// Whether this slot is the primary one.
    ///
    /// The whole of what "primary" earns is decided by asking this: a warm-up
    /// ramp, a top set from the ladder, and back-offs. Every other slot reads its
    /// own history instead.
    pub fn is_primary(&self, slot: crate::prescription::shape::SlotId) -> bool {
        self.primary.slot() == slot
    }
}
