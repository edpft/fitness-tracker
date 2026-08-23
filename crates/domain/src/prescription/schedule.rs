//! Where a date sits in a block.
//!
//! **The calendar is authored, so the answer is deterministic and available for
//! a date in the future** — which is what "issue the next session" needs. The
//! alternative, counting performed sessions, was rejected: a missed session
//! would flip every subsequent role and one absence would desynchronise the
//! programme permanently.
//!
//! Arithmetic resolves through the operator's IANA zone rather than by adding
//! multiples of 24 hours. § II.3 requires it, and a system assuming every local
//! day is 24 hours long is wrong twice a year.
//!
//! **A block's duration counts training weeks, and the calendar it runs against
//! is longer.** A week away is not a week of the plan: the session after it is
//! the same rung of the ladder as the session before it. Days since the start
//! divided by seven says otherwise, and spends a ladder position on a week
//! nobody trained — which is precisely the arithmetic the operator keeps by
//! hand and wants to stop keeping.

use std::{fmt, num::NonZeroU8};

use jiff::{Zoned, civil::Date, tz::TimeZone};

use super::delivery::SessionOrdinal;

/// Which session within a week.
///
/// An ordering rather than state: the two differ in fill and in the primary's
/// loading, and nothing carries "which one is next" because the calendar
/// already says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SessionRole {
    Light,
    Heavy,
}

impl SessionRole {
    pub const ALL: &'static [Self] = &[Self::Light, Self::Heavy];

    /// The stable key. Persisted, so it outlives a rename.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Heavy => "heavy",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} does not name a session role")]
pub struct UnknownSessionRole {
    value: String,
}

impl TryFrom<String> for SessionRole {
    type Error = UnknownSessionRole;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "light" => Ok(Self::Light),
            "heavy" => Ok(Self::Heavy),
            _ => Err(UnknownSessionRole { value }),
        }
    }
}

impl fmt::Display for SessionRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One value per role, both mandatory.
///
/// A struct rather than a map, so a programme missing a role is a compile error
/// rather than a runtime one (§ 24). Two roles exist and both are always needed;
/// a map would make "the heavy session has no rep count" representable and then
/// need checking wherever it is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerRole<T> {
    pub light: T,
    pub heavy: T,
}

impl<T> PerRole<T> {
    pub const fn get(&self, role: SessionRole) -> &T {
        match role {
            SessionRole::Light => &self.light,
            SessionRole::Heavy => &self.heavy,
        }
    }
}

/// Which climbing week of a block, one-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WeekIndex(u32);

impl WeekIndex {
    /// The first week of a block.
    ///
    /// A constant because one is always a valid week and a caller that starts at
    /// the beginning should not have to handle an error that cannot happen.
    pub const FIRST: Self = Self(1);

    /// The week after this one.
    ///
    /// Saturating, which is not a real limit: a block of four billion weeks is
    /// refused long before this, and the alternative is an error nobody can act
    /// on.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    /// # Errors
    ///
    /// [`InvalidWeek`] for zero. Weeks are one-based because the operator
    /// counts them that way and an off-by-one in a load table is expensive.
    pub const fn new(week: u32) -> Result<Self, InvalidWeek> {
        if week == 0 {
            return Err(InvalidWeek);
        }
        Ok(Self(week))
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }

    /// Zero-based, for stepping along a ladder.
    pub const fn as_offset(self) -> u32 {
        self.0 - 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a block's weeks are numbered from one")]
pub struct InvalidWeek;

impl fmt::Display for WeekIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A ladder position, or the block's test.
///
/// The last week of a block is not a ladder position and does not have a
/// percentage; making that a variant rather than a flag is what stops a caller
/// asking the ladder for a load it does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeekKind {
    Climbing(WeekIndex),
    Test,
}

impl WeekKind {
    /// The stable key. Persisted.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Climbing(_) => "climbing",
            Self::Test => "test",
        }
    }

    pub const fn index(self) -> Option<WeekIndex> {
        match self {
            Self::Climbing(week) => Some(week),
            Self::Test => None,
        }
    }
}

