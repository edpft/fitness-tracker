//! When there is room to train, and where.
//!
//! **Operator-level, not programme-level.** This is a fact about a life — when
//! work, family and social commitments leave room to train, and which zone that
//! happens in. Every discipline reads it; none owns it.
//!
//! **And the allocation lives here too.** Which slots are the gym's and which
//! are cycling's was going to be somebody else's business, on the grounds that
//! splitting a week between disciplines is planning rather than fact. It is
//! not, and an alteration is why: a trip where the hotel gym is only free at
//! the weekend turns two weekday evenings into a Saturday morning, and the
//! allocation has to move with them. Anything holding it elsewhere would need
//! to know about alterations as well, which is the knowledge this module exists
//! to keep in one place.
//!
//! What is still not here is *choosing* the split. Recording that Monday
//! evening is the gym's is a fact; deciding it should be weighs cycling,
//! nutrition and the family calendar, and sits above this.
//!
//! ## Two kinds of slot, and this is the other one
//!
//! A [`TrainingSlot`] is a time — Monday evening. `prescription::SlotId` is a
//! position in a session — the knee-dominant one. Both are things to be filled,
//! which is why both were called a slot and why neither should be called only
//! that.
//!
//! ## A training slot is interchangeable time, not merely free time
//!
//! That is the whole of the distinction. A padel game on a Sunday evening
//! occupies the day and is *not* a slot, because it cannot be swapped with
//! Monday's — it constrains the week without joining the pool. What is recorded
//! here is the residue: the times something could be *scheduled into*.
//!
//! ## Two shapes, and they are not versions of each other
//!
//! A [`TrainingPattern`] is the ordinary run of a week, in force from a date
//! until something supersedes it. An [`Alteration`] is a run of days that
//! departs from it.
//!
//! **An alteration is not a holiday.** A course, a visitor, a late finish and a
//! fortnight in Rome all change a week, and only one of them is a trip. The
//! type says what it does — this run of days differs, and here is why.
//!
//! They are held apart rather than nested, because an alteration is a fact
//! about dates and not about which pattern happened to be in force when it was
//! recorded. Nesting them would lose every alteration already recorded the next
//! time the ordinary pattern changed.
//!
//! ## What an alteration can say
//!
//! Both of its fields are optional, and the combinations are the cases the
//! operator actually has:
//!
//! - **zone only** — away, training as usual, in another country.
//! - **no slots** — unable to train at all. The hard case.
//! - **different slots** — able to train at times the ordinary pattern does not
//!   offer. A Friday evening becomes a Saturday morning, which is possible on
//!   holiday and not in ordinary life. It is also how half a day is said: keep
//!   the morning, lose the rest.
//!
//! `None` is "unchanged" and `Some` of an empty set is "none at all". Those are
//! different facts, and collapsing them would make a zone-only alteration
//! cancel every session of the trip.

use std::{collections::BTreeMap, num::NonZeroU8};

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

/// What a training slot is given to.
///
/// **The activity, never the vendor.** Cycling is cycling whether the bike is a
/// Peloton, a turbo trainer or a road; naming the member after the app that
/// happens to record it would make the vocabulary a shape of a source, which is
/// the one thing § II.3 rules out. The same reason the exercise vocabulary is
/// ours rather than Hevy's.
///
/// Closed, and every slot names one. An unclaimed slot is not representable
/// here: a time nobody is going to use is not a training slot, it is a free
/// evening, and the pool is the times something *could* be scheduled into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Discipline {
    Gym,
    Cycling,
}

impl Discipline {
    pub const ALL: &'static [Self] = &[Self::Gym, Self::Cycling];

    /// The stable key. Persisted, so it outlives a rename.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gym => "gym",
            Self::Cycling => "cycling",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} does not name a discipline")]
pub struct UnknownDiscipline {
    value: String,
}

impl TryFrom<String> for Discipline {
    type Error = UnknownDiscipline;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::ALL
            .iter()
            .find(|discipline| discipline.as_str() == value)
            .copied()
            .ok_or(UnknownDiscipline { value })
    }
}

impl std::fmt::Display for Discipline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A time the operator is free to train, and could train instead of another.
///
/// Ordered by weekday then part of day, so a set of them reads as a week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrainingSlot {
    pub weekday: Weekday,
    pub part: PartOfDay,
}

impl TrainingSlot {
    pub const fn new(weekday: Weekday, part: PartOfDay) -> Self {
        Self { weekday, part }
    }
}

