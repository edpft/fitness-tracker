//! What happens when the plan turns out to have been too ambitious.
//!
//! **This is the failure mechanism, and it is deliberately not the plan.**
//! `docs/primary-lift-progression.md` holds both and says why they must not be
//! conflated: the plan is a ladder generated from a duration and a starting 1RM,
//! and this takes over when it fails. Neither is derived from the other, and the
//! plan is not designed around the possibility of failing. [`super::ladder`] is
//! the plan, and its module note points here.
//!
//! ```text
//! a miss                          hold, and re-issue the same loads
//! a second miss at the same load  a stall: suspend the ladder and drop back
//! the re-climb reaching the load  resume the ladder where it was suspended
//! ```
//!
//! **A block whose entry test found a ceiling opens re-climbing, and that costs
//! no stall.** The test failed a load; the block opens at that load and reaches
//! it the same way a stall reaches it, at the first reset's drop and rate. It is
//! [`ClimbBack::Entry`] rather than [`ClimbBack::Reset`] precisely so it cannot
//! be counted: the operator settled on 2026-08-19 that the next failure still
//! gets both resets. See
//! `docs/decisions/0009-a-linear-block-opens-from-its-entry-test.md`.
//!
//! **The anchor is not a parameter of anything here, which is FR-021 held by the
//! signature.** A reset takes its drop from the *failed load*, re-climbs at its
//! own rate, and resumes the ladder untouched. The anchor is a measurement of
//! where the block started and a stall is not evidence about that, so there is no
//! way to write code here that moves it — § 24 rather than a rule to remember.
//! [`progress_after`] takes the entry test's *failed* load, which is one number
//! out of the anchor and not the anchor: it seeds the opening state and nothing
//! can write back to it.
//!
//! **No effort report is read.** A [`GatingTopSet`] carries what was on the bar
//! and whether it went up, and nothing else. That is the model of record's
//! position rather than an omission: on lower-body lifts the discriminable states
//! are coarse, so a gate reading a finer signal does not autoregulate — it
//! introduces a decision point resolved by mood.
//!
//! **An absence is not a miss.** A session nobody trained contributes no
//! [`GatingTopSet`], so the ladder holds and no stall accrues, which is what the
//! spec's edge case asks for and needs no code of its own.
//!
//! **Resuming clears the stall.** After a re-climb returns to the failed load, the
//! next miss there is a first miss again. That is what makes the worked example's
//! two stalls two stalls rather than one escalating immediately, and it is why the
//! failed-load memory is per-stall rather than per-block.

use crate::gym::Kg;

use super::{
    ladder::Ladder,
    parameters::{PlateIncrement, ResetProtocol},
    quantise::quantise_loaded,
    schedule::WeekIndex,
};

/// One gating session's top set, as the progression reads it.
///
/// Two facts, because two is all the mechanism uses. Which session is gating is
/// the programme's business (`gating_role`), and a non-gating session's top set
/// never reaches this — a miss there leaves the ladder alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatingTopSet {
    pub load: Kg,
    /// Whether the set was completed. A failed attempt is `false`; an absent
    /// session is not one of these at all.
    pub completed: bool,
}

/// Which reset is in play.
///
/// Two, and there is deliberately no third. Within an eleven-week ceiling three
/// stalls do not fit before a test intervenes, so `docs/primary-lift-progression.md`
/// leaves the case undecided — and this holds at the failed load rather than
/// inventing a protocol nobody chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reset {
    First,
    Second,
}

impl Reset {
    /// The stable key. Persisted and printed.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::Second => "second",
        }
    }
}

impl std::fmt::Display for Reset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why the primary is climbing back to a load rather than following the plan.
///
/// **Two reasons, and only one of them spends a stall.** They run identically —
/// same drop, same rate, same resumption — so the distinction exists to stop the
/// entry being counted as a reset, and to stop a report calling it one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClimbBack {
    /// The block's opening. Its entry test failed a load, and the block climbs to
    /// that load before the plan proper begins. Spends no stall: the next failure
    /// is still the first reset.
    Entry,
    /// A stall inside the block.
    Reset(Reset),
}

