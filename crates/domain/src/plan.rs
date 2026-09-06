//! The authored unit: a plan, its programmes, and the days they occupy.
//!
//! The operator's hierarchy, 2026-09-06:
//!
//! ```text
//! macrocycle → plan → programme → mesocycle → microcycle → session
//! ```
//!
//! The autumn is **one plan**. It holds a cycling programme and a gym
//! programme, and each of those holds four mesocycles: one entry test and three
//! progressions.
//!
//! **Containment does what succession was doing.** Until 2026-09-06 the
//! mesocycle was the authored unit, and four of them were held together by
//! nothing but successive start dates and a shared stem in their names — so
//! "the autumn plan" existed nowhere and there was no name for the thing being
//! authored (issue #86). A mesocycle has no name now; the plan has one, and
//! re-authoring under it supersedes every mesocycle at once.
//!
//! **The overlap rule mostly stops mattering.** It refused two *differently
//! named* programmes competing for a day, which is now two differently named
//! plans. Inside a plan the two programmes compete for every day on purpose:
//! running the gym and the bike together is the point of the tool.
//!
//! **`Programme` is generic over its mesocycle rather than written twice.** The
//! two disciplines' mesocycles are different types — one is a lift progressing,
//! the other is rides at zones — but what a *programme* does with them is
//! identical: keep them in order, refuse two that collide, and answer for a
//! date. Whether the mesocycles themselves converge is a separate question and
//! not settled here.

use std::fmt;

use jiff::{Timestamp, civil::Date};

use crate::{
    cycling::CyclingMesocycle,
    gym::sequence::{NonEmpty, TooShort},
    newtype::string_name,
    prescription::Mesocycle,
};

/// The days something occupies, without saying what.
///
/// **Calendar weeks, not training weeks.** A mesocycle interrupted for a
/// fortnight occupies those weeks whether or not it trains in them, and
/// something starting inside them would be competing for the same days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    start: Date,
    calendar_weeks: u32,
}

impl Span {
    pub const fn new(start: Date, calendar_weeks: u32) -> Self {
        Self {
            start,
            calendar_weeks,
        }
    }

    pub const fn start(self) -> Date {
        self.start
    }

    pub const fn calendar_weeks(self) -> u32 {
        self.calendar_weeks
    }

    /// The day after the last one this span occupies.
    ///
    /// Exclusive, so that a mesocycle starting the Monday after another ends is
    /// adjacent rather than overlapping — which is the common case and must not
    /// be refused.
    #[must_use]
    pub fn end(self) -> Date {
        let days = jiff::Span::new().days(i64::from(self.calendar_weeks).saturating_mul(7));
        self.start.checked_add(days).unwrap_or(self.start)
    }

    /// Whether two spans compete for a day.
    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        self.start < other.end() && other.start < self.end()
    }

    /// Whether this span is the one that answers for a date.
    #[must_use]
    pub fn covers(self, date: Date) -> bool {
        date >= self.start && date < self.end()
    }

    /// The span covering both, and everything between them.
    #[must_use]
    pub fn joined(self, other: Self) -> Self {
        let start = if self.start <= other.start {
            self.start
        } else {
            other.start
        };
        let end = if self.end() >= other.end() {
            self.end()
        } else {
            other.end()
        };
        let days = (end - start).get_days();
        Self::new(start, u32::try_from(days / 7).unwrap_or(u32::MAX))
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} to {}",
            self.start,
            self.end().yesterday().unwrap_or(self.start)
        )
    }
}

/// Anything that occupies days: a mesocycle, or a programme of them.
pub trait Occupies {
    fn span(&self) -> Span;
}

impl Occupies for Mesocycle {
    fn span(&self) -> Span {
        Span::new(self.calendar().start(), self.calendar().calendar_weeks())
    }
}

impl Occupies for CyclingMesocycle {
    fn span(&self) -> Span {
        Span::new(
            self.start(),
            u32::try_from(self.duration_weeks()).unwrap_or(u32::MAX),
        )
    }
}

