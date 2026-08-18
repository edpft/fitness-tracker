//! The block: what each training week of a periodised programme prescribes.
//!
//! **Three inputs, and every load in the block comes out of them**: how many
//! weeks the calendar allows, which repetition maximum the block is for, and the
//! entry test that anchors it. Nothing else is authored. Research D11 has the
//! derivation and its sources; what follows is what a reader of the code needs
//! in order not to undo it.
//!
//! ```text
//! week 1        entry test at the target repetition count
//! weeks 2..     accumulation    — five sets across, reps descending, load rising
//!    ..N        intensification — one top set,      reps descending, load rising
//! week N        the last intensification week IS the exit test
//! ```
//!
//! **The two phases are loaded by different rules because they are doing
//! different things**, and neither rule is a parameter:
//!
//! - Accumulation runs many sets, so no set can be a maximum. It sits a constant
//!   three repetitions in reserve below one, which is what puts every rung
//!   inside the band Prilepin's chart admits for that week's total number of
//!   lifts — and the three-rep rung exactly on the chart's optimum.
//! - Intensification runs a single top set climbing to the block's endpoint,
//!   which is **105% of the entry maximum**: the figure the Russian squat
//!   routine, Arbic's block programme and meet-attempt convention all land on.
//!
//! **105% is a convention rather than a measurement**, and is recorded as one.
//! Its authority is that it is shared — this block can be read against every
//! published block that also finishes at 105% — and it is falsifiable against
//! the operator's own exit tests, which a number chosen here would not be.
//!
//! **Duration changes where the block starts and never where it finishes.** A
//! seven-week block climbs the same span in three rungs that an eleven-week
//! block climbs in five. That is the same property the `v1` ladder has, and the
//! reason a duration is an input rather than a parameter.

use crate::gym::RepCount;

use crate::prescription::{
    parameters::Percentage,
    prilepin,
    repmax::{PER_REPETITION_IN_RESERVE, rep_max},
    schedule::{WeekIndex, WeekKind},
};

/// Why a block could not be planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidBlock {
    #[error(
        "a block needs an entry test and at least three weeks of each phase, \
         so {weeks} weeks is too few for one"
    )]
    TooShort { weeks: u32 },
    #[error(
        "a {target}-repetition maximum is too many repetitions to plan a block \
         around"
    )]
    TargetTooHigh { target: u32 },
}

/// Which half of the block a week belongs to.
///
/// The tests are not a phase. Week 1 establishes the anchor and week N spends
/// it, and neither is a rung — which [`WeekKind`] already says for the exit and
/// this says for the entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Accumulation,
    Intensification,
}

impl Phase {
    /// The stable key. Persisted, so it outlives a rename.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accumulation => "accumulation",
            Self::Intensification => "intensification",
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeekPlan {
    EntryTest {
        reps: RepCount,
    },
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
    target: RepCount,
}

impl Block {
    /// One entry test and three weeks of each phase.
    pub const MINIMUM_WEEKS: u32 = 7;
    /// The most sets across accumulation will run.
    ///
    /// A ceiling rather than a count: Prilepin's band for the week's load caps
    /// the total lifts, so a high-repetition week runs fewer sets than this and
    /// the chart is what decides. Five is where the operator's own block sat and
    /// is what the lighter weeks come out at anyway.
    pub const ACCUMULATION_SETS: u32 = 5;
    /// Repetitions in reserve on every accumulation set.
    pub const ACCUMULATION_RESERVE: i32 = 3;
    /// Where the block plans to finish, as a share of the entry maximum.
    pub const ENDPOINT: i32 = 10_500;

    /// # Errors
    ///
    /// [`InvalidBlock`] for a block too short to hold both phases, or a target
    /// repetition count so high that its ladder leaves the table.
    pub fn new(duration_weeks: u32, target: RepCount) -> Result<Self, InvalidBlock> {
        if duration_weeks < Self::MINIMUM_WEEKS {
            return Err(InvalidBlock::TooShort {
                weeks: duration_weeks,
            });
        }
        let block = Self {
            duration_weeks,
            target,
        };
        // The longest rung of either ladder has to sit on the table, and the
        // endpoint has to be derivable. Checked here so an unplannable block
        // fails at authoring rather than at the first `prescribe`.
        if block.endpoint().is_none() || block.longest_rung().is_none() {
            return Err(InvalidBlock::TargetTooHigh {
                target: target.as_u32(),
            });
        }
        Ok(block)
    }

    pub const fn duration_weeks(self) -> u32 {
        self.duration_weeks
    }

    pub const fn target(self) -> RepCount {
        self.target
    }

