//! School holidays and public holidays: when the ordinary week cannot be
//! assumed to hold.
//!
//! **Facts about the world, not about the operator** (#181). The operator,
//! 2026-09-22: *"like school holiday, during public holidays, we cannot assume
//! that the normal rules will continue to apply, even if I haven't specifically
//! said something will change"*. So neither is an alteration and neither is
//! typed in: both are read from the bodies that set them, and nothing here is
//! stored (§ 14).
//!
//! **What they are for is whether a run of days is safe to plan into**, not
//! what happens on any one of them. The case that raised it is the autumn
//! plan's last mesocycle, pushed back a week into Christmas: the adaptation is
//! to drop the mesocycle, not to programme around the holiday.

use std::num::NonZeroU8;

use jiff::civil::Date;

/// A run of days the school is on holiday.
///
/// **No name**, because the school's own calendar gives none worth keeping:
/// every holiday in it, Christmas and summer included, is titled "Half term".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchoolHoliday {
    start: Date,
    days: NonZeroU8,
}

impl SchoolHoliday {
    pub const fn new(start: Date, days: NonZeroU8) -> Self {
        Self { start, days }
    }

    pub const fn start(&self) -> Date {
        self.start
    }

    pub const fn days(&self) -> NonZeroU8 {
        self.days
    }

    /// The last day this covers.
    pub fn last(&self) -> Date {
        self.start
            .checked_add(jiff::Span::new().days(i64::from(self.days.get()) - 1))
            .unwrap_or(self.start)
    }
}

/// A public holiday: one day, and what it is called.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PublicHoliday {
    date: Date,
    name: String,
}

impl PublicHoliday {
    pub const fn new(date: Date, name: String) -> Self {
        Self { date, name }
    }

    pub const fn date(&self) -> Date {
        self.date
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Every school and public holiday the sources know about, and how far each
/// source reaches.
///
/// **Past a source's reach there is no data, which is not the same as no
/// holiday.** The school's calendar has a fixed end: in September 2026 it runs
/// to the summer holiday of 2027 and says nothing after it. The operator,
/// 2026-09-22: *"That shouldn't be read as no holiday, it should be read as no
/// data."* So each kind carries the last day its source speaks for, and a
/// question beyond it is answered with `None`.
///
/// `None` for a reach is a source that has published nothing of that kind at
/// all, which reaches nowhere.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Holidays {
    school: Vec<SchoolHoliday>,
    public: Vec<PublicHoliday>,
    school_to: Option<Date>,
    public_to: Option<Date>,
}

impl Holidays {
    /// A school's holidays, published to a date.
    pub fn from_school(mut holidays: Vec<SchoolHoliday>, published_to: Option<Date>) -> Self {
        holidays.sort();
        Self {
            school: holidays,
            school_to: published_to,
            ..Self::default()
        }
    }

    /// Public holidays, published to a date.
    pub fn from_public(mut holidays: Vec<PublicHoliday>, published_to: Option<Date>) -> Self {
        holidays.sort();
        Self {
            public: holidays,
            public_to: published_to,
            ..Self::default()
        }
    }

    /// Both sources' holidays as one. Each source knows only its own kind, and
    /// this is where they meet.
    ///
    /// Where both speak for one kind, the nearer reach stands: beyond it, one
    /// of them has nothing to say.
    #[must_use]
    pub fn and(self, other: Self) -> Self {
        let nearer = |one: Option<Date>, two: Option<Date>| match (one, two) {
            (Some(one), Some(two)) => Some(one.min(two)),
            (one, two) => one.or(two),
        };
        let mut school: Vec<SchoolHoliday> = self.school.into_iter().chain(other.school).collect();
        let mut public: Vec<PublicHoliday> = self.public.into_iter().chain(other.public).collect();
        school.sort();
        public.sort();
        Self {
            school,
            public,
            school_to: nearer(self.school_to, other.school_to),
            public_to: nearer(self.public_to, other.public_to),
        }
    }

    pub fn school(&self) -> &[SchoolHoliday] {
        &self.school
    }

    pub fn public(&self) -> &[PublicHoliday] {
        &self.public
    }

    /// The last day the school's calendar speaks for.
    pub const fn school_published_to(&self) -> Option<Date> {
        self.school_to
    }

    /// The last day the public holidays are published for.
    pub const fn public_published_to(&self) -> Option<Date> {
        self.public_to
    }

    /// Only the holidays that have not ended before a date. The reach is
    /// unchanged.
    #[must_use]
    pub fn since(self, date: Date) -> Self {
        Self {
            school: self
                .school
                .into_iter()
                .filter(|holiday| holiday.last() >= date)
                .collect(),
            public: self
                .public
                .into_iter()
                .filter(|holiday| holiday.date() >= date)
                .collect(),
            ..self
        }
    }

    /// Whether any day from `first` to `last` falls in a school or public
    /// holiday — the question a plan asks of a week it means to put a
    /// mesocycle in.
    ///
    /// `Some(true)` wherever a holiday is known to fall, whatever else is
    /// unknown. `None` where either source has not published as far as `last`:
    /// there is no data, and reading that as no holiday is the one answer
    /// this must not give. `Some(false)` only where both speak for every day.
    pub fn touches(&self, first: Date, last: Date) -> Option<bool> {
        let known = self
            .school
            .iter()
            .any(|holiday| holiday.start() <= last && holiday.last() >= first)
            || self
                .public
                .iter()
                .any(|holiday| holiday.date() >= first && holiday.date() <= last);
        if known {
            return Some(true);
        }

        let reaches = |to: Option<Date>| to.is_some_and(|to| to >= last);
        (reaches(self.school_to) && reaches(self.public_to)).then_some(false)
    }
}
