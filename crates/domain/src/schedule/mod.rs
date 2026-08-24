//! What the operator's week looks like, and where they are.
//!
//! **Operator-level, not programme-level.** A schedule is a fact about a life —
//! when work, family and social commitments leave room to train, and which zone
//! that happens in. Every discipline reads it; none owns it. The gym programme
//! is told which of these slots it may use, and that allocation is planning
//! rather than fact (see `docs/`), so nothing here allocates anything.
//!
//! ## A slot is interchangeable time, not merely free time
//!
//! That is the whole of the distinction. A padel game on a Sunday evening
//! occupies the day and is *not* a slot, because it cannot be swapped with
//! Monday's — it constrains the week without joining the pool. What is recorded
//! here is the residue: the times something could be *scheduled into*.
//!
//! ## Two shapes, and they are not versions of each other
//!
//! A [`Schedule`] is the ordinary week, in force from a date until something
//! supersedes it. A [`Patch`] is a run of days that departs from it — a holiday.
//!
//! They are held apart rather than nested, because a holiday is a fact about
//! dates and not about which version of the ordinary week happened to be in
//! force. Nesting them would lose every booked holiday the next time the
//! ordinary week changed.
//!
//! ## What a patch can say
//!
//! Both of its fields are optional, and the combinations are the cases the
//! operator actually has:
//!
//! - **zone only** — away, training as usual, in another country.
//! - **no slots** — away and unable to train. The hard case.
//! - **different slots** — away, but able to train at times the ordinary week
//!   does not offer. A Friday evening becomes a Saturday morning, which is
//!   possible on holiday and not in ordinary life.
//!
//! `None` is "unchanged" and `Some` of an empty set is "none at all". Those are
//! different facts and collapsing them would make a zone-only patch cancel every
//! session.

use std::{collections::BTreeSet, num::NonZeroU8};

use jiff::civil::{Date, Weekday};

use crate::gym::OperatorZone;

/// Roughly when in the day, as the operator says it.
///
/// A closed vocabulary rather than a time of day, because "Monday evening" is
/// what a life is actually planned in. A range would be false precision: nothing
/// here needs to know that the evening starts at six, and pretending to would
/// invite a session at 17:59 being refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PartOfDay {
    Morning,
    Afternoon,
    Evening,
}

impl PartOfDay {
    pub const ALL: &'static [Self] = &[Self::Morning, Self::Afternoon, Self::Evening];

    /// The stable key. Persisted, so it outlives a rename.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Morning => "morning",
            Self::Afternoon => "afternoon",
            Self::Evening => "evening",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} does not name a part of a day")]
pub struct UnknownPartOfDay {
    value: String,
}

impl TryFrom<String> for PartOfDay {
    type Error = UnknownPartOfDay;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::ALL
            .iter()
            .find(|part| part.as_str() == value)
            .copied()
            .ok_or(UnknownPartOfDay { value })
    }
}

impl std::fmt::Display for PartOfDay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A time the operator is free to train, and could train instead of another.
///
/// Ordered by weekday then part of day, so a set of them reads as a week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Slot {
    pub weekday: Weekday,
    pub part: PartOfDay,
}

impl Slot {
    pub const fn new(weekday: Weekday, part: PartOfDay) -> Self {
        Self { weekday, part }
    }
}

impl PartialOrd for Slot {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Slot {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Monday first, which `jiff` does not order by on its own.
        self.weekday
            .to_monday_zero_offset()
            .cmp(&other.weekday.to_monday_zero_offset())
            .then(self.part.cmp(&other.part))
    }
}

impl std::fmt::Display for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} {}", self.weekday, self.part)
    }
}

/// The ordinary week, in force from a date.
///
/// **Open-ended.** It runs until something supersedes it, because a routine does
/// not have an end date — it has a successor. Which is also why nothing here
/// carries one: an end would be a second place for the same fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    from: Date,
    zone: OperatorZone,
    slots: BTreeSet<Slot>,
}

impl Schedule {
    /// A schedule may have no slots — a period with no room to train at all is a
    /// real thing to record, and refusing it would mean pretending otherwise.
    pub const fn new(from: Date, zone: OperatorZone, slots: BTreeSet<Slot>) -> Self {
        Self { from, zone, slots }
    }

    pub const fn from(&self) -> Date {
        self.from
    }

    pub const fn zone(&self) -> &OperatorZone {
        &self.zone
    }

    pub const fn slots(&self) -> &BTreeSet<Slot> {
        &self.slots
    }
}

/// A run of days that departs from the ordinary week.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    start: Date,
    days: NonZeroU8,
    zone: Option<OperatorZone>,
    slots: Option<BTreeSet<Slot>>,
    /// Why. An unexplained override is unreadable six months later — § II.2's
    /// obligation on an edit overlay, which this is the authored-data analogue
    /// of.
    reason: String,
}