impl fmt::Display for WeekKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Climbing(week) => write!(f, "week {week}"),
            Self::Test => f.write_str("test"),
        }
    }
}

/// Which weekdays the programme runs, and as what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weekdays {
    days: Vec<(jiff::civil::Weekday, SessionRole)>,
}

/// Why a date could not be placed in the block.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotScheduled {
    #[error("{date} is a {weekday}; this programme runs {programmed}")]
    NotAProgrammedDay {
        date: Date,
        weekday: String,
        programmed: String,
    },
    #[error("{date} is before the block starts on {start}")]
    BeforeStart { date: Date, start: Date },
    #[error("{date} is past the end of a {duration}-week block starting {start}")]
    PastEnd {
        date: Date,
        start: Date,
        duration: u32,
    },
    /// The message names the whole skip, not just the date asked about: "the
    /// 8th is not run" is less use than "you are away the 4th to the 11th".
    /// A one-day skip displays as the bare date, so it does not repeat itself.
    #[error("{date} is not run: this block skips {skip}")]
    Interrupted { date: Date, skip: Skip },
}

impl Weekdays {
    /// # Errors
    ///
    /// [`NoWeekdays`] if the programme runs on no day at all.
    pub fn new(days: Vec<(jiff::civil::Weekday, SessionRole)>) -> Result<Self, NoWeekdays> {
        if days.is_empty() {
            return Err(NoWeekdays);
        }
        Ok(Self { days })
    }

    pub fn role_on(&self, weekday: jiff::civil::Weekday) -> Option<SessionRole> {
        self.days
            .iter()
            .find(|(day, _)| *day == weekday)
            .map(|(_, role)| *role)
    }

    pub fn iter(&self) -> impl Iterator<Item = (jiff::civil::Weekday, SessionRole)> + '_ {
        self.days.iter().copied()
    }

    /// Whether the programme runs the given role at all.
    ///
    /// A programme gating on a role it never runs would never advance, which is
    /// one of the three things the types cannot catch.
    pub fn runs(&self, role: SessionRole) -> bool {
        self.days.iter().any(|(_, scheduled)| *scheduled == role)
    }

    fn describe(&self) -> String {
        let mut parts: Vec<String> = self
            .days
            .iter()
            .map(|(day, role)| format!("{day:?} ({role})"))
            .collect();
        parts.sort();
        parts.join(" and ")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a programme that runs on no day of the week issues nothing")]
pub struct NoWeekdays;

/// The sessions a block does not run.
///
/// Authored per block rather than consulted from the family calendar at
/// generation time: what the block skipped is part of what was planned, and a
/// prescription has to be reproducible from the authored record long after the
/// holiday is off anyone's calendar (§ 12).
///
/// **Sessions, not weeks.** This held one date per skipped week until
/// 2026-08-21, which could not say "away Friday but back for Monday" — and the
/// operator's actual September is four individual sessions across three weeks.
/// A week is now interrupted only as a consequence: it is one where nothing
/// survives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Skip {
    start: Date,
    days: NonZeroU8,
}

impl Skip {
    /// A run of days the block does not run, starting at `start`.
    ///
    /// **A count rather than an end date**, which is the operator's own design:
    /// `through < from` is then unwritable rather than checked, so there is no
    /// invalid range to reject and no constructor that can fail. `days` is at
    /// least one, so an empty skip is unwritable too — and an empty skip that
    /// authored successfully would silently not skip anything.
    pub const fn new(start: Date, days: NonZeroU8) -> Self {
        Self { start, days }
    }

    /// One day. What almost every skip is.
    ///
    /// A `Skip` rather than a variant beside one: `Day(d)` and
    /// `Range { start: d, days: 1 }` would be two spellings of one fact, and
    /// two spellings that compare unequal is the defect the single
    /// representation avoids. The document may still write either.
    pub const fn day(start: Date) -> Self {
        Self {
            start,
            days: NonZeroU8::MIN,
        }
    }

    pub const fn start(self) -> Date {
        self.start
    }