impl PartialOrd for TrainingSlot {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TrainingSlot {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Monday first, which `jiff` does not order by on its own.
        self.weekday
            .to_monday_zero_offset()
            .cmp(&other.weekday.to_monday_zero_offset())
            .then(self.part.cmp(&other.part))
    }
}

impl std::fmt::Display for TrainingSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} {}", self.weekday, self.part)
    }
}

/// The ordinary run of a week, in force from a date.
///
/// **Open-ended.** It runs until something supersedes it, because a routine does
/// not have an end date — it has a successor. Which is also why nothing here
/// carries one: an end would be a second place for the same fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainingPattern {
    from: Date,
    zone: OperatorZone,
    slots: BTreeMap<TrainingSlot, Discipline>,
}

impl TrainingPattern {
    /// A schedule may have no slots — a period with no room to train at all is a
    /// real thing to record, and refusing it would mean pretending otherwise.
    pub const fn new(
        from: Date,
        zone: OperatorZone,
        slots: BTreeMap<TrainingSlot, Discipline>,
    ) -> Self {
        Self { from, zone, slots }
    }

    pub const fn from(&self) -> Date {
        self.from
    }

    pub const fn zone(&self) -> &OperatorZone {
        &self.zone
    }

    pub const fn slots(&self) -> &BTreeMap<TrainingSlot, Discipline> {
        &self.slots
    }
}

/// A run of days that departs from the ordinary pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alteration {
    start: Date,
    days: NonZeroU8,
    zone: Option<OperatorZone>,
    slots: Option<BTreeMap<TrainingSlot, Discipline>>,
    /// Why. An unexplained override is unreadable six months later — § II.2's
    /// obligation on an edit overlay, which this is the authored-data analogue
    /// of.
    reason: String,
}

impl Alteration {
    pub const fn new(
        start: Date,
        days: NonZeroU8,
        zone: Option<OperatorZone>,
        slots: Option<BTreeMap<TrainingSlot, Discipline>>,
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

    pub const fn slots(&self) -> Option<&BTreeMap<TrainingSlot, Discipline>> {
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
    pub slots: BTreeMap<TrainingSlot, Discipline>,
}

impl Availability {
    /// Is this date one the operator could train on at all, by any discipline?
    ///
    /// Rarely the question worth asking: a day open to cycling is not a day the
    /// gym can use. [`Self::for_discipline`] is what a programme wants.
    pub fn open(&self, date: Date) -> bool {
        self.slots.keys().any(|slot| slot.weekday == date.weekday())
    }

    /// This date's slots belonging to one discipline.
    pub fn for_discipline(
        &self,
        discipline: Discipline,
        date: Date,
    ) -> impl Iterator<Item = TrainingSlot> + '_ {
        self.slots
            .iter()
            .filter(move |(slot, allocated)| {
                **allocated == discipline && slot.weekday == date.weekday()
            })
            .map(|(slot, _)| *slot)
    }
}

/// Everything the operator has said about their week.
///
/// Holds both shapes because answering "what does this day look like" needs
/// both, and a caller assembling them itself would be the second place the rule
/// lived.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diary {
    patterns: Vec<TrainingPattern>,
    alterations: Vec<Alteration>,
}

impl Diary {
    /// Schedules are sorted on the way in, so `on` can take the last that
    /// applies without every caller having to have sorted them.
    pub fn new(mut patterns: Vec<TrainingPattern>, alterations: Vec<Alteration>) -> Self {
        patterns.sort_by_key(TrainingPattern::from);
        Self {
            patterns,
            alterations,
        }
    }

    pub fn patterns(&self) -> &[TrainingPattern] {
        &self.patterns
    }

    pub fn alterations(&self) -> &[Alteration] {
        &self.alterations
    }

    /// What a date looks like: the pattern in force, as amended by any
    /// alteration covering it.
    ///
    /// `None` before the first schedule begins — a date the operator has said
    /// nothing about is unknown, not empty, and inventing a week for it would be
    /// asserting a fact nobody stated.
    ///
    /// **The last alteration to cover the date wins**, which matters only where two
    /// overlap. Refusing an overlap would be stricter and worse: a long trip
    /// with a different arrangement in the middle of it is a perfectly ordinary
    /// thing to describe that way.
    pub fn on(&self, date: Date) -> Option<Availability> {
        let pattern = self.pattern_on(date)?;

        let mut availability = Availability {
            zone: pattern.zone().clone(),
            slots: pattern.slots().clone(),
        };

        for alteration in self
            .alterations
            .iter()
            .filter(|alteration| alteration.covers(date))
        {
            if let Some(zone) = alteration.zone() {
                availability.zone.clone_from(zone);
            }
            if let Some(slots) = alteration.slots() {
                availability.slots.clone_from(slots);
            }
        }

        Some(availability)
    }

