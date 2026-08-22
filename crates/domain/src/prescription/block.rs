//! Block periodisation: what each training week of a periodised programme
//! prescribes.
//!
//! **The [`linear`](super::linear) template is not superseded by this and is not
//! going anywhere.** The two answer different questions about the same lift. A
//! linear top-set ladder is the right tool for a short or interrupted window —
//! the weeks before Christmas, a run broken up by travel — and periodisation is
//! the right tool when the calendar gives eight training weeks and a test week
//! besides. Which one a programme uses is decided per programme, from the number
//! of weeks available. They were `v1` and `v2` until 2026-08-18, which named them
//! as versions of one thing when they are two models of periodisation.
//!
//! **A note on the word.** [`Block`] here is a *periodised* block — the plan this
//! module builds. "Block" also gets used loosely across this crate for any
//! programme's run of weeks, including a linear one, and that looser sense is
//! what `duration_weeks` and `WeekKind` mean by it.
//!
//! **Two inputs, and every load in the block comes out of them**: how many
//! training weeks the calendar allows, and the entry test the block is anchored
//! on. Nothing else is authored. Research D11 has the loading derivation and D12
//! the phase structure; what follows is what a reader of the code needs in order
//! not to undo them.
//!
//! ```text
//! weeks 1..     accumulation    — five sets across, reps descending, load rising
//!    ..         intensification — one top set,      reps descending, load rising
//!    ..N        realisation     — one top set,      descending to a single
//! week N        the last realisation week IS the exit test, and it is a 1RM
//! ```
//!
//! **The duration counts phase weeks and nothing else** (decision 0013). The
//! operator's table — 8 weeks as 3-3-2, 9 as 4-3-2, 10 as 4-4-2, 11 as 4-4-3 —
//! counts accumulation, intensification and realisation and leaves no week for
//! the entry test, because the entry test is not in the block at all: it is the
//! standalone test the week before, or the exit test of the block before that.
//! Week 1 used to be an entry test and [`Block::total_weeks`] used to be one
//! longer than the duration, which made a duration mean two things in the same
//! way linear's did. Both are gone.
//!
//! **The split is a rule, not four rows.** Eight weeks is the shortest block
//! anyone runs, at 3-3-2, and every week beyond it goes to accumulation, then
//! intensification, then realisation, in rotation. That reproduces the operator's
//! four rows exactly and carries on past them — 12 weeks is 5-4-3 — so a
//! duration nobody tabulated still plans.
//!
//! **There is no RIR anywhere in this plan.** The operator settled that on
//! 2026-08-18: a percentage-based plan states percentages, and reaching one by
//! subtracting a number of repetitions in reserve from a maximum is an RIR
//! parameter whatever it is called. Accumulation used to be placed that way and
//! is not any more — Prilepin's own repetitions-per-set column places it, and the
//! loads it produces are the same shape without the reserve.
//!
//! **The three phases are loaded by two rules, and neither is a parameter:**
//!
//! - Accumulation runs many sets, so no set can be a maximum. Its heaviest rung
//!   is a double, and Prilepin's chart says the lightest load a double belongs at
//!   is 80% — below that the chart asks for sets of three to six. So the phase is
//!   pinned at 80% for its double and every earlier rung is one repetition more
//!   and 2.5 points lighter, which is the repetition-maximum table's own slope.
//!   Both numbers come from published tables; neither is chosen here.
//! - Intensification and realisation run a single top set, and **one ladder runs
//!   through both of them**: the repetitions descend without interruption to the
//!   single the block finishes on, and the load climbs without interruption from
//!   where accumulation left off to the block's endpoint. A discontinuity at the
//!   phase boundary would be a number somebody chose, and there is no such
//!   number here. What realisation contributes is the last rungs — the ones at
//!   and above the entry maximum — which is why the literature's realisation
//!   intensity of 90% and up falls out of the span rather than being asserted
//!   over it.
//!
//! **The endpoint is 105% of the entry one-rep maximum**, and the exit test is a
//! single, so the block plans a 5% gain and tests it in the unit it was planned
//! in. That is the figure the Russian squat routine, Arbic's block programme and
//! meet-attempt convention all land on.
//!
//! **105% is a convention rather than a measurement**, and is recorded as one.
//! Its authority is that it is shared — this block can be read against every
//! published block that also finishes at 105% — and it is falsifiable against
//! the operator's own exit tests, which a number chosen here would not be.
//!
//! **Every percentage here is a share of the one-rep maximum**, and the entry
//! test need not have been one: a three-repetition test is converted through
//! [`rep_max`](crate::prescription::rep_max) where the [`Anchor`] is built, not
//! here. Entering on a triple and exiting on a single is deliberate — a cold
//! maximal single measures technique as much as strength, and a peaked one is
//! what the block spent its realisation weeks preparing. Which repetition count
//! the entry test was taken at is therefore the *test's* business
//! ([`Test::reps`](crate::prescription::test::Test::reps)) and not recorded
//! again here: by the time a block reads it, it is an [`Anchor`] and already a
//! one-rep maximum.
//!
//! **Duration changes where the block starts and never where it finishes.** An
//! eight-week block climbs the same span in five rungs that a twelve-week block
//! climbs in seven. That is the same property the linear ladder has, and the
//! reason a duration is an input rather than a parameter.
//!
//! [`Anchor`]: crate::prescription::Anchor