    pub const fn days(self) -> NonZeroU8 {
        self.days
    }

    /// The last day this skip covers. Derived, never stored beside the start —
    /// a second source of truth is a second thing that can disagree.
    #[must_use]
    pub fn last(self) -> Date {
        let span = jiff::Span::new().days(i64::from(self.days.get()) - 1);
        self.start.checked_add(span).unwrap_or(self.start)
    }

    #[must_use]
    pub fn covers(self, date: Date) -> bool {
        date >= self.start && date <= self.last()
    }
}

impl fmt::Display for Skip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.days.get() == 1 {
            return write!(f, "{}", self.start);
        }
        write!(f, "{} to {}", self.start, self.last())
    }
}

/// Every session a block does not run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Interruptions {
    /// As authored, ascending. Overlaps are permitted and mean nothing extra: a
    /// day skipped twice is skipped.
    skips: Vec<Skip>,
}

impl Interruptions {
    pub fn iter(&self) -> impl Iterator<Item = Skip> + '_ {
        self.skips.iter().copied()
    }

    pub const fn len(&self) -> usize {
        self.skips.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.skips.is_empty()
    }

    /// Whether the block skips this date.
    #[must_use]
    pub fn covers(&self, date: Date) -> bool {
        self.skips.iter().any(|skip| skip.covers(date))
    }

    /// The skip covering a date, for a message that says which holiday.
    #[must_use]
    pub fn covering(&self, date: Date) -> Option<Skip> {
        self.skips.iter().copied().find(|skip| skip.covers(date))
    }
}

/// Why a calendar could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidCalendar {
    #[error("a block of no weeks issues nothing")]
    NoWeeks,
    #[error("{skip} is named as an interruption, but the block starts later, on {start}")]
    InterruptionBeforeStart { skip: Skip, start: Date },
    #[error(
        "{skip} is named as an interruption, but a {duration}-week block starting {start} \
         has already finished by then"
    )]
    InterruptionPastEnd {
        skip: Skip,
        start: Date,
        duration: u32,
    },
    #[error("these interruptions leave a {duration}-week block without {duration} weeks to run in")]
    NeverCompletes { duration: u32 },
}

/// The block's calendar: where it starts, how many training weeks it runs, which
/// weeks it skips, and which weekdays carry which role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    start: Date,
    /// Training weeks. The calendar the block occupies is one week longer per
    /// interruption.
    duration_weeks: u32,
    interruptions: Interruptions,
    weekdays: Weekdays,
    zone: TimeZone,
}

impl Calendar {
    /// # Errors
    ///
    /// [`InvalidCalendar`] for a block of no weeks, or for an interruption that
    /// falls outside the block. An interruption outside the block would change
    /// nothing, and is refused rather than ignored: it means the operator and
    /// this programme disagree about when the block runs, and that disagreement
    /// is worth more than the holiday.
    pub fn new(
        start: Date,
        duration_weeks: u32,
        interruptions: &[Skip],
        weekdays: Weekdays,
        zone: TimeZone,
    ) -> Result<Self, InvalidCalendar> {
        if duration_weeks == 0 {
            return Err(InvalidCalendar::NoWeeks);
        }

        let mut skips: Vec<Skip> = Vec::with_capacity(interruptions.len());
        for skip in interruptions {
            if skip.last() < start {
                return Err(InvalidCalendar::InterruptionBeforeStart { skip: *skip, start });
            }
            skips.push(*skip);
        }
        skips.sort_unstable();
        skips.dedup();

        let calendar = Self {
            start,
            duration_weeks,
            interruptions: Interruptions { skips },
            weekdays,
            zone,
        };

        // The span has to be resolved before an interruption can be called
        // late, because how far the block reaches is a function of the
        // interruptions themselves. Walking is the only way round that, and it
        // is also what catches a set of skips that leaves no block at all.
        let span = calendar
            .span_weeks()
            .ok_or(InvalidCalendar::NeverCompletes {
                duration: duration_weeks,
            })?;
        for skip in calendar.interruptions.iter() {
            if calendar.week_offset(skip.start()) >= i64::from(span) {
                return Err(InvalidCalendar::InterruptionPastEnd {
                    skip,
                    start,
                    duration: duration_weeks,
                });
            }
        }

        Ok(calendar)
    }

