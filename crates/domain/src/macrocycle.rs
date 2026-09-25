//! The macrocycle: a school term, and the holiday that ends it (#224).
//!
//! ```text
//! macrocycle → phase → mesocycle → microcycle → session
//! ```
//!
//! **A macrocycle is a term.** The operator, 2026-09-24: *"The autumn
//! macrocycle does peak towards a 1RM and an FTP test, it just doesn't peak
//! towards an event."* It runs from the first week of term to the last week
//! of the Christmas, Easter or Summer holiday after it, and it is named for
//! its term and the year it starts in: autumn 2026.
//!
//! **In weeks, not days.** The school's holidays start and end on any day:
//! Summer 2027 starts on a Tuesday. A microcycle runs Monday to Sunday, so a
//! week the holiday touches belongs to the transition, and the term is the
//! whole weeks between. Autumn 2026 starts on Monday 7 September, and its last week of
//! term is the week of 14 December.
//!
//! **Read from the school calendar, never stored or typed in.** Its name, its
//! dates and its phases all follow from the holidays, which are facts about
//! the world (#181). Past the calendar's reach there is no macrocycle, which
//! is no data rather than no term.
//!
//! **Three phases, and a phase is a level** (the classical periods of
//! Matveyev and Bompa). The transition is the holiday. The competitive phase
//! holds the mesocycle that ends on the last day of term, and the preparatory
//! phase holds whatever fits before it. The macrocycle holds these as
//! constraints, not as mesocycles: each mesocycle is realised when the one
//! before it ends, because only then is its anchor known (#222).

use std::fmt;

use jiff::civil::Date;

use crate::{
    plan::Span,
    schedule::{Holidays, SchoolHoliday, SchoolHolidayKind},
};

/// The school terms, each named for the season it starts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Term {
    Autumn,
    Spring,
    Summer,
}

impl fmt::Display for Term {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Autumn => "autumn",
            Self::Spring => "spring",
            Self::Summer => "summer",
        })
    }
}

/// A macrocycle's phases, in the order they run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    /// Whatever fits before the competitive mesocycle.
    Preparatory,
    /// The mesocycle that ends on the last day of term, in a 1RM and an FTP
    /// test.
    Competitive,
    /// The holiday that ends the macrocycle. It may hold a mesocycle too.
    Transition,
}

impl fmt::Display for Phase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Preparatory => "preparatory",
            Self::Competitive => "competitive",
            Self::Transition => "transition",
        })
    }
}

/// One term and the holiday after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Macrocycle {
    term: Term,
    start: Date,
    holiday: SchoolHoliday,
}

impl Macrocycle {
    /// The macrocycle a date falls in, read from the school calendar.
    ///
    /// `None` wherever the calendar cannot say: past its reach, before the
    /// first holiday it knows ends a term, or where a holiday that might end
    /// the term is one gov.uk has not yet published far enough to name.
    pub fn on(date: Date, holidays: &Holidays) -> Option<Self> {
        let mut previous: Option<&SchoolHoliday> = None;
        for holiday in holidays.school() {
            let term = match holidays.kind(holiday)? {
                SchoolHolidayKind::HalfTerm => continue,
                SchoolHolidayKind::Christmas => Term::Autumn,
                SchoolHolidayKind::Easter => Term::Spring,
                SchoolHolidayKind::Summer => Term::Summer,
            };
            if *holiday.weeks().end() < date {
                previous = Some(holiday);
                continue;
            }
            let start = previous?.weeks().end().tomorrow().ok()?;
            return Some(Self {
                term,
                start,
                holiday: *holiday,
            });
        }
        None
    }

    pub const fn term(&self) -> Term {
        self.term
    }

    /// The Monday of the first week of term.
    pub const fn start(&self) -> Date {
        self.start
    }

    /// The holiday that ends it.
    pub const fn holiday(&self) -> SchoolHoliday {
        self.holiday
    }

    /// The Monday of the first week the holiday touches, which the
    /// competitive mesocycle ends the day before.
    pub fn transition(&self) -> Date {
        *self.holiday.weeks().start()
    }