use jiff::{Timestamp, civil::Date, tz::TimeZone};

use crate::gym::{Kg, RepCount, exercise::Exercise};

use crate::prescription::{
    anchor::Entry,
    linear::{Primary, PrimaryPattern, SlotFills},
    parameters::Percentage,
    prilepin,
    programme::{InconsistentProgramme, check_primary},
    repmax::{PER_REPETITION, rep_max},
    schedule::{Calendar, InvalidCalendar, SessionRole, Skip, WeekIndex, WeekKind, Weekdays},
    succession::{ProgrammeName, ProgrammeWindow},
};

/// Why a block could not be planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidBlock {
    #[error(
        "the shortest block anyone runs is three weeks of accumulation, three \
         of intensification and two of realisation, so {weeks} weeks is too few \
         for one"
    )]
    TooShort { weeks: u32 },
    #[error(
        "{weeks} weeks of phases would open the top set above the maximum for \
         its own repetition count, so it is too long for one block"
    )]
    TooLong { weeks: u32 },
    #[error(
        "an entry test at {reps} repetitions is too many for the \
         repetition-maximum table to convert into the maximum this block's \
         percentages are shares of"
    )]
    EntryTestTooLong { reps: u32 },
}

/// Which phase of the block a week belongs to.
///
/// The tests are not a phase. Week 1 establishes the anchor and the block's last
/// week spends it, and neither is a rung — which [`WeekKind`] already says for
/// the exit and this says for the entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Accumulation,
    Intensification,
    Realisation,
}

impl Phase {
    /// The stable key. Persisted, so it outlives a rename.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accumulation => "accumulation",
            Self::Intensification => "intensification",
            Self::Realisation => "realisation",
        }
    }
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What one week of the block prescribes for the primary lift.
///
/// A test carries no load, which is the whole reason this is a sum type: a test
/// is worked up to, and a caller cannot ask one for a percentage it does not
/// have. The `expected` share is for the warm-up ramp and is an expectation
/// rather than a prescription — the block hopes to be beaten there.
///
/// **Only the exit test is here.** The entry test is a programme of its own and
/// precedes this one (decision 0013), so a block has exactly one test and it is
/// the one it ends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeekPlan {
    Working {
        phase: Phase,
        sets: RepCount,
        reps: RepCount,
        load: Percentage,
    },
    ExitTest {
        reps: RepCount,
        expected: Percentage,
    },
}

/// A periodised block, as a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    duration_weeks: u32,
}

/// The shortest block's split, which every longer one grows from.
const SHORTEST: (u32, u32, u32) = (3, 3, 2);

