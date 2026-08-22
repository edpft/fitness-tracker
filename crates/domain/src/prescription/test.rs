//! A test: a programme in its own right, belonging to neither neighbour.
//!
//! **One week, no ladder** (decision 0013). A linear programme never includes a
//! test and a periodised block always ends in one, which leaves the case that is
//! neither — the week between a linear programme and the block that needs an
//! entry test — with nowhere to live. This is that week.
//!
//! **It is not a whole programme's worth of authoring.** The operator does not
//! re-author seventeen slots for two sessions, so a test document names what
//! changes — the lift being tested and any accessory variant moving with it —
//! and the rest is inherited from the programme it follows. That inheritance is
//! resolved when the document is read, not when a session is derived: what this
//! type holds is a complete [`SlotFills`] exactly as a linear programme does, so
//! nothing downstream can tell the difference and re-authoring the predecessor
//! cannot retroactively move what this test prescribes (§ 12, § 14).
//!
//! **The week is two sessions and only one of them is the test.**
//!
//! ```text
//! light   the predecessor's session, unchanged, at the light load its
//!         progression stands at
//! heavy   the test: a ramp toward the target, then one autoregulated single
//! ```
//!
//! That asymmetry is why the tested exercise and the predecessor's may both
//! appear in the fills, as the two halves of one [`Fill::Alternating`]. Where the
//! test changes the primary pattern they occupy different slots instead, and
//! where it does not they share one.
//!
//! [`Fill::Alternating`]: crate::prescription::Fill::Alternating

use jiff::{Timestamp, civil::Date, tz::TimeZone};

use crate::{
    gym::{Kg, RepCount, exercise::Exercise},
    prescription::{
        linear::{PrimaryPattern, SlotFills},
        programme::{InconsistentProgramme, check_primary},
        repmax::rep_max,
        schedule::{Calendar, InvalidCalendar, SessionRole, Skip, Weekdays},
        shape::SlotId,
        succession::{ProgrammeName, ProgrammeWindow},
    },
};

/// What the test is an attempt at.
///
/// **Inherited by default, because the target moves as the record does**
/// (decision 0011). Every rung the predecessor's progression makes raises it, so
/// a number written into a document at authoring time is stale the first time a
/// session goes up. This is the one thing a test does *not* resolve at
/// authoring.
///
/// Declared is for the case inheritance cannot answer: a test with nothing
/// before it, or one whose predecessor trained a different lift — a front squat
/// target is not evidence about an RDL, so 0013 refuses to carry one across.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestTarget {
    /// From the programme before it, as the record stands.
    Inherited,
    /// Stated, where there is nothing to inherit from.
    Declared(Kg),
}

/// What is being tested, and how it is being measured.
///
/// **One argument because they are one decision**, which is the reasoning
/// [`Primary`](crate::prescription::Primary) is built on: the pattern names a
/// slot, the exercise fills it, and the repetition count says what the resulting
/// number will mean. Choosing a lift to test without saying what a successful
/// attempt at it looks like is not a decision anyone makes.
///
/// Where [`Primary`] carries a gating role, this carries a repetition count.
/// That is the whole difference between progressing a lift and measuring one.
///
/// [`Primary`]: crate::prescription::Primary
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tested {
    pattern: PrimaryPattern,
    exercise: Exercise,
    reps: RepCount,
}

impl Tested {
    pub const fn new(pattern: PrimaryPattern, exercise: Exercise, reps: RepCount) -> Self {
        Self {
            pattern,
            exercise,
            reps,
        }
    }

    pub const fn pattern(self) -> PrimaryPattern {
        self.pattern
    }

    pub const fn exercise(self) -> Exercise {
        self.exercise
    }

    pub const fn reps(self) -> RepCount {
        self.reps
    }
}

/// A standalone test week.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Test {
    /// What identifies this test across re-authorings (decision 0012). A test
    /// is a programme, so it names itself and competes for its days like one.
    name: ProgrammeName,
    /// The lift being tested, the slot it fills — which is the *next*
    /// programme's primary pattern, not the predecessor's — and what the attempt
    /// is performed at.
    ///
    /// **A single before a linear programme, a triple before a block.** The
    /// choice belongs to what the test is for: `block.rs` enters on a triple
    /// deliberately, because a cold maximal single measures technique as much as
    /// strength, and exits on a single because that is what its realisation
    /// weeks prepared. A test that anchors nothing in particular is a single.
    ///
    /// The count is consumed where the [`Anchor`](crate::prescription::Anchor)
    /// is built from what the test achieved, through
    /// [`rep_max`](crate::prescription::rep_max) — so by the time any programme
    /// reads that anchor it is a one-rep maximum and this number is history.
    tested: Tested,
    /// Complete, and resolved from the predecessor's when the document was read.
    fills: SlotFills,
    /// One week, always: [`Test::new`] builds it and nothing else may.
    calendar: Calendar,
    target: TestTarget,
    authored_at: Timestamp,
}

impl Test {
    /// The week a test occupies.
    ///
    /// One, by definition. A test that ran for two weeks would be two attempts
    /// at one maximum, and the second would be reading the first as history.
    pub const WEEKS: u32 = 1;

    /// The session the test is taken on.
    ///
    /// **Heavy, and not a choice the document makes.** The light session of the
    /// week is the predecessor's, run unchanged; the heavy one is replaced by
    /// the test. A test programme that names a gating role would be naming
    /// something with no ladder to gate, which is why [`Test`] has no such
    /// field where [`Linear`](crate::prescription::Linear) does.
    pub const ROLE: SessionRole = SessionRole::Heavy;