    /// The Sunday of the last week of term.
    pub fn last_day_of_term(&self) -> Date {
        self.transition().yesterday().unwrap_or(self.start)
    }

    /// The Sunday of the last week the holiday touches, and so the last day
    /// of the macrocycle.
    pub fn last(&self) -> Date {
        *self.holiday.weeks().end()
    }

    /// Which phase a date falls in, given the mesocycle running on it.
    ///
    /// **Competitive only for a mesocycle that ends on the last day of term.**
    /// One that ends earlier is preparatory, whatever it holds: a macrocycle
    /// whose last mesocycle stops short of the term has not realised its
    /// competitive phase yet.
    pub fn phase(&self, date: Date, mesocycle: Option<Span>) -> Phase {
        let transition = self.transition();
        if date >= transition {
            return Phase::Transition;
        }
        let competitive = mesocycle
            .is_some_and(|mesocycle| mesocycle.covers(date) && mesocycle.end() == transition);
        if competitive {
            Phase::Competitive
        } else {
            Phase::Preparatory
        }
    }
}

impl fmt::Display for Macrocycle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}", self.term, self.start.year())
    }
}

/// Which chain of published cycling mesocycles a plan follows (#222).
///
/// The two the wizard offers. **Stated once for the term** by the operator,
/// 2026-09-25, and followed until a disruption means it no longer fits.
/// Nothing here ranks the chains against each other: which one a term follows
/// is his choice every term.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Chain {
    /// Base 1 → Base 2 → Build.
    BaseThenBuild,
    /// Build → Peak 1 → Peak 2.
    BuildThenPeak,
}

impl Chain {
    /// Its mesocycles, in the order they are run.
    pub const fn mesocycles(self) -> [Cycling; 3] {
        match self {
            Self::BaseThenBuild => [Cycling::Base1, Cycling::Base2, Cycling::Build],
            Self::BuildThenPeak => [Cycling::Build, Cycling::Peak1, Cycling::Peak2],
        }
    }

    /// The stable key. Persisted.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BaseThenBuild => "base-then-build",
            Self::BuildThenPeak => "build-then-peak",
        }
    }

    /// Whether a piece of this chain fills exactly `weeks` and ends in a peak.
    ///
    /// A piece starts where the chain can start after a test (Base 1, Build or
    /// Peak 1) and runs in order to a mesocycle that can end a term.
    fn fills(self, weeks: u32) -> bool {
        let mesocycles = self.mesocycles();
        mesocycles.iter().enumerate().any(|(first, one)| {
            one.may_follow(Some(Cycling::Hold { holding: 0 }))
                && mesocycles
                    .iter()
                    .enumerate()
                    .skip(first)
                    .scan(0_u32, |total, (_, next)| {
                        *total = total.saturating_add(next.weeks());
                        Some((*total, *next))
                    })
                    .any(|(total, last)| total == weeks && last.competitive())
        })
    }

    /// The mesocycle after one of its own, if it has one.
    fn after(self, previous: Cycling) -> Option<Cycling> {
        let mesocycles = self.mesocycles();
        let at = mesocycles.iter().position(|one| *one == previous)?;
        mesocycles.get(at.checked_add(1)?).copied()
    }
}

impl fmt::Display for Chain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::BaseThenBuild => "Base → Build",
            Self::BuildThenPeak => "Build → Peak",
        })
    }
}

/// A chain's key that names no chain.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a chain: expected base-then-build or build-then-peak")]
pub struct UnknownChain(pub String);

impl std::str::FromStr for Chain {
    type Err = UnknownChain;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "base-then-build" => Ok(Self::BaseThenBuild),
            "build-then-peak" => Ok(Self::BuildThenPeak),
            other => Err(UnknownChain(other.to_owned())),
        }
    }
}

/// What a cycling mesocycle is, as a macrocycle chooses between them.
///
/// **A hold is a mesocycle that ends in a test** (the operator, 2026-09-25):
/// zero or more holding microcycles, then a test microcycle, which is a
/// holding microcycle with its harder session replaced by the FTP test. The
/// standalone test week is a hold with nothing held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cycling {
    Hold {
        holding: u32,
    },
    Base1,
    /// Ours ends in an FTP test where the published one does not: the last
    /// microcycle's shorter, more intense ride is replaced by it.
    Base2,
    Build,
    Peak1,
    Peak2,
}

