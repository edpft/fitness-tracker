//! What a mesocycle is: a test, or a progression.
//!
//! **Two levels, because there are two questions** (decision 0013). The first is
//! whether this mesocycle measures or progresses — a test belongs to neither
//! neighbour and climbs nothing, so it is not a degenerate progression but the
//! other thing a mesocycle can be. The second only arises once the answer is
//! "progresses": how it progresses.
//!
//! ```text
//! Mesocycle ─┬─ Test         one week, no ladder, a maximum
//!            └─ Progression ─┬─ Linear              a fixed increment a week
//!                            ├─ BlockPeriodisation  phases to a planned endpoint
//!                            └─ Sbs                 a published chart
//! ```
//!
//! Flattening these into one enum would put `Linear` and `Test` side by side and
//! lose the fact that the first two share an entry test, an anchor and a primary
//! that climbs, while a test shares none of it.
//!
//! **It was `Programme` until 2026-09-06**, and the level was wrong rather than
//! the shape. The operator's hierarchy is
//! `macrocycle → plan → programme → mesocycle → microcycle → session`: what this
//! type holds is four weeks of one discipline, which is a mesocycle, and a
//! *programme* is the set of them one discipline runs inside a plan. The enum
//! below was `Periodisation` in the same move — block periodisation is one way
//! of progressing rather than the category all of them belong to.
//!
//! **`Sbs` is a name still owed a replacement.** It labels the method after the
//! publisher of one chart, where its siblings are named for what they do — and
//! the operator, 2026-09-06, on there being other SBS programmes: *Squat 2x Int*
//! is an external programme providing mesocycles, exactly as *Peak Your Power
//! Zones* is. What this variant really is, is a progression taken from such a
//! programme rather than derived here.
//!
//! **The discriminant is not a type.** [`linear`](super::linear) records why:
//! selecting a template is selecting among mesocycle types, so a `Template`
//! enum beside this one would be a second copy of the same distinction, free to
//! disagree with it. What the store needs is a stable string, and
//! [`Mesocycle::template`] derives it from the variant in force.

use crate::{
    gym::exercise::Exercise,
    prescription::{
        anchor::{Anchor, Entry},
        block::BlockPeriodisation,
        linear::{Linear, PrimaryPattern, SlotFills},
        sbs::Sbs,
        schedule::{Calendar, SessionRole},
    },
    provider::ProvidedFrom,
};

/// What was authored: one programme, of whichever kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mesocycle {
    Test(crate::prescription::test::Test),
    Progression(Progression),
}

/// A programme that progresses a lift, by one of the two models.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progression {
    Linear(Linear),
    BlockPeriodisation(BlockPeriodisation),
    /// Taken from an external programme rather than derived here.
    ///
    /// **Not a third way of deriving a progression — the case where we derive
    /// none.** Linear climbs at a rate and block periodisation computes its
    /// phases; this reads its loads off a table somebody else published, and
    /// what moves week to week is the *maximum* rather than a position on a
    /// ladder (decision 0024).
    ///
    /// **It was `Sbs` until 2026-09-06**, which named the variant after the
    /// publisher of one chart where its siblings are named for what they do. The
    /// operator, 2026-09-06, on there being other SBS programmes: *Squat 2x Int*
    /// is an external programme providing mesocycles in the same way *Peak Your
    /// Power Zones* is. Which external programme is a fact about this mesocycle,
    /// not a variant of this enum.
    ///
    /// **The payload is still the SBS cycle**, so the convergence the rename
    /// implies has not happened yet: a provided cycling mesocycle is a different
    /// type today, and whether the two collapse into one is settled by building
    /// both and looking rather than by predicting.
    Provided {
        /// Which microcycles of which external programme this is.
        from: ProvidedFrom,
        cycle: Sbs,
    },
}

