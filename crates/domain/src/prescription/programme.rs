//! What a programme is: a test, or a way of periodising.
//!
//! **Two levels, because there are two questions** (decision 0013). The first is
//! whether this programme measures or progresses — a test belongs to neither
//! neighbour and climbs nothing, so it is not a degenerate progression but the
//! other thing a programme can be. The second only arises once the answer is
//! "progresses": linear and block are two models of periodisation, and
//! `block.rs` has said so since 2026-08-18.
//!
//! ```text
//! Programme  ─┬─ Test                    one week, no ladder, a maximum
//!             └─ Periodisation ─┬─ Linear   a top-set ladder at a rate
//!                               └─ Block    phases to a planned endpoint
//! ```
//!
//! Flattening these into one enum would put `Linear` and `Test` side by side and
//! lose the fact that the first two share an entry test, an anchor and a primary
//! that climbs, while a test shares none of it.
//!
//! **The discriminant is not a type.** [`linear`](super::linear) records why:
//! selecting a template is selecting among programme types, so a `Template`
//! enum beside this one would be a second copy of the same distinction, free to
//! disagree with it. What the store and the document reader need is a stable
//! string, and [`Programme::template`] derives it from the variant in force.

use jiff::Timestamp;

use crate::{
    gym::exercise::Exercise,
    prescription::{
        anchor::{Anchor, Entry},
        block::Periodised,
        linear::{Linear, PrimaryPattern, SlotFills},
        schedule::{Calendar, SessionRole},
        succession::{ProgrammeName, ProgrammeWindow},
    },
};

/// What was authored: one programme, of whichever kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Programme {
    Test(crate::prescription::test::Test),
    Periodisation(Periodisation),
}

/// A programme that progresses a lift, by one of the two models.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Periodisation {
    Linear(Linear),
    Block(Periodised),
}

impl Programme {
    /// The stable key. Persisted, so it outlives a rename.
    pub const fn template(&self) -> &'static str {
        match self {
            Self::Test(_) => "test",
            Self::Periodisation(periodisation) => periodisation.template(),
        }
    }

    pub const fn name(&self) -> &ProgrammeName {
        match self {
            Self::Test(test) => test.name(),
            Self::Periodisation(periodisation) => periodisation.name(),
        }
    }

    pub const fn fills(&self) -> &SlotFills {
        match self {
            Self::Test(test) => test.fills(),
            Self::Periodisation(periodisation) => periodisation.fills(),
        }
    }

    pub const fn calendar(&self) -> &Calendar {
        match self {
            Self::Test(test) => test.calendar(),
            Self::Periodisation(periodisation) => periodisation.calendar(),
        }
    }

    pub const fn authored_at(&self) -> Timestamp {
        match self {
            Self::Test(test) => test.authored_at(),
            Self::Periodisation(periodisation) => periodisation.authored_at(),
        }
    }

    /// The slot this programme's primary lift fills.
    ///
    /// For a test that is the lift being tested, which is the *next*
    /// programme's primary rather than the predecessor's.
    pub const fn primary(&self) -> PrimaryPattern {
        match self {
            Self::Test(test) => test.primary(),
            Self::Periodisation(periodisation) => periodisation.primary(),
        }
    }

    pub const fn primary_exercise(&self) -> Exercise {
        match self {
            Self::Test(test) => test.primary_exercise(),
            Self::Periodisation(periodisation) => periodisation.primary_exercise(),
        }
    }

    /// The days this programme occupies, for the rule that two programmes may
    /// not compete for one of them.
    #[must_use]
    pub fn window(&self) -> ProgrammeWindow {
        match self {
            Self::Test(test) => test.window(),
            Self::Periodisation(periodisation) => periodisation.window(),
        }
    }

    /// The anchor this programme's loads derive from, where it has one.
    ///
    /// **A test has none, and that is the point of it.** It produces the number
    /// the next programme anchors on rather than consuming one.
    #[must_use]
    pub const fn anchor(&self) -> Option<Anchor> {
        match self {
            Self::Test(_) => None,
            Self::Periodisation(periodisation) => Some(periodisation.anchor()),
        }
    }

    /// Which session's top set advances the plan, where anything does.
    ///
    /// A test gates nothing: it has no ladder to advance and its own session is
    /// fixed at [`Test::ROLE`](crate::prescription::test::Test::ROLE).
    #[must_use]
    pub const fn gating_role(&self) -> Option<SessionRole> {
        match self {
            Self::Test(_) => None,
            Self::Periodisation(periodisation) => Some(periodisation.gating_role()),
        }
    }
}