impl Cycling {
    /// How many microcycles it runs.
    pub const fn weeks(self) -> u32 {
        match self {
            Self::Hold { holding } => holding.saturating_add(1),
            Self::Base1 | Self::Base2 | Self::Build | Self::Peak1 | Self::Peak2 => 4,
        }
    }

    /// Whether it ends in an FTP test, and so is an entry test for what
    /// follows. Base 1 and Peak 1 do not.
    pub const fn ends_in_test(self) -> bool {
        !matches!(self, Self::Base1 | Self::Peak1)
    }

    /// Whether it may follow `previous`, the last mesocycle of this
    /// macrocycle. `None` at the start of one: prerequisites do not carry
    /// across a long holiday.
    ///
    /// Base 1, Build and Peak 1 need an entry test; Base 2 needs Base 1 and
    /// Peak 2 needs Peak 1. A hold needs nothing.
    pub fn may_follow(self, previous: Option<Self>) -> bool {
        match self {
            Self::Hold { .. } => true,
            Self::Base1 | Self::Build | Self::Peak1 => previous.is_some_and(Self::ends_in_test),
            Self::Base2 => previous == Some(Self::Base1),
            Self::Peak2 => previous == Some(Self::Peak1),
        }
    }

    /// Whether, run last, it ends the term in a peak. A hold ends in a test
    /// but is not one: the competitive phase is Build, or the second half of
    /// Base or Peak.
    pub const fn competitive(self) -> bool {
        matches!(self, Self::Base2 | Self::Build | Self::Peak2)
    }
}

impl fmt::Display for Cycling {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hold { holding: 0 } => formatter.write_str("FTP test"),
            Self::Hold { holding } => write!(formatter, "hold ({holding} weeks, then FTP test)"),
            Self::Base1 => formatter.write_str("Base 1"),
            Self::Base2 => formatter.write_str("Base 2"),
            Self::Build => formatter.write_str("Build"),
            Self::Peak1 => formatter.write_str("Peak 1"),
            Self::Peak2 => formatter.write_str("Peak 2"),
        }
    }
}

/// What a gym mesocycle is, as a macrocycle chooses between them.
///
/// **SBS or a hold, and both end in a test**: SBS week 4 is the 1RM, and a hold
/// is holding microcycles of the linear template and then a test microcycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gym {
    Hold { holding: u32 },
    Sbs,
}

impl Gym {
    pub const fn weeks(self) -> u32 {
        match self {
            Self::Hold { holding } => holding.saturating_add(1),
            Self::Sbs => 4,
        }
    }
}

impl fmt::Display for Gym {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hold { holding: 0 } => formatter.write_str("1RM test"),
            Self::Hold { holding } => write!(formatter, "hold ({holding} weeks, then 1RM test)"),
            Self::Sbs => formatter.write_str("SBS"),
        }
    }
}

/// One concurrent mesocycle: a gym mesocycle and a cycling mesocycle that
/// start together and end together.
///
/// **The two hold together or not at all.** A hold is what both disciplines
/// do when no pair fits; one discipline holding while the other progresses is
/// #190's partially completed week, not a choice a macrocycle makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Concurrent {
    gym: Gym,
    cycling: Cycling,
}

impl Concurrent {
    /// Both disciplines hold for `holding` weeks, then test.
    pub const fn hold(holding: u32) -> Self {
        Self {
            gym: Gym::Hold { holding },
            cycling: Cycling::Hold { holding },
        }
    }

    /// SBS beside a progressing cycling mesocycle. A hold is [`Self::hold`].
    pub const fn progressing(cycling: Cycling) -> Self {
        Self {
            gym: Gym::Sbs,
            cycling,
        }
    }

    pub const fn gym(self) -> Gym {
        self.gym
    }

    pub const fn cycling(self) -> Cycling {
        self.cycling
    }

    pub const fn weeks(self) -> u32 {
        self.cycling.weeks()
    }
}

