//! A programme written against `v1`.
//!
//! **Its purpose is to increase the primary exercise's 1RM**, and its whole
//! primary loading series is a function of two authored values: a duration in
//! weeks and a starting 1RM. Everything else on this struct decides which
//! exercises fill which slots, not what the plan is.
//!
//! Fills are inputs rather than choices the programme makes. Generation produces
//! the loading series, never the exercise selection.

use jiff::Timestamp;

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
    #[error(
        "this programme starts on {start} but its entry test is dated {tested},          which is not before it"
    )]
    EntryTestIsNotBeforeTheBlock {
        start: jiff::civil::Date,
        tested: jiff::civil::Date,
    },
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
    /// not fill the slot named as primary, or a climb and duration that do not
    /// make a ladder.
    pub fn new(
        primary: PrimaryPattern,
        primary_exercise: Exercise,
        fills: SlotFills,
        anchor: Anchor,
        gating_role: SessionRole,
        calendar: Calendar,
        parameters: &GenerationParameters,
    ) -> Result<Self, InconsistentProgramme> {
        Self::check(
            primary,
            primary_exercise,
            &fills,
            gating_role,
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
        if anchor.from() >= calendar.start() {
            return Err(InconsistentProgramme::EntryTestIsNotBeforeTheBlock {
                start: calendar.start(),
                tested: anchor.from(),
            });
        }

        // 5. And the climb has to make a ladder over this duration. Checked here
        //    so an unbuildable plan fails at authoring rather than at the first
        //    `prescribe`. Training weeks, not calendar ones: a block interrupted
        //    by a holiday is the same ladder run over a longer stretch of the
        //    year, not a longer ladder.
        Ladder::new(
            anchor,
            parameters.ladder_climb_per_week,
            calendar.duration_weeks(),
            parameters.plate_increment,
        )?;

        Ok(Self {
            primary,
            primary_exercise,
            fills,
            anchor,
            gating_role,
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
        primary: PrimaryPattern,
        primary_exercise: Exercise,
        fills: SlotFills,
        anchor: Anchor,
        gating_role: SessionRole,
        calendar: Calendar,
        authored_at: Timestamp,
    ) -> Result<Self, InconsistentProgramme> {
        Self::check(
            primary,
            primary_exercise,
            &fills,
            gating_role,
            calendar.weekdays(),
        )?;
        Ok(Self {
            primary,
            primary_exercise,
            fills,
            anchor,
            gating_role,
            calendar,
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
            super::template::SlotContent::Superset(_) | super::template::SlotContent::Static(_) => {
                primary_exercise
            }
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
    /// place the climb becomes a ladder. `Programme::new` has already proved this
    /// succeeds for the duration it holds.
    ///
    /// # Errors
    ///
    /// [`InvalidLadder`] only if the parameters handed in differ from the ones
    /// the programme was authored against.
    pub fn ladder(&self, parameters: &GenerationParameters) -> Result<Ladder, InvalidLadder> {
        Ladder::new(
            self.anchor,
            parameters.ladder_climb_per_week,
            self.calendar.duration_weeks(),
            parameters.plate_increment,
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