/// The longest a plan's name may be.
///
/// A terminal line, not a rule about naming: it is printed beside a date and a
/// discipline, and something longer than this wraps.
pub const MAX_NAME: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidPlanName {
    #[error("a plan's name must not be empty")]
    Empty,
    #[error("a plan's name must be at most {MAX_NAME} characters, and this is {length}")]
    TooLong { length: usize },
    #[error("a plan's name must be one line of printable text")]
    NotPrintable,
}

/// What identifies a plan across re-authorings.
///
/// **Declared, never inferred.** The obvious natural key was the start date, and
/// it is wrong: correcting a start date would silently fork a new plan rather
/// than amend the one that exists. The operator names it.
///
/// **Free text, deliberately.** It is his own label, so nothing here imposes a
/// shape on it. The rules that do exist are the ones a label has to satisfy to be
/// an identity at all: surrounding whitespace is trimmed rather than rejected,
/// so `"autumn"` and `" autumn "` cannot become two plans.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlanName(String);

impl TryFrom<String> for PlanName {
    type Error = InvalidPlanName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(InvalidPlanName::Empty);
        }
        let length = trimmed.chars().count();
        if length > MAX_NAME {
            return Err(InvalidPlanName::TooLong { length });
        }
        if trimmed.chars().any(char::is_control) {
            return Err(InvalidPlanName::NotPrintable);
        }
        Ok(Self(trimmed.to_owned()))
    }
}

string_name!(PlanName, InvalidPlanName);

/// The identity the store gives a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlanId(i64);

impl PlanId {
    pub const fn new(value: i64) -> Self {
        Self(value)
    }

    pub const fn as_i64(self) -> i64 {
        self.0
    }
}

impl fmt::Display for PlanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A plan's name and the days it occupies.
///
/// What the overlap rule reads. Carried separately from the plan itself because
/// deciding whether a new plan may be authored needs every existing plan's
/// *span* and none of their contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanWindow {
    name: PlanName,
    span: Span,
}

impl PlanWindow {
    pub const fn new(name: PlanName, span: Span) -> Self {
        Self { name, span }
    }

    pub const fn name(&self) -> &PlanName {
        &self.name
    }

    pub const fn span(&self) -> Span {
        self.span
    }

    /// Whether two plans compete for a day.
    ///
    /// **Versions of one plan never do.** Two windows sharing a name are the
    /// same plan re-authored, and only the latest is ever read, so they are
    /// permitted to sit on top of each other — which is what makes re-authoring
    /// the autumn legal without deleting anything.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        if self.name == other.name {
            return false;
        }
        self.span.overlaps(other.span)
    }
}

impl fmt::Display for PlanWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.span)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidProgramme {
    #[error("a programme is at least one mesocycle")]
    NoMesocycles,
    #[error(
        "mesocycle {at} starts on {start}, which is not after mesocycle {before} ends — \
         two mesocycles of one programme may not compete for a day"
    )]
    OutOfOrder {
        at: usize,
        before: usize,
        start: Date,
    },
}

/// One discipline's mesocycles inside a plan, in the order they are run.
///
/// **Ordered and non-overlapping, but gaps are legal.** A blank week between two
/// mesocycles is a real thing — a deload nobody programmed, a fortnight away —
/// and a date inside it belongs to neither. What is refused is two mesocycles
/// competing for one day, which would make the answer to "what am I doing today"
/// depend on the order rows came back in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Programme<M> {
    mesocycles: NonEmpty<M>,
}