impl Block {
    /// Three weeks of accumulation, three of intensification and two of
    /// realisation — the shortest block the literature admits, and the operator's
    /// own floor.
    pub const MINIMUM_WEEKS: u32 = SHORTEST.0 + SHORTEST.1 + SHORTEST.2;
    /// The most sets across accumulation will run.
    ///
    /// A ceiling rather than a count: Prilepin's band for the week's load caps
    /// the total lifts, so a high-repetition week runs fewer sets than this and
    /// the chart is what decides. Five is where the operator's own block sat and
    /// is what the lighter weeks come out at anyway.
    pub const ACCUMULATION_SETS: u32 = 5;
    /// The repetition count accumulation descends to.
    ///
    /// Two, because a double is the smallest set Prilepin's chart will put in a
    /// many-set week and the load it belongs at is the chart's own boundary. A
    /// phase descending past it would leave the band that admits it.
    pub const ACCUMULATION_FLOOR_REPS: u32 = 2;
    /// Where the block plans to finish, as a share of the entry one-rep maximum.
    pub const ENDPOINT: i32 = 10_500;

    /// # Errors
    ///
    /// [`InvalidBlock`] for a block too short to hold all three phases, or one
    /// so long that its repetition ladder leaves the table.
    pub fn new(duration_weeks: u32) -> Result<Self, InvalidBlock> {
        if duration_weeks < Self::MINIMUM_WEEKS {
            return Err(InvalidBlock::TooShort {
                weeks: duration_weeks,
            });
        }
        let block = Self { duration_weeks };
        // Checked here so an unplannable block fails at authoring rather than at
        // the first `prescribe`.
        if !block.top_set_ladder_is_liftable() {
            return Err(InvalidBlock::TooLong {
                weeks: duration_weeks,
            });
        }
        Ok(block)
    }

    /// Weeks of phases, which is every week the block occupies.
    ///
    /// The entry test sits outside it and so does not lengthen it: what the
    /// operator's table counts and what [`Self::weeks`] returns are the same
    /// number (decision 0013).
    pub const fn duration_weeks(self) -> u32 {
        self.duration_weeks
    }

    /// Weeks of accumulation.
    pub const fn accumulation_weeks(self) -> u32 {
        self.split().0
    }

    /// Weeks of intensification.
    pub const fn intensification_weeks(self) -> u32 {
        self.split().1
    }

    /// Weeks of realisation, the last of which is the exit test.
    pub const fn realisation_weeks(self) -> u32 {
        self.split().2
    }

    /// The split, as one rule.
    ///
    /// Eight weeks is 3-3-2 and each week beyond it goes to accumulation, then
    /// intensification, then realisation, in rotation. The operator's four rows
    /// come out of it — 8: 3-3-2, 9: 4-3-2, 10: 4-4-2, 11: 4-4-3 — and so does
    /// every duration they did not tabulate.
    const fn split(self) -> (u32, u32, u32) {
        let extra = self.duration_weeks - Self::MINIMUM_WEEKS;
        // The three phases take turns, accumulation first. A whole rotation goes
        // to all three; what is left over goes to the front of the order.
        let rotations = extra / 3;
        let over = extra % 3;
        (
            SHORTEST.0 + rotations + if over >= 1 { 1 } else { 0 },
            SHORTEST.1 + rotations + if over >= 2 { 1 } else { 0 },
            SHORTEST.2 + rotations,
        )
    }

    /// The rungs the single top set runs over: intensification and realisation
    /// together, because one ladder runs through both.
    const fn top_set_weeks(self) -> u32 {
        self.intensification_weeks() + self.realisation_weeks()
    }

    /// What the block finishes at, as a share of the entry one-rep maximum.
    ///
    /// 105%, whatever the duration and whatever the entry test measured. The
    /// exit test is a single, so this needs no conversion to be read: the block
    /// plans to add five percent to the maximum it started from.
    #[must_use]
    pub fn endpoint(self) -> Option<Percentage> {
        Percentage::from_basis_points(Self::ENDPOINT).ok()
    }

    /// What each week prescribes.
    #[must_use]
    pub fn weeks(self) -> Vec<WeekPlan> {
        (1..=self.duration_weeks())
            .filter_map(|week| WeekIndex::new(week).ok().and_then(|week| self.week(week)))
            .collect()
    }

    /// What one week prescribes, or `None` for a week past the block.
    #[must_use]
    pub fn week(self, week: WeekIndex) -> Option<WeekPlan> {
        let rung = week.as_u32();
        if rung > self.duration_weeks() {
            return None;
        }
        if rung <= self.accumulation_weeks() {
            return self.accumulation(rung);
        }
        self.top_set(rung - self.accumulation_weeks())
    }