impl Patch {
    pub const fn new(
        start: Date,
        days: NonZeroU8,
        zone: Option<OperatorZone>,
        slots: Option<BTreeSet<Slot>>,
        reason: String,
    ) -> Self {
        Self {
            start,
            days,
            zone,
            slots,
            reason,
        }
    }

    pub const fn start(&self) -> Date {
        self.start
    }

    pub const fn days(&self) -> NonZeroU8 {
        self.days
    }

    pub const fn zone(&self) -> Option<&OperatorZone> {
        self.zone.as_ref()
    }

    pub const fn slots(&self) -> Option<&BTreeSet<Slot>> {
        self.slots.as_ref()
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// The last day this covers.
    pub fn last(&self) -> Date {
        self.start
            .checked_add(jiff::Span::new().days(i64::from(self.days.get()) - 1))
            .unwrap_or(self.start)
    }

    pub fn covers(&self, date: Date) -> bool {
        date >= self.start && date <= self.last()
    }
}

/// What a given day actually looks like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Availability {
    pub zone: OperatorZone,
    pub slots: BTreeSet<Slot>,
}

impl Availability {
    /// Is this date one the operator could train on at all?
    pub fn open(&self, date: Date) -> bool {
        self.slots.iter().any(|slot| slot.weekday == date.weekday())
    }
}

/// Everything the operator has said about their week.
///
/// Holds both shapes because answering "what does this day look like" needs
/// both, and a caller assembling them itself would be the second place the rule
/// lived.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diary {
    schedules: Vec<Schedule>,
    patches: Vec<Patch>,
}

impl Diary {
    /// Schedules are sorted on the way in, so `on` can take the last that
    /// applies without every caller having to have sorted them.
    pub fn new(mut schedules: Vec<Schedule>, patches: Vec<Patch>) -> Self {
        schedules.sort_by_key(Schedule::from);
        Self { schedules, patches }
    }

    pub fn schedules(&self) -> &[Schedule] {
        &self.schedules
    }

    pub fn patches(&self) -> &[Patch] {
        &self.patches
    }

    /// What a date looks like: the ordinary week in force, as amended by any
    /// patch covering it.
    ///
    /// `None` before the first schedule begins — a date the operator has said
    /// nothing about is unknown, not empty, and inventing a week for it would be
    /// asserting a fact nobody stated.
    ///
    /// **The last patch to cover the date wins**, which matters only where two
    /// overlap. Refusing an overlap would be stricter and worse: a long trip
    /// with a different arrangement in the middle of it is a perfectly ordinary
    /// thing to describe that way.
    pub fn on(&self, date: Date) -> Option<Availability> {
        let schedule = self
            .schedules
            .iter()
            .rfind(|schedule| schedule.from() <= date)?;

        let mut availability = Availability {
            zone: schedule.zone().clone(),
            slots: schedule.slots().clone(),
        };

        for patch in self.patches.iter().filter(|patch| patch.covers(date)) {
            if let Some(zone) = patch.zone() {
                availability.zone.clone_from(zone);
            }
            if let Some(slots) = patch.slots() {
                availability.slots.clone_from(slots);
            }
        }

        Some(availability)
    }

    /// Every date in a range the operator could not train on, given the slots a
    /// programme has been allocated.
    ///
    /// **This is what a programme consults, and it takes the allocation rather
    /// than reading the pool.** Half the operator's slots may belong to another
    /// discipline entirely, so a programme asking "which of *my* days do I lose"
    /// is the only question it can answer without knowing about the rest.
    /// **A day is lost when the allocated *slot* is gone, not when the day
    /// empties.** Parts of a day are the whole reason a slot is a weekday and a
    /// part rather than a weekday: the operator trains on Monday morning and
    /// goes away at lunchtime, and a programme holding the Monday evening has
    /// lost that Monday however open the morning still is.
    ///
    /// Asking [`Availability::open`] instead answers "could the operator train
    /// at all", which is a different question and the wrong one here — it read
    /// a surviving morning as a surviving evening and reported nothing lost.
    pub fn unavailable(&self, from: Date, until: Date, allocated: &BTreeSet<Slot>) -> Vec<Date> {
        let mut lost = Vec::new();
        let mut cursor = from;

        while cursor <= until {
            let mine: Vec<&Slot> = allocated
                .iter()
                .filter(|slot| slot.weekday == cursor.weekday())
                .collect();

            if !mine.is_empty() {
                let available = self.on(cursor).is_some_and(|availability| {
                    mine.iter().any(|slot| availability.slots.contains(slot))
                });
                if !available {
                    lost.push(cursor);
                }
            }

            let Ok(next) = cursor.tomorrow() else { break };
            cursor = next;
        }

        lost
    }
}