    /// The week a test occupies, as a calendar.
    ///
    /// **The only way to build one for a test.** [`Self::WEEKS`] is applied
    /// here, so a duration is not something a caller can get wrong — and a
    /// document that tries to state one has nowhere to put it.
    ///
    /// # Errors
    ///
    /// [`InvalidCalendar`] where the week the test occupies is interrupted away,
    /// which is a test that is never taken.
    pub fn week(
        start: Date,
        interruptions: &[Skip],
        weekdays: Weekdays,
        zone: TimeZone,
    ) -> Result<Calendar, InvalidCalendar> {
        Calendar::new(start, Self::WEEKS, interruptions, weekdays, zone)
    }

    /// Build, running the checks the type system cannot.
    ///
    /// # Errors
    ///
    /// [`InconsistentProgramme`] for a calendar longer than a test's one week, a
    /// tested exercise not counted in repetitions, one that does not fill the
    /// slot the pattern names, a repetition count the maximum table cannot
    /// convert, or a weekday map that never runs the session the test is taken
    /// on.
    pub fn new(
        name: ProgrammeName,
        tested: Tested,
        fills: SlotFills,
        calendar: Calendar,
        target: TestTarget,
    ) -> Result<Self, InconsistentProgramme> {
        Self::check(tested, &fills, &calendar)?;
        Ok(Self {
            name,
            tested,
            fills,
            calendar,
            target,
            authored_at: Timestamp::now(),
        })
    }

    /// Rebuild a test that was already authored.
    ///
    /// Runs the same checks: they depend on nothing but the test itself, so a
    /// stored test failing one is corrupt rather than inconsistent and the store
    /// reports it that way.
    ///
    /// # Errors
    ///
    /// As [`Self::new`].
    pub fn rehydrate(
        name: ProgrammeName,
        tested: Tested,
        fills: SlotFills,
        calendar: Calendar,
        target: TestTarget,
        authored_at: Timestamp,
    ) -> Result<Self, InconsistentProgramme> {
        Self::check(tested, &fills, &calendar)?;
        Ok(Self {
            name,
            tested,
            fills,
            calendar,
            target,
            authored_at,
        })
    }

    /// The four checks that need nothing but the test.
    fn check(
        tested: Tested,
        fills: &SlotFills,
        calendar: &Calendar,
    ) -> Result<(), InconsistentProgramme> {
        // 1. A test is one week. `Self::week` is the only builder that says so,
        //    and a calendar can also arrive from the store, so this is what
        //    holds for a row somebody wrote by hand.
        if calendar.duration_weeks() != Self::WEEKS {
            return Err(InconsistentProgramme::TestIsNotOneWeek {
                weeks: calendar.duration_weeks(),
            });
        }

        // 2. A test that never runs the session it is taken on is not a test.
        //    The linear equivalent is a gate on a role the programme never runs,
        //    and it is the same mistake: a plan whose whole purpose falls on a
        //    day it does not train.
        if !calendar.weekdays().runs(Self::ROLE) {
            return Err(InconsistentProgramme::TestNeverRunsItsSession { role: Self::ROLE });
        }

        // 3. The tested lift has to be countable in repetitions and has to fill
        //    the slot it named. The role matters here in a way it does not for a
        //    linear programme: the light session of this week may legitimately
        //    fill the same slot with the predecessor's lift, so it is the test's
        //    own session that has to agree.
        check_primary(tested.pattern(), tested.exercise(), fills, Self::ROLE)?;

        // 4. And what the test measures has to be convertible into the maximum
        //    every programme after it derives from. This check used to live on
        //    `Block::new`, where it asked the same question of a number the
        //    block held; the number belongs to the test, so the check follows
        //    it.
        if rep_max(tested.reps()).is_none() {
            return Err(InconsistentProgramme::TestRepsTooMany {
                reps: tested.reps().as_u32(),
            });
        }
        Ok(())
    }

    pub const fn name(&self) -> &ProgrammeName {
        &self.name
    }

    /// What is being tested, whole.
    pub const fn tested(&self) -> Tested {
        self.tested
    }

    /// The slot the tested lift fills.
    pub const fn primary(&self) -> PrimaryPattern {
        self.tested.pattern()
    }

    /// The lift being tested.
    pub const fn primary_exercise(&self) -> Exercise {
        self.tested.exercise()
    }

    /// What the test is performed at: a single, or a triple before a block.
    pub const fn reps(&self) -> RepCount {
        self.tested.reps()
    }

    pub const fn fills(&self) -> &SlotFills {
        &self.fills
    }

    pub const fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    pub const fn target(&self) -> TestTarget {
        self.target
    }

    pub const fn authored_at(&self) -> Timestamp {
        self.authored_at
    }

    /// The days this test occupies, for the rule that two programmes may not
    /// compete for one of them.
    #[must_use]
    pub fn window(&self) -> ProgrammeWindow {
        ProgrammeWindow::new(
            self.name.clone(),
            self.calendar.start(),
            self.calendar.calendar_weeks(),
        )
    }

    /// Whether this slot is the one being tested.
    ///
    /// **Asked of a role as well as a slot**, unlike the linear equivalent. On
    /// the light session the same slot is the predecessor's primary and gets a
    /// primary's treatment from its own progression, not a test's.
    #[must_use]
    pub fn is_tested(&self, slot: SlotId, role: SessionRole) -> bool {
        role == Self::ROLE && self.tested.pattern().slot() == slot
    }
}