impl<M: Occupies> Programme<M> {
    /// # Errors
    ///
    /// [`InvalidProgramme`] for no mesocycles at all, or for two that overlap or
    /// are out of order.
    pub fn new(mesocycles: Vec<M>) -> Result<Self, InvalidProgramme> {
        for (at, pair) in mesocycles.windows(2).enumerate() {
            let (Some(before), Some(next)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            let (before, next) = (before.span(), next.span());
            if next.start() < before.end() {
                return Err(InvalidProgramme::OutOfOrder {
                    at: at.saturating_add(2),
                    before: at.saturating_add(1),
                    start: next.start(),
                });
            }
        }
        Ok(Self {
            mesocycles: NonEmpty::new(mesocycles)
                .map_err(|_: TooShort| InvalidProgramme::NoMesocycles)?,
        })
    }

    pub fn mesocycles(&self) -> impl Iterator<Item = &M> {
        self.mesocycles.iter()
    }

    pub const fn first(&self) -> &M {
        self.mesocycles.first()
    }

    pub const fn count(&self) -> usize {
        self.mesocycles.count()
    }

    /// The mesocycle that answers for a date, and where it sits in this
    /// programme.
    ///
    /// `None` for a day in a gap, before the first, or after the last. A real
    /// state, not a fault.
    pub fn on(&self, date: Date) -> Option<(usize, &M)> {
        self.mesocycles
            .iter()
            .enumerate()
            .find(|(_, mesocycle)| mesocycle.span().covers(date))
    }

    /// The mesocycle immediately before a date, if this programme holds one.
    ///
    /// **What a test takes its target from** (decision 0013). Inside a plan that
    /// is the previous element of a list rather than a question for the store —
    /// only a mesocycle at the very start of a plan has to ask.
    pub fn preceding(&self, date: Date) -> Option<&M> {
        self.mesocycles
            .iter()
            .filter(|mesocycle| mesocycle.span().end() <= date)
            .last()
    }

    /// The days this programme occupies, first mesocycle to last.
    #[must_use]
    pub fn span(&self) -> Span {
        self.mesocycles
            .iter()
            .map(Occupies::span)
            .reduce(Span::joined)
            .unwrap_or_else(|| self.mesocycles.first().span())
    }
}

impl<M: Occupies> Occupies for Programme<M> {
    fn span(&self) -> Span {
        Self::span(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a plan trains something: it holds a gym programme, a cycling programme, or both")]
pub struct EmptyPlan;

/// What is authored: one plan, holding one programme per discipline.
///
/// **A field per discipline rather than a list.** A plan with two gym programmes
/// is not a thing, and a list would let one be built; a `KnownDiscipline` key
/// would be the same list with a lookup in front of it. What this costs is a line
/// per discipline when a third arrives, which is the right price for making the
/// wrong shape unrepresentable.
///
/// **Either may be absent, but not both.** A gym-only plan is what the store has
/// held all autumn, and a cycling-only one is what `fitness plan` writes today;
/// a plan holding neither trains nothing and is refused at construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    name: PlanName,
    authored_at: Timestamp,
    gym: Option<Programme<Mesocycle>>,
    cycling: Option<Programme<CyclingMesocycle>>,
}

impl Plan {
    /// # Errors
    ///
    /// [`EmptyPlan`] if it holds no programme at all.
    pub fn new(
        name: PlanName,
        authored_at: Timestamp,
        gym: Option<Programme<Mesocycle>>,
        cycling: Option<Programme<CyclingMesocycle>>,
    ) -> Result<Self, EmptyPlan> {
        if gym.is_none() && cycling.is_none() {
            return Err(EmptyPlan);
        }
        Ok(Self {
            name,
            authored_at,
            gym,
            cycling,
        })
    }

    pub const fn name(&self) -> &PlanName {
        &self.name
    }

    pub const fn authored_at(&self) -> Timestamp {
        self.authored_at
    }

    pub const fn gym(&self) -> Option<&Programme<Mesocycle>> {
        self.gym.as_ref()
    }

    pub const fn cycling(&self) -> Option<&Programme<CyclingMesocycle>> {
        self.cycling.as_ref()
    }

    /// The days this plan occupies, across both disciplines.
    ///
    /// **The two need not start on the same day.** The autumn's do, and that is
    /// an outcome of planning rather than a rule: coherence is a constraint the
    /// planner applies, not an invariant of the type.
    #[must_use]
    pub fn span(&self) -> Span {
        match (self.gym.as_ref(), self.cycling.as_ref()) {
            (Some(gym), Some(cycling)) => gym.span().joined(cycling.span()),
            (Some(gym), None) => gym.span(),
            (None, Some(cycling)) => cycling.span(),
            // Unrepresentable: `new` refuses a plan with neither. Answered
            // rather than panicked, because a plan is not worth an unwrap.
            (None, None) => Span::new(self.authored_at.to_zoned(jiff::tz::TimeZone::UTC).date(), 0),
        }
    }

    /// The name and the days, for the rule that two plans may not compete.
    #[must_use]
    pub fn window(&self) -> PlanWindow {
        PlanWindow::new(self.name.clone(), self.span())
    }
}