impl Mesocycle {
    /// The stable key. Persisted, so it outlives a rename.
    pub const fn template(&self) -> &'static str {
        match self {
            Self::Test(_) => "test",
            Self::Progression(periodisation) => periodisation.template(),
        }
    }

    pub const fn fills(&self) -> &SlotFills {
        match self {
            Self::Test(test) => test.fills(),
            Self::Progression(periodisation) => periodisation.fills(),
        }
    }

    pub const fn calendar(&self) -> &Calendar {
        match self {
            Self::Test(test) => test.calendar(),
            Self::Progression(periodisation) => periodisation.calendar(),
        }
    }

    /// The slot this programme's primary lift fills.
    ///
    /// For a test that is the lift being tested, which is the *next*
    /// programme's primary rather than the predecessor's.
    pub const fn primary(&self) -> PrimaryPattern {
        match self {
            Self::Test(test) => test.primary(),
            Self::Progression(periodisation) => periodisation.primary(),
        }
    }

    pub const fn primary_exercise(&self) -> Exercise {
        match self {
            Self::Test(test) => test.primary_exercise(),
            Self::Progression(periodisation) => periodisation.primary_exercise(),
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
            Self::Progression(periodisation) => Some(periodisation.anchor()),
        }
    }

    /// The maximum this programme leaves behind it, if it leaves one.
    ///
    /// **What a block asks of the programme before it** (decision 0013). A block
    /// opens from a measured maximum in its own lift, and there are exactly two
    /// things that produce one:
    ///
    /// ```text
    /// test      the lift it tested — that is the whole of what it is for
    /// block     its primary, measured by the exit test it always ends on
    /// linear    nothing, ever: a linear programme never includes a test
    /// ```
    ///
    /// The operator's six compositions fall out of comparing this against the
    /// next block's primary, and so do 0013's own six examples. A linear
    /// programme answers `None` even in the same lift, which is the row that
    /// most invites a wrong guess: its last heavy single feels like a test and is
    /// not one, because nothing peaked for it and nothing recorded it as a
    /// maximum.
    ///
    /// It says what the programme *plans* to measure, not what the record holds:
    /// a block abandoned in week three planned an exit test it never took. The
    /// recency rule beside it is what keeps that from anchoring anything, since
    /// an abandoned block's successor is not the week after its exit test.
    #[must_use]
    pub const fn produces_maximum(&self) -> Option<Exercise> {
        match self {
            Self::Test(test) => Some(test.primary_exercise()),
            Self::Progression(Progression::BlockPeriodisation(block)) => {
                Some(block.primary_exercise())
            }
            // **An SBS cycle always ends on a one-rep maximum.** Week 4 day 2
            // is a test and is not optional, so a cycle that runs to its end
            // leaves a measured maximum behind exactly as a block does — and
            // that maximum is what the next cycle opens from, which is what
            // makes the chart self-perpetuating (decision 0024).
            Self::Progression(Progression::Provided { cycle: sbs, .. }) => {
                Some(sbs.primary_exercise())
            }
            Self::Progression(Progression::Linear(_)) => None,
        }
    }

    /// Whether this programme's anchor is a claim about a test that already
    /// happened.
    ///
    /// **A block's anchor comes from one of three places, and the authored
    /// programme says which:**
    ///
    /// ```text
    /// a previous test    provenance = tested, and no entry test of its own
    /// its own entry test the anchor is what the operator expects; week one
    ///                    measures it
    /// declared           provenance = asserted or estimated: a number, and
    ///                    it says so
    /// ```
    ///
    /// Only the first is a claim about the past, so only the first is checkable
    /// — and it is the one the store has to be asked about. The other two are
    /// complete statements on their own: an entry test measures its own anchor,
    /// and a declared one is honest about being a number.
    ///
    /// **Blocks only.** A linear programme's anchor may be superseded by a
    /// declared opening — the summer block's tested anchor is a month old and
    /// deliberately feeds nothing — so the same rule there would need a carve-out
    /// for exactly the case it exists to allow. A block has no opening: every
    /// load is a share of the anchor.
    #[must_use]
    pub const fn claims_an_earlier_maximum(&self) -> bool {
        match self {
            Self::Progression(Progression::BlockPeriodisation(block)) => {
                block.entry_test().is_none()
                    && matches!(
                        block.entry().anchor().provenance(),
                        crate::prescription::AnchorProvenance::Tested
                    )
            }
            // **The same claim a block makes, and for a stronger reason.** An
            // SBS cycle has no entry test at all — its test is the *last*
            // session, not the first — so a `Tested` anchor here can only be
            // pointing at something that already happened: the standalone week 4
            // that opens the sequence, or the previous cycle's own week 4. There
            // is no case where the cycle is about to measure its own opening, so
            // no carve-out is needed for one.
            Self::Progression(Progression::Provided { cycle: sbs, .. }) => matches!(
                sbs.entry().anchor().provenance(),
                crate::prescription::AnchorProvenance::Tested
            ),
            Self::Progression(Progression::Linear(_)) | Self::Test(_) => false,
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
            Self::Progression(periodisation) => Some(periodisation.gating_role()),
        }
    }
}

