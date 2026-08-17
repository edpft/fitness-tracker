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

use std::fmt;

use jiff::{Zoned, civil::Date, tz::TimeZone};

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

/// The block's calendar: where it starts, how long it runs, and which weekdays
/// carry which role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    start: Date,
    duration_weeks: u32,
    weekdays: Weekdays,
    zone: TimeZone,
}

impl Calendar {
    pub const fn new(start: Date, duration_weeks: u32, weekdays: Weekdays, zone: TimeZone) -> Self {
        Self {
            start,
            duration_weeks,
            weekdays,
            zone,
        }
    }

    pub const fn start(&self) -> Date {
        self.start
    }

    pub const fn duration_weeks(&self) -> u32 {
        self.duration_weeks
    }

    pub const fn weekdays(&self) -> &Weekdays {
        &self.weekdays
    }

    /// Where a date sits: which week of the block, and in which role.
    ///
    /// # Errors
    ///
    /// [`NotScheduled`] if the date falls on no programmed weekday, before the
    /// block, or past its end. Declining is deliberate — silently prescribing
    /// Friday's session for a Wednesday is worse than saying no.
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
        let days = (date - self.start).get_days();
        let week = u32::try_from(days / 7).unwrap_or(u32::MAX) + 1;

        if week > self.duration_weeks {
            return Err(NotScheduled::PastEnd {
                date,
                start: self.start,
                duration: self.duration_weeks,
            });
        }

        // The last week of a block is its test, and is not a ladder position.
        let kind = if week == self.duration_weeks {
            WeekKind::Test
        } else {
            WeekKind::Climbing(WeekIndex::new(week).unwrap_or(WeekIndex(1)))
        };
        Ok((kind, role))
    }

    /// The next programmed day at or after a date.
    ///
    /// What `--date` defaults to: "the next session" is what an operator wants
    /// on a rest day and today is what they want on a training day, and this
    /// gives both.
    pub fn next_programmed(&self, from: Date) -> Option<Date> {
        // A week of candidates is enough: any programmed weekday recurs within
        // seven days of any date.
        (0..7).find_map(|offset| {
            let candidate = from.checked_add(jiff::Span::new().days(offset)).ok()?;
            self.weekdays
                .role_on(candidate.weekday())
                .map(|_| candidate)
        })
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