    pub const fn start(&self) -> Date {
        self.start
    }

    /// Training weeks, which is what the ladder is laid out over.
    pub const fn duration_weeks(&self) -> u32 {
        self.duration_weeks
    }

    /// Calendar weeks: the training weeks plus the ones skipped between them.
    ///
    /// Walked rather than counted. A skipped *week* used to be a thing the
    /// operator named, so this was addition; now a week is skipped only when
    /// every session in it is, which is a question about the weekday map and
    /// the skips together.
    pub fn calendar_weeks(&self) -> u32 {
        self.span_weeks().unwrap_or(self.duration_weeks)
    }

    /// How many calendar weeks the block occupies, or `None` if the
    /// interruptions never leave it enough weeks to run in.
    fn span_weeks(&self) -> Option<u32> {
        // Past the last skipped day every week runs, so the block always
        // completes within its duration plus the weeks the skips reach over.
        let reach = self
            .interruptions
            .iter()
            .map(|skip| self.week_offset(skip.last()).max(0))
            .max()
            .unwrap_or(0);
        let ceiling = i64::from(self.duration_weeks)
            .saturating_add(reach)
            .saturating_add(1);

        let mut training = 0_u32;
        for week in 0..ceiling {
            if self.week_runs(week) {
                training = training.saturating_add(1);
                if training == self.duration_weeks {
                    return u32::try_from(week + 1).ok();
                }
            }
        }
        None
    }

    /// Which calendar week from the block's start a date falls in, zero-based.
    fn week_offset(&self, date: Date) -> i64 {
        offset_of(self.start, date)
    }

    /// Whether any session survives in this calendar week.
    ///
    /// **A week is a training week if at least one of its sessions runs.** The
    /// operator settled that on 2026-08-21, and it is what makes a half-skipped
    /// week — away Friday, back for Monday — a week of the block rather than a
    /// gap in it. The old whole-week behaviour falls out as the case where
    /// nothing survives.
    fn week_runs(&self, week: i64) -> bool {
        (0..7).any(|day| {
            let span = jiff::Span::new().days(week.saturating_mul(7).saturating_add(day));
            self.start.checked_add(span).is_ok_and(|date| {
                self.weekdays.role_on(date.weekday()).is_some() && !self.interruptions.covers(date)
            })
        })
    }

    pub const fn interruptions(&self) -> &Interruptions {
        &self.interruptions
    }

    pub const fn weekdays(&self) -> &Weekdays {
        &self.weekdays
    }

    /// Where a date sits: which **training** week of the block, and in which
    /// role.
    ///
    /// # Errors
    ///
    /// [`NotScheduled`] if the date falls on no programmed weekday, before the
    /// block, in a week the block skips, or past its end. Declining is
    /// deliberate — silently prescribing Friday's session for a Wednesday is
    /// worse than saying no, and so is prescribing week 4 to someone who spent
    /// week 3 on a beach.
    pub fn place(&self, date: Date) -> Result<(WeekKind, SessionRole), NotScheduled> {
        let Some(role) = self.weekdays.role_on(date.weekday()) else {
            return Err(NotScheduled::NotAProgrammedDay {
                date,
                weekday: format!("{:?}", date.weekday()),
                programmed: self.weekdays.describe(),
            });
        };

        if date < self.start {
            return Err(NotScheduled::BeforeStart {
                date,
                start: self.start,
            });
        }

        // Whole weeks since the block's first day. Calendar days rather than
        // hours, which is what keeps a daylight-saving boundary from shifting a
        // session into the previous week.
        let offset = offset_of(self.start, date);

        if let Some(skip) = self.interruptions.covering(date) {
            return Err(NotScheduled::Interrupted { date, skip });
        }

        // The training week: calendar weeks elapsed, less the ones the block
        // skipped on the way here.
        let week = u32::try_from(offset - self.skipped_before(offset)).unwrap_or(u32::MAX) + 1;

        if week > self.duration_weeks {
            return Err(NotScheduled::PastEnd {
                date,
                start: self.start,
                duration: self.duration_weeks,
            });
        }

        // **Every week of a linear block climbs** (decision 0013). The last
        // week used to be its test, which made `duration_weeks` mean "climbing
        // weeks and a test" and left the programme describing a session that
        // was not its own. A test is a programme in its own right now, so this
        // calendar has none to place.
        //
        // `WeekKind::Test` stays for the templates that do own their test: a
        // periodised block always ends in one, and a standalone test programme
        // is nothing else.
        let kind = WeekKind::Climbing(WeekIndex::new(week).unwrap_or(WeekIndex(1)));
        Ok((kind, role))
    }