impl Progression {
    /// The stable key. Persisted.
    pub const fn template(&self) -> &'static str {
        match self {
            Self::Linear(_) => "linear",
            // **Still the old spellings, and deliberately.** These are
            // persisted keys, named by the store's own CHECK constraint, and
            // this method exists precisely so a variant can be renamed without
            // rewriting rows. They change when the schema does.
            Self::BlockPeriodisation(_) => "block",
            Self::Provided { .. } => "sbs",
        }
    }

    pub const fn fills(&self) -> &SlotFills {
        match self {
            Self::Linear(linear) => linear.fills(),
            Self::BlockPeriodisation(block) => block.fills(),
            Self::Provided { cycle: sbs, .. } => sbs.fills(),
        }
    }

    pub const fn calendar(&self) -> &Calendar {
        match self {
            Self::Linear(linear) => linear.calendar(),
            Self::BlockPeriodisation(block) => block.calendar(),
            Self::Provided { cycle: sbs, .. } => sbs.calendar(),
        }
    }

    pub const fn primary(&self) -> PrimaryPattern {
        match self {
            Self::Linear(linear) => linear.primary(),
            Self::BlockPeriodisation(block) => block.primary(),
            Self::Provided { cycle: sbs, .. } => sbs.primary(),
        }
    }

    pub const fn primary_exercise(&self) -> Exercise {
        match self {
            Self::Linear(linear) => linear.primary_exercise(),
            Self::BlockPeriodisation(block) => block.primary_exercise(),
            Self::Provided { cycle: sbs, .. } => sbs.primary_exercise(),
        }
    }

    /// The entry test both models open from, and the opening where one is
    /// declared rather than derived.
    pub const fn entry(&self) -> Entry {
        match self {
            Self::Linear(linear) => linear.entry(),
            Self::BlockPeriodisation(block) => block.entry(),
            Self::Provided { cycle: sbs, .. } => sbs.entry(),
        }
    }

    #[must_use]
    pub const fn anchor(&self) -> Anchor {
        self.entry().anchor()
    }

    pub const fn gating_role(&self) -> SessionRole {
        match self {
            Self::Linear(linear) => linear.gating_role(),
            Self::BlockPeriodisation(block) => block.gating_role(),
            Self::Provided { cycle: sbs, .. } => sbs.gating_role(),
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
pub enum InconsistentMesocycle {
    #[error(
        "this programme gates on the {gating} session but never runs one, \
         so its ladder would never advance"
    )]
    GatingRoleNeverRuns { gating: SessionRole },
    #[error(
        "the SBS chart states four weeks and this calendar runs {given} — a \
         cycle of another length is a different programme, not this one \
         stretched"
    )]
    ChartIsFourWeeks { given: u32 },
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
/// [`InconsistentMesocycle`] for either.
pub fn check_primary(
    pattern: PrimaryPattern,
    exercise: Exercise,
    fills: &SlotFills,
    role: SessionRole,
) -> Result<(), InconsistentMesocycle> {
    if !matches!(exercise, Exercise::Reps(_)) {
        return Err(InconsistentMesocycle::PrimaryIsNotCountedInReps {
            primary: exercise.as_str(),
            measure: exercise.measure(),
        });
    }
    let filled = *fills.primary(pattern, role);
    if filled != exercise {
        return Err(InconsistentMesocycle::PrimaryDoesNotFillItsSlot {
            pattern,
            primary: exercise.as_str(),
            fill: filled.as_str(),
        });
    }
    Ok(())
}