    /// Weeks of intensification. **It drops first**, so an odd number of phase
    /// weeks gives the extra one to accumulation.
    pub const fn intensification_weeks(self) -> u32 {
        (self.duration_weeks - 1) / 2
    }

    /// Weeks of accumulation.
    pub const fn accumulation_weeks(self) -> u32 {
        (self.duration_weeks - 1) - self.intensification_weeks()
    }

    /// What the block finishes at, as a share of the entry maximum: 105% of the
    /// maximum, expressed at the target repetition count.
    ///
    /// For a three-rep target that is a little under the entry one-rep maximum,
    /// which is the whole claim the block makes and is small enough to hold in
    /// your head.
    #[must_use]
    pub fn endpoint(self) -> Option<Percentage> {
        let at_target = rep_max(self.target)?.as_basis_points();
        let scaled = i64::from(at_target)
            .checked_mul(i64::from(Self::ENDPOINT))?
            .checked_div(i64::from(Percentage::WHOLE.as_basis_points()))?;
        Percentage::from_basis_points(i32::try_from(scaled).ok()?).ok()
    }

    /// What each week prescribes.
    #[must_use]
    pub fn weeks(self) -> Vec<WeekPlan> {
        (1..=self.duration_weeks)
            .filter_map(|week| WeekIndex::new(week).ok().and_then(|week| self.week(week)))
            .collect()
    }

    /// What one week prescribes, or `None` for a week past the block.
    #[must_use]
    pub fn week(self, week: WeekIndex) -> Option<WeekPlan> {
        if week.as_u32() > self.duration_weeks {
            return None;
        }
        if week.as_u32() == 1 {
            return Some(WeekPlan::EntryTest { reps: self.target });
        }
        let rung = week.as_u32() - 1;
        if rung <= self.accumulation_weeks() {
            return self.accumulation(rung);
        }
        self.intensification(rung - self.accumulation_weeks())
    }

    /// Which kind of week this is, in the vocabulary the calendar and the store
    /// already speak.
    #[must_use]
    pub fn kind(self, week: WeekIndex) -> Option<WeekKind> {
        match self.week(week)? {
            WeekPlan::EntryTest { .. } | WeekPlan::ExitTest { .. } => Some(WeekKind::Test),
            WeekPlan::Working { .. } => Some(WeekKind::Climbing(week)),
        }
    }

    /// Rung `k` of accumulation, one-based.
    ///
    /// Repetitions descend to two, so the phase's length decides where they
    /// start. The load is a constant distance below the maximum for that
    /// repetition count, which is what holds every week inside Prilepin's band.
    fn accumulation(self, rung: u32) -> Option<WeekPlan> {
        let reps = RepCount::new(self.accumulation_weeks().checked_sub(rung)? + 2).ok()?;
        let below = PER_REPETITION_IN_RESERVE.checked_mul(Self::ACCUMULATION_RESERVE)?;
        let load =
            Percentage::from_basis_points(rep_max(reps)?.as_basis_points().checked_sub(below)?)
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

    /// Rung `k` of intensification, one-based. The last one is the exit test.
    ///
    /// Repetitions descend to the target, and the load spans accumulation's exit
    /// to the block's endpoint — so the maximum each week implies climbs past
    /// the entry test on the way, which is the gain the block is planning.
    fn intensification(self, rung: u32) -> Option<WeekPlan> {
        let weeks = self.intensification_weeks();
        let reps =
            RepCount::new(self.target.as_u32().checked_add(weeks)?.checked_sub(rung)?).ok()?;
        let endpoint = self.endpoint()?;
        if rung == weeks {
            return Some(WeekPlan::ExitTest {
                reps,
                expected: endpoint,
            });
        }

        let start = self.accumulation_exit()?.as_basis_points();
        let span = i64::from(endpoint.as_basis_points().checked_sub(start)?);
        // Multiply before dividing, so the rounding happens once rather than
        // accumulating across the phase.
        let advanced = span
            .checked_mul(i64::from(rung.checked_sub(1)?))?
            .checked_div(i64::from(weeks.checked_sub(1)?))?;
        let points = i64::from(start).checked_add(advanced)?;
        Some(WeekPlan::Working {
            phase: Phase::Intensification,
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
            WeekPlan::EntryTest { .. } | WeekPlan::ExitTest { .. } => None,
        }
    }

    /// The highest repetition count either ladder reaches, which is the one that
    /// has to sit on the table.
    fn longest_rung(self) -> Option<Percentage> {
        let accumulation = RepCount::new(self.accumulation_weeks() + 1).ok()?;
        let intensification = RepCount::new(
            self.target
                .as_u32()
                .checked_add(self.intensification_weeks())?
                - 1,
        )
        .ok()?;
        rep_max(accumulation).and_then(|_| rep_max(intensification))
    }
}