impl Periodisation {
    /// The stable key. Persisted.
    pub const fn template(&self) -> &'static str {
        match self {
            Self::Linear(_) => "linear",
            Self::Block(_) => "block",
        }
    }

    pub const fn name(&self) -> &ProgrammeName {
        match self {
            Self::Linear(linear) => linear.name(),
            Self::Block(block) => block.name(),
        }
    }

    pub const fn fills(&self) -> &SlotFills {
        match self {
            Self::Linear(linear) => linear.fills(),
            Self::Block(block) => block.fills(),
        }
    }

    pub const fn calendar(&self) -> &Calendar {
        match self {
            Self::Linear(linear) => linear.calendar(),
            Self::Block(block) => block.calendar(),
        }
    }

    pub const fn authored_at(&self) -> Timestamp {
        match self {
            Self::Linear(linear) => linear.authored_at(),
            Self::Block(block) => block.authored_at(),
        }
    }

    pub const fn primary(&self) -> PrimaryPattern {
        match self {
            Self::Linear(linear) => linear.primary(),
            Self::Block(block) => block.primary(),
        }
    }

    pub const fn primary_exercise(&self) -> Exercise {
        match self {
            Self::Linear(linear) => linear.primary_exercise(),
            Self::Block(block) => block.primary_exercise(),
        }
    }

    /// The entry test both models open from, and the opening where one is
    /// declared rather than derived.
    pub const fn entry(&self) -> Entry {
        match self {
            Self::Linear(linear) => linear.entry(),
            Self::Block(block) => block.entry(),
        }
    }

    #[must_use]
    pub const fn anchor(&self) -> Anchor {
        self.entry().anchor()
    }

    pub const fn gating_role(&self) -> SessionRole {
        match self {
            Self::Linear(linear) => linear.gating_role(),
            Self::Block(block) => block.gating_role(),
        }
    }

    #[must_use]
    pub fn window(&self) -> ProgrammeWindow {
        match self {
            Self::Linear(linear) => linear.window(),
            Self::Block(block) => block.window(),
        }
    }
}

/// What the types could not catch about an authored programme.
///
/// **One enum across the three templates**, because the checks overlap almost
/// entirely: a primary that is not counted in repetitions and a primary that
/// does not fill its own slot are the same mistake whichever template made it,
/// and the store and the CLI report them the same way. What differs is which
/// variants a given template can produce, and that is not worth three types
/// which every reader would then have to hold apart.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InconsistentProgramme {
    #[error(
        "this programme gates on the {gating} session but never runs one, \
         so its ladder would never advance"
    )]
    GatingRoleNeverRuns { gating: SessionRole },
    #[error(
        "a test is taken on the {role} session and this one never runs it, \
         so the test would never be taken"
    )]
    TestNeverRunsItsSession { role: SessionRole },
    #[error("a test is one week and this one is {weeks}")]
    TestIsNotOneWeek { weeks: u32 },
    #[error(
        "a test at {reps} repetitions is too many for the repetition-maximum \
         table to convert into the maximum a programme after it would anchor on"
    )]
    TestRepsTooMany { reps: u32 },
    #[error(
        "a block opens from a measured maximum and this one is {provenance}; \
         a number that was asserted rather than tested is not an entry test"
    )]
    BlockAnchorIsNotTested {
        provenance: crate::prescription::anchor::AnchorProvenance,
    },
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
    Ladder(#[from] crate::prescription::ladder::InvalidLadder),
    #[error("the block is not a plan: {0}")]
    Block(#[from] crate::prescription::block::InvalidBlock),
    #[error(
        "this programme starts on {start} but its entry test is dated {tested},          which is not before it"
    )]
    EntryTestIsNotBeforeTheBlock {
        start: jiff::civil::Date,
        tested: jiff::civil::Date,
    },
}

/// The two checks that are about the lift rather than about the plan.
///
/// **One function because it is one rule twice, whatever the template.** A
/// primary that cannot carry a top set and a primary that does not fill the slot
/// it named are mistakes about the relationship between a template's slots and
/// the lift being progressed — and that relationship belongs to the template,
/// not to the model of periodisation.
///
/// The role is what the caller varies, and it is not the same question each
/// time: a linear programme and a block ask about their gating session, and a
/// test asks about its own. Whether the programme runs that session at all is
/// left to the caller, because the message for getting it wrong differs — a
/// ladder that never advances is not a test that is never taken.
///
/// # Errors
///
/// [`InconsistentProgramme`] for either.
pub fn check_primary(
    pattern: PrimaryPattern,
    exercise: Exercise,
    fills: &SlotFills,
    role: SessionRole,
) -> Result<(), InconsistentProgramme> {
    if !matches!(exercise, Exercise::Reps(_)) {
        return Err(InconsistentProgramme::PrimaryIsNotCountedInReps {
            primary: exercise.as_str(),
            measure: exercise.measure(),
        });
    }
    let filled = *fills.primary(pattern, role);
    if filled != exercise {
        return Err(InconsistentProgramme::PrimaryDoesNotFillItsSlot {
            pattern,
            primary: exercise.as_str(),
            fill: filled.as_str(),
        });
    }
    Ok(())
}