    /// Which kind of week this is, in the vocabulary the calendar and the store
    /// already speak.
    #[must_use]
    pub fn kind(self, week: WeekIndex) -> Option<WeekKind> {
        match self.week(week)? {
            WeekPlan::ExitTest { .. } => Some(WeekKind::Test),
            WeekPlan::Working { .. } => Some(WeekKind::Climbing(week)),
        }
    }

    /// Rung `k` of accumulation, one-based.
    ///
    /// Repetitions descend to two, so the phase's length decides where they
    /// start. The load is pinned at the double and steps down the
    /// repetition-maximum table's slope from there, which is what holds every
    /// week inside Prilepin's band without any reference to effort.
    fn accumulation(self, rung: u32) -> Option<WeekPlan> {
        let reps = RepCount::new(
            self.accumulation_weeks()
                .checked_sub(rung)?
                .checked_add(Self::ACCUMULATION_FLOOR_REPS)?,
        )
        .ok()?;
        // 80%, where the chart stops asking for sets of three to six and starts
        // admitting a double — then one repetition further up the ladder for
        // every 2.5 points down it.
        let pinned = prilepin::floor_for_sets_of(Self::ACCUMULATION_FLOOR_REPS)?;
        let steps =
            i32::try_from(reps.as_u32().checked_sub(Self::ACCUMULATION_FLOOR_REPS)?).ok()?;
        let load = Percentage::from_basis_points(
            pinned
                .as_basis_points()
                .checked_sub(PER_REPETITION.checked_mul(steps)?)?,
        )
        .ok()?;
        // Five across, unless five would put the week over the total lifts
        // Prilepin's band for that load admits — which it does at six and seven
        // repetitions, and which is the chart doing work rather than being cited.
        let sets = prilepin::sets_across(load, reps.as_u32(), Self::ACCUMULATION_SETS);
        Some(WeekPlan::Working {
            phase: Phase::Accumulation,
            sets: RepCount::new(sets).ok()?,
            reps,
            load,
        })
    }

    /// Rung `k` of the single top set, one-based over intensification and
    /// realisation together. The last one is the exit test.
    ///
    /// Repetitions descend to one and the load spans accumulation's exit to the
    /// block's endpoint, both without a break at the phase boundary — so the
    /// maximum each week implies climbs past the entry test on the way, which is
    /// the gain the block is planning. Which phase a rung belongs to is the
    /// split's business, not the ladder's.
    fn top_set(self, rung: u32) -> Option<WeekPlan> {
        let weeks = self.top_set_weeks();
        let reps = RepCount::new(weeks.checked_sub(rung)?.checked_add(1)?).ok()?;
        if rung == weeks {
            return Some(WeekPlan::ExitTest {
                reps,
                expected: self.endpoint()?,
            });
        }
        let phase = if rung <= self.intensification_weeks() {
            Phase::Intensification
        } else {
            Phase::Realisation
        };

        let start = self.accumulation_exit()?.as_basis_points();
        let span = i64::from(self.endpoint()?.as_basis_points().checked_sub(start)?);
        // Multiply before dividing, so the rounding happens once rather than
        // accumulating across the phase.
        let advanced = span
            .checked_mul(i64::from(rung.checked_sub(1)?))?
            .checked_div(i64::from(weeks.checked_sub(1)?))?;
        let points = i64::from(start).checked_add(advanced)?;
        Some(WeekPlan::Working {
            phase,
            sets: RepCount::new(1).ok()?,
            reps,
            load: Percentage::from_basis_points(i32::try_from(points).ok()?).ok()?,
        })
    }

    /// Where accumulation finishes: a double at three in reserve, which is the
    /// same load whatever the block's duration.
    fn accumulation_exit(self) -> Option<Percentage> {
        match self.accumulation(self.accumulation_weeks())? {
            WeekPlan::Working { load, .. } => Some(load),
            WeekPlan::ExitTest { .. } => None,
        }
    }

