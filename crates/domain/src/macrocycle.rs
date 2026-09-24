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