impl fmt::Display for Concurrent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.gym, self.cycling) {
            (Gym::Hold { holding: 0 }, Cycling::Hold { .. }) => formatter.write_str("test week"),
            (Gym::Hold { holding }, Cycling::Hold { .. }) => {
                write!(formatter, "hold ({holding} weeks, then a test week)")
            }
            (gym, cycling) => write!(formatter, "{gym} + {cycling}"),
        }
    }
}

impl Macrocycle {
    /// The concurrent mesocycle to commit, starting on the Monday `start`.
    ///
    /// **One, never a sequence** (#222): each is chosen when the one before it
    /// ends, because only then is its entry test known. What follows it is
    /// only ever checked for, never written down.
    ///
    /// `previous` is the last cycling mesocycle of this macrocycle, `None` at
    /// its start. In order:
    ///
    /// 1. the next of the chain after `previous`;
    /// 2. a mesocycle the chain can start from (Base 1, Build, Peak 1);
    /// 3. Build, the one mesocycle that can end a term on its own;
    /// 4. a hold, as short as leaves a piece of the chain filling the rest of
    ///    the term exactly — which is the longest piece that fits;
    /// 5. otherwise the shortest hold after which the term can still end in a
    ///    peak some other way;
    /// 6. otherwise a hold to the end of term.
    ///
    /// **The chain's surplus weeks go into the hold before it**, so a term
    /// with fifteen weeks and a twelve-week chain holds for two and tests in
    /// the third, rather than testing at once and finding room for the rest
    /// somewhere in the middle.
    ///
    /// Each of 1–3 is taken only if it may follow `previous` and the term can
    /// still end in a peak after it, on the last day of term: before the
    /// holiday, never across it.
    ///
    /// `None` when `start` is not before the transition: there is no term left
    /// to commit anything in.
    pub fn next(&self, chain: Chain, previous: Option<Cycling>, start: Date) -> Option<Concurrent> {
        let weeks = weeks_between(start, self.transition())?;
        if weeks == 0 {
            return None;
        }

        let continuing = previous.and_then(|previous| chain.after(previous));
        let entries = chain
            .mesocycles()
            .into_iter()
            .filter(|one| matches!(one, Cycling::Base1 | Cycling::Build | Cycling::Peak1));
        let progressing = continuing
            .into_iter()
            .chain(entries)
            .chain(std::iter::once(Cycling::Build))
            .find(|one| one.may_follow(previous) && reaches_a_peak(chain, *one, weeks));
        if let Some(cycling) = progressing {
            return Some(Concurrent::progressing(cycling));
        }

        let holding = (0..weeks)
            .find(|holding| chain.fills(weeks.saturating_sub(holding.saturating_add(1))))
            .or_else(|| {
                (0..weeks).find(|holding| {
                    reaches_a_peak(chain, Cycling::Hold { holding: *holding }, weeks)
                })
            })
            .unwrap_or_else(|| weeks.saturating_sub(1));
        Some(Concurrent::hold(holding))
    }
}

/// Whole weeks from one Monday to another, `None` if `to` is before `from`.
fn weeks_between(from: Date, to: Date) -> Option<u32> {
    let days = (to - from).get_days();
    u32::try_from(days.checked_div(7)?).ok()
}

/// Whether, running `first` with `weeks` of term left, the term can still end
/// in a peak on its last day.
///
/// **Searched, never stored.** Only the first mesocycle is ever committed, so
/// this asks whether *some* way to finish exists and forgets which.
fn reaches_a_peak(chain: Chain, first: Cycling, weeks: u32) -> bool {
    let Some(left) = weeks.checked_sub(first.weeks()) else {
        return false;
    };
    if left == 0 {
        return first.competitive();
    }
    let chained = chain.mesocycles();
    let candidates = chained
        .into_iter()
        .chain(std::iter::once(Cycling::Build))
        .chain((0..left).map(|holding| Cycling::Hold { holding }));
    // Two holds in a row are one longer hold, which is already a candidate.
    let after_hold = matches!(first, Cycling::Hold { .. });
    candidates
        .filter(|next| !(after_hold && matches!(next, Cycling::Hold { .. })))
        .filter(|next| next.may_follow(Some(first)))
        .any(|next| reaches_a_peak(chain, next, left))
}
