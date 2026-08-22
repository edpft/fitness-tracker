//! What makes one programme a different programme from another.
//!
//! **Two relations, and they are not the same** (decision 0012). The same
//! programme re-authored to correct it supersedes its earlier version, and
//! `authored_at` settles which version wins. A *different* programme, later in
//! time, succeeds it: both are real, and which one answers a question depends on
//! the date being asked about.
//!
//! One mechanism carried both until 2026-08-22, which is why authoring the
//! autumn block would have replaced the summer one rather than followed it.

use std::fmt;

use jiff::civil::Date;

use crate::newtype::string_name;

/// The longest a programme's name may be.
///
/// A terminal line, not a rule about naming: the name is printed beside a date
/// and a template in `programme show`, and something longer than this wraps.
/// Nothing downstream depends on the value.
pub const MAX_NAME: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidProgrammeName {
    #[error("a programme's name must not be empty")]
    Empty,
    #[error("a programme's name must be at most {MAX_NAME} characters, and this is {length}")]
    TooLong { length: usize },
    #[error("a programme's name must be one line of printable text")]
    NotPrintable,
}

/// What identifies a programme across re-authorings.
///
/// **Declared, never inferred.** The obvious natural key was the start date, and
/// it is wrong: correcting a start date would silently fork a new programme
/// rather than amend the one that exists. The operator names it, and the
/// document carries the name so the authored record is reproducible from the
/// document alone (§ 12).
///
/// **Free text, deliberately.** It is the operator's own label, so nothing here
/// imposes a shape on it — no kebab-case, no character set beyond "one printable
/// line". The rules that do exist are the ones a label has to satisfy to be an
/// identity at all: surrounding whitespace is trimmed rather than rejected, so
/// that `"autumn"` and `" autumn "` cannot become two programmes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProgrammeName(String);

impl TryFrom<String> for ProgrammeName {
    type Error = InvalidProgrammeName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(InvalidProgrammeName::Empty);
        }
        let length = trimmed.chars().count();
        if length > MAX_NAME {
            return Err(InvalidProgrammeName::TooLong { length });
        }
        if trimmed.chars().any(char::is_control) {
            return Err(InvalidProgrammeName::NotPrintable);
        }
        Ok(Self(trimmed.to_owned()))
    }
}

string_name!(ProgrammeName, InvalidProgrammeName);

/// A programme's name and the calendar it occupies.
///
/// What the overlap rule reads. Carried separately from the programme itself
/// because deciding whether a new programme may be authored needs every existing
/// programme's *span* and none of their contents.
///
/// **Calendar weeks, not training weeks.** A block interrupted for a fortnight
/// occupies those weeks whether or not it trains in them, and a programme
/// starting inside them would be competing for the same days.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgrammeWindow {
    name: ProgrammeName,
    start: Date,
    calendar_weeks: u32,
}

impl ProgrammeWindow {
    pub const fn new(name: ProgrammeName, start: Date, calendar_weeks: u32) -> Self {
        Self {
            name,
            start,
            calendar_weeks,
        }
    }

    pub const fn name(&self) -> &ProgrammeName {
        &self.name
    }

    pub const fn start(&self) -> Date {
        self.start
    }

    pub const fn calendar_weeks(&self) -> u32 {
        self.calendar_weeks
    }

    /// The day after the last one this programme occupies.
    ///
    /// Exclusive, so that a programme starting the Monday after another ends is
    /// adjacent rather than overlapping — which is the common case and must not
    /// be refused.
    #[must_use]
    pub fn end(&self) -> Date {
        let span = jiff::Span::new().days(i64::from(self.calendar_weeks).saturating_mul(7));
        self.start.checked_add(span).unwrap_or(self.start)
    }

    /// Whether two programmes compete for a day.
    ///
    /// **Versions of one programme never do.** Two windows sharing a name are
    /// the same programme re-authored, and only the latest is ever read, so they
    /// are permitted to sit on top of each other — which is what makes the
    /// existing store's five authorings legal without deleting any of them.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        if self.name == other.name {
            return false;
        }
        self.start < other.end() && other.start < self.end()
    }

    /// Whether this programme is the one that answers for a date.
    #[must_use]
    pub fn covers(&self, date: Date) -> bool {
        date >= self.start && date < self.end()
    }
}

impl fmt::Display for ProgrammeWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({} to {})",
            self.name,
            self.start,
            self.end().yesterday().unwrap_or(self.start)
        )
    }
}