    /// Every date in a range on which a discipline ordinarily trains and no
    /// longer can.
    ///
    /// **The allocation is read here rather than passed in**, which is the whole
    /// reason this module owns it. An alteration may replace the week's slots
    /// outright — a holiday that turns Monday and Friday evenings into a
    /// Saturday morning — and a caller holding a fixed set of its own slots
    /// would find none of them and report everything lost, while the Saturday
    /// sat unclaimed. Only something that knows both the pattern and the
    /// alterations can answer.
    ///
    /// **A day is lost when the discipline's slot is gone, not when the day
    /// empties.** Parts of a day are the whole reason a slot is a weekday and a
    /// part: the operator trains Monday morning and goes away at lunchtime, and
    /// a programme holding the Monday evening has lost that Monday however open
    /// the morning still is. A day the *other* discipline keeps is lost too.
    ///
    /// **A moved slot is a loss here and a gain elsewhere.** If the holiday
    /// above gives the gym its Saturday morning, this still reports the Monday
    /// and the Friday: they are days the programme cannot run. What it does
    /// with the Saturday is [`Self::slots_on`] and a decision, and deciding is
    /// not this module's business.
    pub fn unavailable(&self, from: Date, until: Date, discipline: Discipline) -> Vec<Date> {
        let mut lost = Vec::new();
        let mut cursor = from;

        while cursor <= until {
            // The baseline is the pattern in force, unaltered: "ordinarily" is
            // what the week says, and the alteration is what happened to it.
            let ordinarily: Vec<TrainingSlot> = self
                .pattern_on(cursor)
                .map(|pattern| {
                    pattern
                        .slots()
                        .iter()
                        .filter(|(slot, allocated)| {
                            **allocated == discipline && slot.weekday == cursor.weekday()
                        })
                        .map(|(slot, _)| *slot)
                        .collect()
                })
                .unwrap_or_default();

            if !ordinarily.is_empty() {
                let kept = self.on(cursor).is_some_and(|availability| {
                    ordinarily
                        .iter()
                        .any(|slot| availability.slots.get(slot) == Some(&discipline))
                });
                if !kept {
                    lost.push(cursor);
                }
            }

            let Ok(next) = cursor.tomorrow() else { break };
            cursor = next;
        }

        lost
    }

    /// A discipline's slots on one date, after every alteration covering it.
    ///
    /// The other half of [`Self::unavailable`]: what a programme *has*, rather
    /// than what it lost.
    pub fn slots_on(&self, date: Date, discipline: Discipline) -> Vec<TrainingSlot> {
        self.on(date)
            .map(|availability| availability.for_discipline(discipline, date).collect())
            .unwrap_or_default()
    }

    /// The pattern in force on a date, before any alteration.
    /// The weekdays one discipline ordinarily holds, as of a date.
    ///
    /// **Ordinary, so alterations are not applied.** A programme's weekdays are
    /// its weekly shape; an alteration is a run of days that departs from that
    /// shape, and the calendar already takes those out as skips. Applying them
    /// here would let a holiday covering the start date decide the shape of
    /// every week after it — and the autumn block starts inside one.
    ///
    /// Monday first, and one entry per weekday however many slots that day
    /// holds: a discipline given both a morning and an evening on a Saturday
    /// still trains on one Saturday.
    ///
    /// `None` before the first pattern begins, for the reason [`Self::on`]
    /// gives: a date the operator has said nothing about is unknown rather than
    /// empty. `Some(vec![])` is the different fact that the week is known and
    /// holds nothing for this discipline.
    pub fn ordinarily(&self, date: Date, discipline: Discipline) -> Option<Vec<Weekday>> {
        let pattern = self.pattern_on(date)?;
        // `TrainingSlot` orders Monday-first and then by part of day, so the
        // map is already in the order this returns and equal weekdays are
        // adjacent.
        let mut days: Vec<Weekday> = pattern
            .slots()
            .iter()
            .filter(|(_, held)| **held == discipline)
            .map(|(slot, _)| slot.weekday)
            .collect();
        days.dedup_by_key(|weekday| weekday.to_monday_zero_offset());
        Some(days)
    }

    fn pattern_on(&self, date: Date) -> Option<&TrainingPattern> {
        self.patterns.iter().rfind(|pattern| pattern.from() <= date)
    }
}