    /// Whether the top-set ladder opens at a load its own repetition count can
    /// carry.
    ///
    /// **This is what bounds the duration, and the bound is derived rather than
    /// authored.** The ladder opens where accumulation finished and its opening
    /// repetition count is the length of the two phases it spans, so a long
    /// enough block asks for a set of nine, ten, eleven at 80% — and past nine
    /// that is a load above the maximum for the repetition count, which is not a
    /// hard set but an impossible one. The longest block that survives the test
    /// is fifteen weeks, which is also about where the literature stops
    /// describing one block rather than two.
    fn top_set_ladder_is_liftable(self) -> bool {
        let Some(opening) = RepCount::new(self.top_set_weeks())
            .ok()
            .and_then(rep_max)
            .map(Percentage::as_basis_points)
        else {
            return false;
        };
        let Some(start) = self.accumulation_exit().map(Percentage::as_basis_points) else {
            return false;
        };
        start <= opening
    }
}

/// The week a block spends measuring what it is about to plan from.
///
/// **Optional, and prepended rather than counted.** Decision 0013 refused an
/// optional test for the linear template because the test was its *last* week,
/// folded into `duration_weeks` — so `5` meant either five climbing weeks or four
/// and a test. A week put in front of a block forks nothing: `duration_weeks`
/// goes on counting phase weeks whether this is present or not, and the calendar
/// carries one more when it is.
///
/// **The block owns it because the block is what it is for.** A test session has
/// to run the lower slots the way the programme it anchors runs them — testing
/// the front squat makes the hip-dominant slot that day's accessory — so the
/// pattern, the primary and the two lower fills have to agree between the test
/// and the block. Held in one place they cannot disagree; held in two documents
/// they are three facts nothing checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryTest {
    reps: RepCount,
    light: Option<Kg>,
}

impl EntryTest {
    /// # Errors
    ///
    /// [`InvalidBlock::EntryTestTooLong`] for a repetition count the
    /// repetition-maximum table cannot convert into the one-rep maximum the
    /// block's percentages are shares of.
    pub fn new(reps: RepCount, light: Option<Kg>) -> Result<Self, InvalidBlock> {
        if rep_max(reps).is_none() {
            return Err(InvalidBlock::EntryTestTooLong {
                reps: reps.as_u32(),
            });
        }
        Ok(Self { reps, light })
    }

    /// What the attempt is performed at.
    ///
    /// **A triple, by convention.** `block.rs` enters a block on one
    /// deliberately: a cold maximal single measures technique as much as
    /// strength, and a peaked one is what the realisation weeks prepare for. The
    /// block exits on a single whatever it entered on.
    pub const fn reps(self) -> RepCount {
        self.reps
    }

    /// What the week's other session runs its primary at.
    ///
    /// **Authored, and `None` means the session is not run.** There is no honest
    /// derivation here: the lift's maximum is what Friday is about to measure, so
    /// a load taken from the programme before would be a share of a maximum in
    /// possibly a different lift — a number fitted to the record with no decision
    /// behind it. Stated, the way `opening` is stated when the derivation has
    /// nothing to work from.
    pub const fn light(self) -> Option<Kg> {
        self.light
    }
}

/// What one week of a block is, entry test included.
///
/// [`Block`] plans phase weeks and knows nothing about an entry test; this is the
/// programme's view, where week one may be the measurement the rest is a share
/// of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockWeek {
    Entry(EntryTest),
    Planned(WeekPlan),
}

/// Weeks of phases: every week the calendar holds, less the entry test's.
const fn phase_weeks_of(calendar: &Calendar, entry_test: Option<EntryTest>) -> u32 {
    calendar
        .duration_weeks()
        .saturating_sub(if entry_test.is_some() { 1 } else { 0 })
}