impl ClimbBack {
    /// The protocol this climb runs at.
    ///
    /// The entry runs at the first reset's, because it is a first failure — the
    /// second reset is defined as the slowdown *after* one.
    const fn protocol(self, first: ResetProtocol, second: ResetProtocol) -> ResetProtocol {
        match self {
            Self::Entry | Self::Reset(Reset::First) => first,
            Self::Reset(Reset::Second) => second,
        }
    }
}

impl std::fmt::Display for ClimbBack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Entry => f.write_str("entry"),
            Self::Reset(reset) => write!(f, "{reset} reset"),
        }
    }
}

/// Where the primary's progression has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// On the plan, at this climbing week.
    Climbing { week: WeekIndex },
    /// Climbing back toward a load that was failed, at that climb's own rate, and
    /// the ladder resumes at the week it was suspended at when the re-climb
    /// arrives. Either a stall inside the block or the block's own opening.
    ReClimbing {
        climb: ClimbBack,
        /// What to prescribe now.
        load: Kg,
        /// The load that was failed, and where the ladder waits.
        toward: Kg,
        resuming_at: WeekIndex,
    },
}

impl Progress {
    /// The heavy session's top set this state prescribes.
    ///
    /// `None` only where the ladder has none — the block's test week, which is not
    /// a position and carries no load. A re-climb always has one, because it is a
    /// load rather than a position.
    #[must_use]
    pub fn heavy_top_set(self, ladder: Ladder, increment: PlateIncrement) -> Option<Kg> {
        match self {
            Self::Climbing { week } => ladder.heavy_top_set(week, increment),
            Self::ReClimbing { load, .. } => Some(load),
        }
    }

    /// The light session's top set: a proportion of the same state's heavy one.
    ///
    /// Derived from the heavy load rather than from the anchor, exactly as
    /// [`Ladder::light_top_set`] does — so the two roles move together whether the
    /// week's load came from the plan or from a re-climb.
    #[must_use]
    pub fn light_top_set(
        self,
        ladder: Ladder,
        increment: PlateIncrement,
        light_of_heavy: super::parameters::Percentage,
    ) -> Option<Kg> {
        self.heavy_top_set(ladder, increment)
            .map(|heavy| quantise_loaded(light_of_heavy.of(heavy), increment))
    }

    /// Which climbing week the plan is at, or would resume at.
    #[must_use]
    pub const fn week(self) -> WeekIndex {
        match self {
            Self::Climbing { week }
            | Self::ReClimbing {
                resuming_at: week, ..
            } => week,
        }
    }

    /// Which reset is in play, if the ladder is suspended by one.
    ///
    /// `None` for the block's entry climb, which is not a reset and must not be
    /// reported or counted as one.
    #[must_use]
    pub const fn reset(self) -> Option<Reset> {
        match self {
            Self::Climbing { .. }
            | Self::ReClimbing {
                climb: ClimbBack::Entry,
                ..
            } => None,
            Self::ReClimbing {
                climb: ClimbBack::Reset(reset),
                ..
            } => Some(reset),
        }
    }

    /// Why the primary is climbing back, if it is.
    #[must_use]
    pub const fn climb_back(self) -> Option<ClimbBack> {
        match self {
            Self::Climbing { .. } => None,
            Self::ReClimbing { climb, .. } => Some(climb),
        }
    }
}

