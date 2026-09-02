//! A cycling programme: weeks of sessions, and which of them get ridden.
//!
//! **The programme states every session it holds; the operator rides a subset.**
//! Those are two different facts and the type keeps them apart. *Peak Your Power
//! Zones* prescribes three sessions a week and the operator trains cycling twice
//! (decision 0025), so a [`Selection`] names which days he takes and which
//! weekday each falls on. Baking the subset into the programme would lose the
//! thing the choice was made against — the shape of the whole — and make
//! changing it a re-transcription rather than a decision.
//!
//! **A day is a position in the programme's own cycle, not a weekday.** *Peak
//! Your Power Zones* is written as days 1 to 7 with sessions on 1, 3 and 6, and
//! says nothing about Wednesdays. What maps a cycle day onto a weekday is the
//! operator's schedule, which is exactly where that belongs — the programme is
//! the training and the calendar is his week.

use std::collections::BTreeMap;

use jiff::civil::{Date, Weekday};

use crate::{gym::sequence::NonEmpty, newtype::string_name};

use super::session::CyclingSession;

/// A position in the programme's weekly cycle.
///
/// One to seven. Not a weekday: *Peak Your Power Zones* runs sessions on days 1,
/// 3 and 6 and leaves the rest to recovery, and which calendar day each lands on
/// is the schedule's business.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CycleDay(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a training cycle runs seven days, so day {value} is not one of them")]
pub struct InvalidCycleDay {
    value: u8,
}

impl CycleDay {
    /// # Errors
    ///
    /// [`InvalidCycleDay`] outside 1 to 7.
    pub const fn new(day: u8) -> Result<Self, InvalidCycleDay> {
        if day == 0 || day > 7 {
            return Err(InvalidCycleDay { value: day });
        }
        Ok(Self(day))
    }

    pub const fn as_u8(self) -> u8 {
        self.0
    }
}

impl std::fmt::Display for CycleDay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "day {}", self.0)
    }
}

/// One week of the programme, as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgrammeWeek {
    sessions: BTreeMap<CycleDay, CyclingSession>,
}

impl ProgrammeWeek {
    #[must_use]
    pub const fn new(sessions: BTreeMap<CycleDay, CyclingSession>) -> Self {
        Self { sessions }
    }

    pub const fn sessions(&self) -> &BTreeMap<CycleDay, CyclingSession> {
        &self.sessions
    }

    #[must_use]
    pub fn session(&self, day: CycleDay) -> Option<&CyclingSession> {
        self.sessions.get(&day)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidProgrammeName {
    #[error("a cycling programme's name must not be empty")]
    Empty,
    #[error("a cycling programme's name must be one line of printable text")]
    NotPrintable,
}

/// What a cycling programme is called.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CyclingProgrammeName(String);

impl TryFrom<String> for CyclingProgrammeName {
    type Error = InvalidProgrammeName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(InvalidProgrammeName::Empty);
        }
        if trimmed.chars().any(char::is_control) {
            return Err(InvalidProgrammeName::NotPrintable);
        }
        Ok(Self(trimmed.to_owned()))
    }
}

string_name!(CyclingProgrammeName, InvalidProgrammeName);

/// A published cycling programme, transcribed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CyclingProgramme {
    name: CyclingProgrammeName,
    weeks: NonEmpty<ProgrammeWeek>,
}

impl CyclingProgramme {
    #[must_use]
    pub const fn new(name: CyclingProgrammeName, weeks: NonEmpty<ProgrammeWeek>) -> Self {
        Self { name, weeks }
    }

    pub const fn name(&self) -> &CyclingProgrammeName {
        &self.name
    }

    pub const fn weeks(&self) -> &NonEmpty<ProgrammeWeek> {
        &self.weeks
    }

    /// How many weeks the programme runs.
    #[must_use]
    pub const fn duration_weeks(&self) -> usize {
        self.weeks.count()
    }

    /// One week, counting from one as the programme itself does.
    #[must_use]
    pub fn week(&self, number: usize) -> Option<&ProgrammeWeek> {
        number
            .checked_sub(1)
            .and_then(|index| self.weeks.iter().nth(index))
    }
}

/// Which sessions the operator actually rides, and on which weekday.
///
/// **Two facts in one place because they must agree.** Taking day 6 and riding
/// on Sunday are the same decision — day 6 is the long ride and Sunday morning
/// is the only slot long enough — and splitting them across two records would
/// let them drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// A list rather than a map because `jiff`'s `Weekday` is deliberately not
    /// `Ord` — a week has no universal first day. At most seven entries, so a
    /// scan is the whole cost.
    days: Vec<(Weekday, CycleDay)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a selection that rides no day is not a selection")]
pub struct EmptySelection;

impl Selection {
    /// # Errors
    ///
    /// [`EmptySelection`] if no weekday is mapped.
    pub fn new(days: Vec<(Weekday, CycleDay)>) -> Result<Self, EmptySelection> {
        if days.is_empty() {
            return Err(EmptySelection);
        }
        Ok(Self { days })
    }

    #[must_use]
    pub fn days(&self) -> &[(Weekday, CycleDay)] {
        &self.days
    }

    /// Which cycle day this date rides, if it rides one.
    #[must_use]
    pub fn cycle_day(&self, date: Date) -> Option<CycleDay> {
        self.days
            .iter()
            .find(|(weekday, _)| *weekday == date.weekday())
            .map(|(_, day)| *day)
    }
}