/// A programme written against the periodised block.
///
/// **The plan above is not a programme.** [`Block`] is a pure function of a
/// duration — which weeks accumulate, which intensify, what share of the maximum
/// each asks for — and says nothing about who is lifting, what fills the other
/// sixteen slots, or when any of it happens. This carries that, exactly as
/// [`Linear`] carries it for the other model, so the two are interchangeable
/// wherever a caller only wants a programme.
///
/// **The plan is derived rather than stored**, from the calendar's own duration.
/// One number, one meaning: a block whose stored plan disagreed with its
/// calendar would prescribe one thing and be reported as another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Periodised {
    name: ProgrammeName,
    primary: Primary,
    fills: SlotFills,
    /// The maximum every load in this block is a share of.
    ///
    /// **An expectation where the block tests, a measurement where it does
    /// not.** A block that runs its own entry test is authored before the number
    /// is known: the anchor says what the operator expects to lift, the test
    /// week confirms it, and a result that differs is answered by re-authoring
    /// the block — which decision 0012 makes a supersession rather than a second
    /// programme. A block *without* an entry test has no such opportunity, so
    /// its anchor must already have been tested.
    ///
    /// The opening feeds nothing here: a block's loads are shares of the anchor
    /// rather than rungs off an opening. It is carried because [`Entry`] travels
    /// whole.
    entry: Entry,
    /// The week in front of the phases, where this block measures its own anchor.
    entry_test: Option<EntryTest>,
    /// **Every week the block occupies**, the entry test's included. What the
    /// operator's table counts is [`Self::phase_weeks`].
    calendar: Calendar,
    authored_at: Timestamp,
}

impl Periodised {
    /// Build, running the checks the type system cannot.
    ///
    /// # Errors
    ///
    /// [`InconsistentProgramme`] for a gating role the programme never runs, a
    /// primary that cannot carry a top set, a primary that does not fill its own
    /// slot, an entry test that does not precede the block, an anchor that was
    /// not tested, or a duration that does not make a block.
    pub fn new(
        name: ProgrammeName,
        primary: Primary,
        fills: SlotFills,
        entry: Entry,
        entry_test: Option<EntryTest>,
        calendar: Calendar,
    ) -> Result<Self, InconsistentProgramme> {
        Self::check(primary, &fills, entry, entry_test, &calendar)?;
        Ok(Self {
            name,
            primary,
            fills,
            entry,
            entry_test,
            calendar,
            authored_at: Timestamp::now(),
        })
    }

    /// The calendar a block occupies, from the weeks its phases take.
    ///
    /// **`phase_weeks` is what the operator's table counts** — 8 as 3-3-2, 9 as
    /// 4-3-2 — and an entry test adds one in front. Built here so a document
    /// states one number and cannot state the other: a `duration_weeks` that
    /// sometimes included the test is the fork decision 0013 removed from the
    /// linear template, and it is not coming back through this door.
    ///
    /// # Errors
    ///
    /// [`InvalidCalendar`] for a block of no weeks, or for an interruption
    /// outside it.
    pub fn weeks(
        start: Date,
        phase_weeks: u32,
        entry_test: bool,
        interruptions: &[Skip],
        weekdays: Weekdays,
        zone: TimeZone,
    ) -> Result<Calendar, InvalidCalendar> {
        let weeks = phase_weeks.saturating_add(u32::from(entry_test));
        Calendar::new(start, weeks, interruptions, weekdays, zone)
    }

    /// Rebuild a block that was already authored.
    ///
    /// **Re-runs every check, unlike [`Linear::rehydrate`].** Linear holds one
    /// back because its ladder check asks a question about the parameters in
    /// force rather than about the programme; a block's plan is a function of its
    /// duration alone, so there is no such parameter and nothing to hold back.
    ///
    /// # Errors
    ///
    /// As [`Self::new`].
    pub fn rehydrate(
        name: ProgrammeName,
        primary: Primary,
        fills: SlotFills,
        entry: Entry,
        entry_test: Option<EntryTest>,
        calendar: Calendar,
        authored_at: Timestamp,
    ) -> Result<Self, InconsistentProgramme> {
        Self::check(primary, &fills, entry, entry_test, &calendar)?;
        Ok(Self {
            name,
            primary,
            fills,
            entry,
            entry_test,
            calendar,
            authored_at,
        })
    }