/// Walk the gating sessions and say where the progression stands.
///
/// **Derived on every read, never stored.** A stored position is a second source
/// of truth about a series the record already determines, and asking twice would
/// then advance it twice. Nothing here reads a clock, a store or a previous
/// answer.
///
/// **No anchor**, which is the point: see the module note. `entry_failed` is the
/// one number taken out of it — what the entry test failed, if it found a
/// ceiling — and it seeds the opening state without anything being able to write
/// back to it.
#[must_use]
pub fn progress_after(
    gating: &[GatingTopSet],
    entry_failed: Option<Kg>,
    first: ResetProtocol,
    second: ResetProtocol,
    increment: PlateIncrement,
) -> Progress {
    // A block whose entry test found a ceiling opens climbing back to it. The
    // ladder's first week *is* that load, so this arrives exactly where the plan
    // was going to start — and `stalls` stays at zero, which is what makes the
    // next failure the first reset rather than the second.
    let mut progress = entry_failed.map_or(
        Progress::Climbing {
            week: WeekIndex::FIRST,
        },
        |failed| begin(ClimbBack::Entry, failed, first, increment, WeekIndex::FIRST),
    );
    // **How many stalls the block has had, which is not the same as what state it
    // is in.** A reset that completes puts the plan back in charge, so the state
    // says nothing about whether a stall has already been spent — and the next
    // stall is the *second*, at the slower protocol. Deciding the protocol from
    // the state instead is a bug the worked example catches: it escalates to reset
    // one twice and never reaches reset two.
    let mut stalls = 0_u32;
    // The load a miss has already been recorded at, and only the *current* stall's.
    // Cleared whenever a set goes up, which is what makes a second miss mean "at
    // the same load, consecutively" rather than "ever before in this block".
    let mut missed_at: Option<Kg> = None;

    for set in gating {
        if set.completed {
            missed_at = None;
            progress = advance(progress, increment, first, second);
            continue;
        }

        let stalling = missed_at == Some(set.load);
        missed_at = Some(set.load);
        if !stalling {
            // A first miss holds: the same loads are re-issued next week.
            continue;
        }

        missed_at = None;
        // The week to come back to is where the plan stands, whether the stall
        // interrupted the plan itself or a re-climb of it.
        let suspended = progress.week();
        progress = match stalls {
            0 => begin(
                ClimbBack::Reset(Reset::First),
                set.load,
                first,
                increment,
                suspended,
            ),
            1 => begin(
                ClimbBack::Reset(Reset::Second),
                set.load,
                second,
                increment,
                suspended,
            ),
            // A third stall, which the model of record leaves undecided: within an
            // eleven-week ceiling three do not fit before a test intervenes.
            // Holding is the absence of a protocol rather than a third one, and it
            // keeps the state legible until there is a decision to encode.
            _ => progress,
        };
        stalls = stalls.saturating_add(1);
    }

    progress
}

/// A completed set: advance the plan, or the re-climb.
const fn advance(
    progress: Progress,
    increment: PlateIncrement,
    first: ResetProtocol,
    second: ResetProtocol,
) -> Progress {
    let Progress::ReClimbing {
        climb,
        load,
        toward,
        resuming_at,
    } = progress
    else {
        let Progress::Climbing { week } = progress else {
            return progress;
        };
        return Progress::Climbing { week: week.next() };
    };

    let rate = climb.protocol(first, second).reclimb_per_week;
    let next = quantise_loaded(
        Kg::from_grams(load.as_grams().saturating_add(rate.as_grams())),
        increment,
    );
    if next.as_grams() >= toward.as_grams() {
        // The re-climb has arrived, so the plan takes over again at the week it
        // was suspended at — whose load is `toward`.
        Progress::Climbing { week: resuming_at }
    } else {
        Progress::ReClimbing {
            climb,
            load: next,
            toward,
            resuming_at,
        }
    }
}

/// Drop back from the load that was failed, and climb to it.
fn begin(
    climb: ClimbBack,
    failed: Kg,
    protocol: ResetProtocol,
    increment: PlateIncrement,
    resuming_at: WeekIndex,
) -> Progress {
    Progress::ReClimbing {
        climb,
        // The drop is applied to the failed load. `applied_to` adds a negative
        // proportion to the whole, so −10% of 90kg is 81kg, and the plate grid
        // takes it to 80.
        load: quantise_loaded(protocol.drop.applied_to(failed), increment),
        toward: failed,
        resuming_at,
    }
}