    /// The next session at or after a date, or `None` once the block is over.
    ///
    /// What `--date` defaults to: "the next session" is what an operator wants
    /// on a rest day and today is what they want on a training day, and this
    /// gives both.
    ///
    /// **A programmed weekday is not the same as a session.** An interrupted
    /// week has both its weekdays and no sessions at all, so this asks
    /// [`Self::place`] rather than the weekday map, and searches to the end of
    /// the block rather than over a single week.
    pub fn next_programmed(&self, from: Date) -> Option<Date> {
        let end = self
            .start
            .checked_add(jiff::Span::new().weeks(i64::from(self.calendar_weeks())))
            .ok()?;
        let horizon = (end - from).get_days();
        (0..horizon).find_map(|offset| {
            let candidate = from.checked_add(jiff::Span::new().days(offset)).ok()?;
            self.place(candidate).ok().map(|_| candidate)
        })
    }

    /// Which session of the block this date is, counting from the first.
    ///
    /// **Counts sessions, not days and not weeks.** A week the block skipped
    /// contributes nothing, and a week running two sessions contributes two — so
    /// the number is a total order over everything the block prescribes, which
    /// is what an ordered list of them needs and what a week index cannot give.
    ///
    /// `None` for a date the block does not run, which is the same answer
    /// [`Self::place`] gives and for the same reason.
    pub fn ordinal(&self, date: Date) -> Option<SessionOrdinal> {
        if self.place(date).is_err() {
            return None;
        }

        // Walk the programmed days from the start rather than deriving from the
        // week index: interruptions are runs of days rather than whole weeks
        // (decision 0010), so a week can lose one of its two sessions and
        // arithmetic over weeks would count it anyway.
        let mut counted: u32 = 0;
        let mut cursor = self.start;
        while cursor <= date {
            if self.place(cursor).is_ok() {
                counted = counted.saturating_add(1);
            }
            cursor = cursor.tomorrow().ok()?;
        }

        SessionOrdinal::new(counted).ok()
    }

    /// How many calendar weeks before this one had no session at all.
    ///
    /// Those are not weeks of the block, so they do not advance the training
    /// week. A week that lost only some of its sessions is not one of them.
    fn skipped_before(&self, offset: i64) -> i64 {
        let skipped = (0..offset).filter(|week| !self.week_runs(*week)).count();
        i64::try_from(skipped).unwrap_or(i64::MAX)
    }

    /// Today, in the operator's zone.
    ///
    /// Here rather than at a call site because "today" is a question about a
    /// zone, and answering it from a system clock in UTC is how a session lands
    /// on the wrong day for anyone training in the evening.
    pub fn today(&self, now: jiff::Timestamp) -> Date {
        Zoned::new(now, self.zone.clone()).date()
    }
}

/// Which calendar week from a block's start a date falls in, zero-based.
///
/// Negative before the start, which every caller has already excluded. A block's
/// weeks run from the weekday it started on, so this is deliberately not
/// anchored to Monday: a block beginning on a Wednesday has Wednesday-to-Tuesday
/// weeks, and its interruptions are the same weeks its sessions are.
fn offset_of(start: Date, date: Date) -> i64 {
    i64::from((date - start).get_days()) / 7
}