    fn check(
        primary: Primary,
        fills: &SlotFills,
        entry: Entry,
        entry_test: Option<EntryTest>,
        calendar: &Calendar,
    ) -> Result<(), InconsistentProgramme> {
        if !calendar.weekdays().runs(primary.gating_role()) {
            return Err(InconsistentProgramme::GatingRoleNeverRuns {
                gating: primary.gating_role(),
            });
        }
        check_primary(
            primary.pattern(),
            primary.exercise(),
            fills,
            primary.gating_role(),
        )?;

        // The entry test precedes the block it anchors — 0009's rule, which 0013
        // keeps and makes the weaker half of a stronger one.
        if entry.anchor().from() >= calendar.start() {
            return Err(InconsistentProgramme::EntryTestIsNotBeforeTheBlock {
                start: calendar.start(),
                tested: entry.anchor().from(),
            });
        }

        // **Whether this anchor had to be measured is not a question this type
        // can answer.** A block opens from a test where one precedes it, runs
        // its own where none is usable, and may state a number outright only
        // where nothing precedes it at all — and which of those is the case is a
        // fact about the store. So the rule lives in `Authoring`, and what is
        // left here is everything decidable from the programme alone.

        // The phases have to make a block over what is left of the calendar
        // once the entry test has taken its week. Checked here so an unplannable
        // block fails at authoring rather than at the first `prescribe`.
        Block::new(phase_weeks_of(calendar, entry_test))?;
        Ok(())
    }

    /// The block's plan.
    ///
    /// # Errors
    ///
    /// [`InvalidBlock`] never, in practice: both constructors have already
    /// proved it succeeds for the duration this calendar holds.
    pub fn plan(&self) -> Result<Block, InvalidBlock> {
        Block::new(self.phase_weeks())
    }

    /// Weeks of phases: what the operator's table counts.
    #[must_use]
    pub const fn phase_weeks(&self) -> u32 {
        phase_weeks_of(&self.calendar, self.entry_test)
    }

    /// The week in front of the phases, where there is one.
    pub const fn entry_test(&self) -> Option<EntryTest> {
        self.entry_test
    }

    /// What one week of this block is.
    ///
    /// **The entry test takes week one and the phases shift behind it**, which
    /// is why this exists rather than callers asking [`Block`] directly: `Block`
    /// plans phase weeks and has no idea a measurement week might sit in front
    /// of them.
    #[must_use]
    pub fn week(&self, week: WeekIndex) -> Option<BlockWeek> {
        let offset = match self.entry_test {
            Some(test) => {
                if week.as_u32() == 1 {
                    return Some(BlockWeek::Entry(test));
                }
                1
            }
            None => 0,
        };
        let rung = WeekIndex::new(week.as_u32().checked_sub(offset)?).ok()?;
        self.plan().ok()?.week(rung).map(BlockWeek::Planned)
    }

    /// Which kind of week this is, in the vocabulary the calendar and the store
    /// already speak.
    #[must_use]
    pub fn kind(&self, week: WeekIndex) -> Option<WeekKind> {
        match self.week(week)? {
            // Both tests, and both recorded as such: the week the block measures
            // its anchor and the week it spends it.
            BlockWeek::Entry(_) | BlockWeek::Planned(WeekPlan::ExitTest { .. }) => {
                Some(WeekKind::Test)
            }
            BlockWeek::Planned(WeekPlan::Working { .. }) => Some(WeekKind::Climbing(week)),
        }
    }

    pub const fn name(&self) -> &ProgrammeName {
        &self.name
    }

    pub const fn primary(&self) -> PrimaryPattern {
        self.primary.pattern()
    }

    pub const fn primary_exercise(&self) -> Exercise {
        self.primary.exercise()
    }

    pub const fn gating_role(&self) -> SessionRole {
        self.primary.gating_role()
    }

    pub const fn fills(&self) -> &SlotFills {
        &self.fills
    }

    pub const fn entry(&self) -> Entry {
        self.entry
    }

    pub const fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    pub const fn authored_at(&self) -> Timestamp {
        self.authored_at
    }

    /// The days this block occupies, for the rule that two programmes may not
    /// compete for one of them.
    #[must_use]
    pub fn window(&self) -> ProgrammeWindow {
        ProgrammeWindow::new(
            self.name.clone(),
            self.calendar.start(),
            self.calendar.calendar_weeks(),
        )
    }

    /// Whether this slot is the primary one.
    #[must_use]
    pub fn is_primary(&self, slot: crate::prescription::shape::SlotId) -> bool {
        self.primary.pattern().slot() == slot
    }
}
