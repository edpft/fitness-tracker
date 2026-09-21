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
//! **Every alteration is an absence, and there are two kinds.** A holiday takes
//! the operator out of the routine — away from the gym and the bike, perhaps in
//! another zone, with whatever slots the trip allows. Illness is only illness
//! when it prevents training, so it has no slots at all. A late finish or a
//! session moved to the next morning is neither: a session can be performed
//! any time in its window, and the scheduling side need not know. A lasting
//! change is a new [`TrainingPattern`].
//!
//! They are held apart rather than nested, because an alteration is a fact
//! about dates and not about which pattern happened to be in force when it was
//! recorded. Nesting them would lose every alteration already recorded the next
//! time the ordinary pattern changed.
//!
//! ## What an absence can say
//!
//! - **a holiday that keeps the slots** — away, perhaps in another zone,
//!   training at the usual times.
//! - **a holiday with no slots** — unable to train at all.
//! - **a holiday with different slots** — able to train at times the ordinary
//!   pattern does not offer. A Friday evening becomes a Saturday morning, which
//!   is possible on holiday and not in ordinary life. It is also how half a day
//!   is said: keep the morning, lose the rest.
//! - **illness** — unable to train at all, wherever the operator is. If it runs
//!   on, the illness is extended rather than a second one recorded beside it.
//!
//! A holiday's `None` slots are "the ordinary week stands" and `Some` of an
//! empty set is "none at all". Those are different facts, and collapsing them
//! would make training away as usual cancel every session of the trip.

mod role;

use std::{collections::BTreeMap, num::NonZeroU8};

use jiff::civil::{Date, Weekday};

use crate::normalised::OperatorZone;

pub use role::{Relative, SessionRole, UnknownRelative};

/// Roughly when in the day, as the operator says it.
///
/// A closed vocabulary rather than a time of day, because "Monday evening" is
/// what a life is actually planned in.
///
/// **It has hours, and until 2026-09-20 it deliberately did not.** The doc here
/// argued that a range would be false precision and "would invite a session at
/// 17:59 being refused". The argument survives, narrowed to what it was
/// actually about: the hours answer *how much of this window is left*, and they
/// never answer *may this session be performed now*. Nothing refuses a session
/// for being early. What needed them is #185 — on a Saturday evening there is
/// still time for Friday's gym test, and on the Sunday morning there is not,
/// and neither sentence is sayable without knowing when an evening ends.
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

    /// Which part a wall-clock time falls in: 06:00–11:59, 12:00–17:59,
    /// 18:00–23:59.
    ///
    /// `None` from midnight to 05:59, which is not a part of anybody's training
    /// day. [`DayPart::containing`] is what a caller with a moment in hand
    /// wants, and says what it does with those six hours.
    #[must_use]
    pub fn of(time: jiff::civil::Time) -> Option<Self> {
        match time.hour() {
            6..=11 => Some(Self::Morning),
            12..=17 => Some(Self::Afternoon),
            18..=23 => Some(Self::Evening),
            _ => None,
        }
    }

    /// The part after this one, within the same day.
    #[must_use]
    pub const fn after(self) -> Option<Self> {
        match self {
            Self::Morning => Some(Self::Afternoon),
            Self::Afternoon => Some(Self::Evening),
            Self::Evening => None,
        }
    }
}

/// A moment in the grain the diary is kept in: a date, and a part of it.
///
/// **Ordered, and that is the whole point of the type.** "Is there still time
/// for Friday's session?" is a comparison between where the day has got to and
/// where the next session starts, and a date beside a part of a day compares
/// wrongly as often as not unless the pair is one value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DayPart {
    pub date: Date,
    pub part: PartOfDay,
}

impl DayPart {
    pub const fn new(date: Date, part: PartOfDay) -> Self {
        Self { date, part }
    }

    /// Where a moment sits, for counting what is left of a window.
    ///
    /// **A part in progress counts as still usable until it ends**, which is
    /// the operator's own reading: at eight on a Saturday evening there is *"in
    /// theory"* still time for Friday's gym test. So this answers with the part
    /// the clock is in rather than the next one.
    ///
    /// **Before 06:00 the answer is that morning.** The day has not begun and
    /// none of it has been spent, so nothing is lost by saying so — where
    /// answering "yesterday evening" would hand back a part that has in fact
    /// ended, and an `Option` would push six hours of every day onto every
    /// caller.
    #[must_use]
    pub fn containing(at: jiff::civil::DateTime) -> Self {
        Self {
            date: at.date(),
            part: PartOfDay::of(at.time()).unwrap_or(PartOfDay::Morning),
        }
    }

    /// The part of a day after this one, rolling into tomorrow morning.
    #[must_use]
    pub fn next(self) -> Option<Self> {
        self.part.after().map_or_else(
            || {
                self.date
                    .tomorrow()
                    .ok()
                    .map(|date| Self::new(date, PartOfDay::Morning))
            },
            |part| Some(Self::new(self.date, part)),
        )
    }

    /// The moment a slot on a date begins.
    #[must_use]
    pub const fn of_slot(date: Date, slot: TrainingSlot) -> Self {
        Self::new(date, slot.part)
    }
}

impl std::fmt::Display for DayPart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.date, self.part)
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

/// What a training slot is given to, and as what.
///
/// **Two facts in one value because they are one decision.** Sunday morning is
/// cycling's *and* it is the longer ride — it is the only slot long enough, and
/// that is the same sentence twice. Holding them apart would let them drift,
/// which is what `cycling_weekday` and `gym_weekday` did eight times over per
/// discipline (issue #63).
///
/// **The role is the operator's input, not a derivation.** Deriving it would
/// need a slot's capacity, the fact that a Sunday morning is extendable where a
/// weeknight is not, and a concept of commitments this system does not have.
/// The operator, 2026-09-16: the allocation *"is not likely to change
/// frequently"*, so it is stated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Allocation {
    pub discipline: Discipline,
    pub role: SessionRole,
}

impl Allocation {
    pub const fn new(discipline: Discipline, role: SessionRole) -> Self {
        Self { discipline, role }
    }
}

impl std::fmt::Display for Allocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.discipline, self.role)
    }
}

/// One discipline's ordinary week: which weekdays it trains, and the role each
/// of those days is held for.
///
/// **Derived from the diary on every read, and never stored.** The gym kept its
/// own copy in `gym_weekday` until 2026-09-20 — the same two rows written once
/// per mesocycle, eight times over for the autumn, with nothing keeping them in
/// step with the week the operator actually trains (issue #63). The shape of
/// the type was never the problem; a programme owning a copy of it was.
///
/// **A block's calendar needs it and does not hold it.** Whether a calendar
/// week counts as a training week depends on whether any of its days runs, so
/// the question cannot be answered without this — and it is a fact about a
/// life, not about a block, so it is handed in rather than kept.
///
/// **Non-empty**, because a programme that runs on no day issues nothing. A
/// week that is known and holds nothing for a discipline is a real answer — it
/// is [`Diary::ordinarily`] returning an empty vector — but it is not a
/// training week and it cannot build a calendar.
///
/// A list rather than a map because `jiff`'s `Weekday` is deliberately not
/// `Ord`: a week has no universal first day. At most a handful of entries, so a
/// scan is the whole cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrainingWeek {
    /// Monday-first, one entry per weekday: a discipline given both a morning
    /// and an evening on a Saturday still trains on one Saturday, and the
    /// earlier slot's role is the one that stands.
    days: Vec<(Weekday, SessionRole)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a programme that runs on no day of the week issues nothing")]
pub struct NoTrainingDays;

impl TrainingWeek {
    /// # Errors
    ///
    /// [`NoTrainingDays`] if the discipline trains on no day at all.
    pub fn new(days: Vec<(Weekday, SessionRole)>) -> Result<Self, NoTrainingDays> {
        let mut days = days;
        days.sort_by_key(|(weekday, _)| weekday.to_monday_zero_offset());
        days.dedup_by_key(|(weekday, _)| weekday.to_monday_zero_offset());
        if days.is_empty() {
            return Err(NoTrainingDays);
        }
        Ok(Self { days })
    }

    #[must_use]
    pub fn runs(&self, day: Weekday) -> bool {
        self.days.iter().any(|(weekday, _)| *weekday == day)
    }

    #[must_use]
    pub fn role_on(&self, day: Weekday) -> Option<SessionRole> {
        self.days
            .iter()
            .find(|(weekday, _)| *weekday == day)
            .map(|(_, role)| *role)
    }

    /// Whether the week holds a session in this role at all.
    ///
    /// A programme gating on a role the week never offers would never advance,
    /// which is the one thing the types cannot catch between them.
    #[must_use]
    pub fn offers(&self, role: SessionRole) -> bool {
        self.days.iter().any(|(_, held)| *held == role)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Weekday, SessionRole)> + '_ {
        self.days.iter().copied()
    }

    /// The week in a sentence, for a refusal that says what a programme does
    /// run rather than only what it does not.
    #[must_use]
    pub fn describe(&self) -> String {
        let named: Vec<String> = self
            .days
            .iter()
            .map(|(day, role)| format!("{day:?} ({role})"))
            .collect();
        match named.split_last() {
            None => String::new(),
            Some((last, [])) => last.clone(),
            Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        }
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
    slots: BTreeMap<TrainingSlot, Allocation>,
}

impl TrainingPattern {
    /// A schedule may have no slots — a period with no room to train at all is a
    /// real thing to record, and refusing it would mean pretending otherwise.
    pub const fn new(
        from: Date,
        zone: OperatorZone,
        slots: BTreeMap<TrainingSlot, Allocation>,
    ) -> Self {
        Self { from, zone, slots }
    }

    pub const fn from(&self) -> Date {
        self.from
    }

    pub const fn zone(&self) -> &OperatorZone {
        &self.zone
    }

    pub const fn slots(&self) -> &BTreeMap<TrainingSlot, Allocation> {
        &self.slots
    }
}

/// Why a run of days departs from the ordinary pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Absence {
    /// Out of the routine. A `None` zone is "the zone is unchanged", which is
    /// not the same as any zone; `None` slots are "the ordinary week stands",
    /// and an empty set is "no room to train at all".
    ///
    /// **Only a holiday is asked why.** It has somewhere to be and a week to
    /// rearrange, so "Rome" is what makes the rearrangement readable six months
    /// later. Illness explains itself.
    Holiday {
        zone: Option<OperatorZone>,
        slots: Option<BTreeMap<TrainingSlot, Allocation>>,
        reason: String,
    },
    /// Too ill to train. Bad enough to prevent training is what makes it
    /// illness, so it has no slots, and where the operator is does not matter.
    Illness,
}

impl Absence {
    pub const fn as_str(&self) -> &'static str {
        self.kind().as_str()
    }

    /// Which of the two this is, without what it carries.
    ///
    /// **What a session's state needs and all it needs.** A session skipped for
    /// a holiday and one skipped for an illness are different facts worth
    /// telling apart; the zone, the slots and the reason belong to the
    /// alteration and say nothing about the session.
    pub const fn kind(&self) -> AbsenceKind {
        match self {
            Self::Holiday { .. } => AbsenceKind::Holiday,
            Self::Illness => AbsenceKind::Illness,
        }
    }
}

/// Which kind of absence, with nothing it carries.
///
/// **Named after [`Absence`]'s own variants**, which #178 settled, rather than
/// after the distinction between them. An earlier proposal called the pair
/// `Intentional` and `Illness`: that reads one of the two as the absence of the
/// other's property, and a holiday is a holiday whether or not it was planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AbsenceKind {
    Holiday,
    Illness,
}

impl AbsenceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Holiday => "holiday",
            Self::Illness => "illness",
        }
    }
}

impl std::fmt::Display for AbsenceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Illness's slots, so [`Alteration::slots`] can lend a map whatever the kind.
static NO_SLOTS: BTreeMap<TrainingSlot, Allocation> = BTreeMap::new();

/// A run of days that departs from the ordinary pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alteration {
    start: Date,
    days: NonZeroU8,
    absence: Absence,
}

impl Alteration {
    pub const fn new(start: Date, days: NonZeroU8, absence: Absence) -> Self {
        Self {
            start,
            days,
            absence,
        }
    }

    pub const fn start(&self) -> Date {
        self.start
    }

    pub const fn days(&self) -> NonZeroU8 {
        self.days
    }

    pub const fn absence(&self) -> &Absence {
        &self.absence
    }

    /// The zone a holiday is spent in, when it is not the ordinary one.
    pub const fn zone(&self) -> Option<&OperatorZone> {
        match &self.absence {
            Absence::Holiday { zone, .. } => zone.as_ref(),
            Absence::Illness => None,
        }
    }

    /// The slots while it lasts, which replace the ordinary week's. `None`
    /// when the ordinary week stands.
    pub fn slots(&self) -> Option<&BTreeMap<TrainingSlot, Allocation>> {
        match &self.absence {
            Absence::Holiday { slots, .. } => slots.as_ref(),
            Absence::Illness => Some(&NO_SLOTS),
        }
    }

    /// Why, which only a holiday has. An unexplained trip is unreadable six
    /// months later — § II.2's obligation on an edit overlay, which this is the
    /// authored-data analogue of. Illness needs no explanation beyond itself.
    pub const fn reason(&self) -> Option<&str> {
        match &self.absence {
            Absence::Holiday { reason, .. } => Some(reason.as_str()),
            Absence::Illness => None,
        }
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
    pub slots: BTreeMap<TrainingSlot, Allocation>,
}

impl Availability {
    /// Is this date one the operator could train on at all, by any discipline?
    ///
    /// Rarely the question worth asking: a day open to cycling is not a day the
    /// gym can use. [`Self::for_discipline`] is what a programme wants.
    pub fn open(&self, date: Date) -> bool {
        self.slots.keys().any(|slot| slot.weekday == date.weekday())
    }

    /// This date's slots belonging to one discipline, each with the role it is
    /// held for.
    pub fn for_discipline(
        &self,
        discipline: Discipline,
        date: Date,
    ) -> impl Iterator<Item = (TrainingSlot, SessionRole)> + '_ {
        self.slots
            .iter()
            .filter(move |(slot, allocated)| {
                allocated.discipline == discipline && slot.weekday == date.weekday()
            })
            .map(|(slot, allocated)| (*slot, allocated.role))
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
                            allocated.discipline == discipline && slot.weekday == cursor.weekday()
                        })
                        .map(|(slot, _)| *slot)
                        .collect()
                })
                .unwrap_or_default();

            if !ordinarily.is_empty() {
                // **The discipline is what has to survive, not the role.** A
                // holiday may hand the gym a Saturday morning as its
                // higher-intensity session where the ordinary week held a
                // Friday evening; the Friday is still a day the programme
                // cannot run, and a role that moved with it is not a loss.
                let kept = self.on(cursor).is_some_and(|availability| {
                    ordinarily.iter().any(|slot| {
                        availability
                            .slots
                            .get(slot)
                            .is_some_and(|allocated| allocated.discipline == discipline)
                    })
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
    pub fn slots_on(&self, date: Date, discipline: Discipline) -> Vec<(TrainingSlot, SessionRole)> {
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
            .filter(|(_, held)| held.discipline == discipline)
            .map(|(slot, _)| slot.weekday)
            .collect();
        days.dedup_by_key(|weekday| weekday.to_monday_zero_offset());
        Some(days)
    }

    /// One discipline's ordinary week as of a date, ready for a calendar.
    ///
    /// **Ordinary, so alterations are not applied**, for the reason
    /// [`Self::ordinarily`] gives: an alteration is a run of days that departs
    /// from the week's shape, and letting a holiday covering the start date
    /// decide the shape of every week after it is how the autumn block — which
    /// starts inside one — would have been built on a fortnight in Rome.
    ///
    /// `None` where the diary says nothing about the date, and where it says
    /// the discipline has no day at all. The two are different facts and
    /// [`Self::ordinarily`] tells them apart; neither builds a calendar, which
    /// is all this is for.
    #[must_use]
    pub fn training_week(&self, date: Date, discipline: Discipline) -> Option<TrainingWeek> {
        let pattern = self.pattern_on(date)?;
        // `TrainingSlot` orders Monday-first and then by part of day, so equal
        // weekdays are adjacent and the earlier part of the day comes first.
        let days: Vec<(Weekday, SessionRole)> = pattern
            .slots()
            .iter()
            .filter(|(_, held)| held.discipline == discipline)
            .map(|(slot, held)| (slot.weekday, held.role))
            .collect();
        TrainingWeek::new(days).ok()
    }

    fn pattern_on(&self, date: Date) -> Option<&TrainingPattern> {
        self.patterns.iter().rfind(|pattern| pattern.from() <= date)
    }

    /// Every slot the ordinary week gives one date, before any alteration.
    ///
    /// **What a microcycle's sessions are**, and the reason this exists beside
    /// [`Self::slots_of`] (#185). An absence removes a day's slots, so the
    /// altered answer makes a session lost to a holiday *vanish* rather than
    /// report it as lost — and saying "skipped (holiday)" is the whole of what
    /// a session's state is for. The absence is then read as the reason the
    /// session did not happen, which is [`Self::taken`].
    pub fn ordinary_slots_of(&self, date: Date) -> Vec<ScheduledSlot> {
        self.pattern_on(date)
            .map(|pattern| {
                pattern
                    .slots()
                    .iter()
                    .filter(|(slot, _)| slot.weekday == date.weekday())
                    .map(|(slot, allocated)| ScheduledSlot {
                        date,
                        slot: *slot,
                        discipline: allocated.discipline,
                        role: allocated.role,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The first slot the ordinary week gives a day after this one.
    ///
    /// [`Self::first_after`]'s counterpart, and what closes a session's window:
    /// a session may be performed until the next one starts, and an absence
    /// must not be able to *extend* that by deleting the slot that ends it.
    /// Bounded exactly as [`Self::first_after`] is.
    pub fn first_ordinary_after(&self, date: Date) -> Option<ScheduledSlot> {
        let described = self
            .patterns
            .iter()
            .map(TrainingPattern::from)
            .chain(self.alterations.iter().map(Alteration::last))
            .max()?
            .max(date);
        let horizon = described.checked_add(jiff::Span::new().days(7)).ok()?;

        let mut cursor = date.tomorrow().ok()?;
        while cursor <= horizon {
            if let Some(first) = self.ordinary_slots_of(cursor).into_iter().next() {
                return Some(first);
            }
            cursor = cursor.tomorrow().ok()?;
        }
        None
    }

    /// What has taken a part of a day, where an absence has taken it.
    ///
    /// `None` is time the operator has. **Any part counts, not only a training
    /// slot**: a session may be performed at any point in its window, and the
    /// question here is whether there was room in the day at all.
    ///
    /// The three cases an absence has, which #188 restored and #189 recorded:
    /// slots it does not state take nothing, an empty set takes the whole day,
    /// and a stated set takes every part it does not keep. The last alteration
    /// to state slots wins, exactly as [`Self::on`] resolves an overlap.
    pub fn taken(&self, at: DayPart) -> Option<AbsenceKind> {
        let mut stated: Option<(&BTreeMap<TrainingSlot, Allocation>, AbsenceKind)> = None;
        for alteration in self
            .alterations
            .iter()
            .filter(|alteration| alteration.covers(at.date))
        {
            if let Some(slots) = alteration.slots() {
                stated = Some((slots, alteration.absence().kind()));
            }
        }

        let (slots, kind) = stated?;
        let kept = slots
            .keys()
            .any(|slot| slot.weekday == at.date.weekday() && slot.part == at.part);
        (!kept).then_some(kind)
    }

    /// Every slot on one date, in the order the day runs, after every
    /// alteration covering it.
    pub fn slots_of(&self, date: Date) -> Vec<ScheduledSlot> {
        self.on(date)
            .map(|availability| {
                availability
                    .slots
                    .iter()
                    .filter(|(slot, _)| slot.weekday == date.weekday())
                    .map(|(slot, allocated)| ScheduledSlot {
                        date,
                        slot: *slot,
                        discipline: allocated.discipline,
                        role: allocated.role,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The first slot on a day after this one.
    ///
    /// **Bounded by what the diary says.** Past the last pattern and the last
    /// alteration the week only repeats, so a week beyond both that holds no
    /// slot means none ever comes — and `None` says so, rather than searching
    /// for ever.
    pub fn first_after(&self, date: Date) -> Option<ScheduledSlot> {
        let described = self
            .patterns
            .iter()
            .map(TrainingPattern::from)
            .chain(self.alterations.iter().map(Alteration::last))
            .max()?
            .max(date);
        let horizon = described.checked_add(jiff::Span::new().days(7)).ok()?;

        let mut cursor = date.tomorrow().ok()?;
        while cursor <= horizon {
            if let Some(first) = self.slots_of(cursor).into_iter().next() {
                return Some(first);
            }
            cursor = cursor.tomorrow().ok()?;
        }
        None
    }
}

/// One slot on one date, and whose it is.
///
/// Ordered by date, then by the slot within it, so a list of them reads as the
/// days ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScheduledSlot {
    pub date: Date,
    pub slot: TrainingSlot,
    pub discipline: Discipline,
    /// What the session filling it is, against the microcycle's others. Flat
    /// beside the discipline rather than an [`Allocation`], because this is a
    /// slot *on a date* and the pair is already settled by the time it is one.
    pub role: SessionRole,
}

impl std::fmt::Display for ScheduledSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}, {} {}", self.discipline, self.date, self.slot.part)
    }
}

/// One session the record holds, and what the record says it was.
///
/// Named for the record rather than for the act, because a session performed
/// and a session the record *holds* are not the same set: a workout the source
/// has not served yet happened and is not here.
///
/// **What can be said differs by discipline, and that is the record talking**
/// rather than a hole in the model. A performed ride names the class it was
/// ridden to, and a class is one of the planned rides or none of them — so
/// cycling can say which session of the microcycle a ride *was*. A gym workout
/// performed against no prescription names the day it was done and nothing
/// else.
///
/// `role` of `None` is therefore not "unknown, assume the usual": it is the
/// record declining to say, and [`accounted`] falls back to asking whose turn
/// it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RecordedSession {
    /// The day it was performed on.
    pub date: Date,
    pub discipline: Discipline,
    /// What it was, where the record names it.
    pub role: Option<SessionRole>,
}

impl RecordedSession {
    /// A session the record can say nothing about beyond its day.
    #[must_use]
    pub const fn unnamed(date: Date, discipline: Discipline) -> Self {
        Self {
            date,
            discipline,
            role: None,
        }
    }

    /// A session the record names.
    #[must_use]
    pub const fn named(date: Date, discipline: Discipline, role: SessionRole) -> Self {
        Self {
            date,
            discipline,
            role: Some(role),
        }
    }
}

/// Which planned sessions the record accounts for, one answer per slot.
///
/// **A role is matched, never conferred** (#177). The operator, 2026-09-20, on
/// the endurance ride he performed in what the week holds as the shorter,
/// harder slot:
///
/// > I didn't complete a FTP test last week so an FTP is still owed. The fact
/// > that the endurance ride was on a Wednesday [...] doesn't change what it
/// > was!
///
/// So a named session answers for the slot of its discipline that holds its
/// role, **whatever day it landed on** — before that slot as readily as after.
/// This is § 11 at the scheduling layer: the slot never re-describes what was
/// performed in it. Until this was read, the Wednesday ride marked the FTP slot
/// done and the FTP test went unridden into the first Build mesocycle.
///
/// **Where the record names nothing, the slot whose turn it was answers**: the
/// latest slot of its discipline on or before the day it was performed. The
/// operator, 2026-09-19: *"if the previous prescribed session was performed
/// since we last checked, that's fine, the slot doesn't have to match"* — a
/// test missed through illness on the Friday and performed on the Saturday is
/// that test, because the Friday is the last gym slot the Saturday is after.
///
/// **Read from the session rather than from the slot**, which is the
/// correction #185 forced. Walking the slots instead and giving each the
/// earliest session that could answer for it is the same thing while there is
/// one slot in question, and wrong across a whole microcycle: a gym session
/// done on the Sunday would answer for the *Monday* six days earlier and leave
/// the Friday looking missed.
///
/// **A slot takes one session**, and the named pass runs first so that it
/// cannot be robbed by an unnamed one. Two performed on one slot's watch fill
/// it once; there is no second slot for the second to reach back to.
///
/// `slots` is in the order the week runs.
#[must_use]
pub fn accounted(slots: &[ScheduledSlot], performed: &[RecordedSession]) -> Vec<bool> {
    let mut answered = vec![false; slots.len()];

    let claim = |answered: &mut Vec<bool>, at: Option<usize>| {
        if let Some(answer) = at.and_then(|at| answered.get_mut(at)) {
            *answer = true;
        }
    };

    for session in performed {
        let Some(role) = session.role else {
            continue;
        };
        let held = slots
            .iter()
            .enumerate()
            .find(|(at, slot)| {
                slot.discipline == session.discipline
                    && slot.role == role
                    && !answered.get(*at).copied().unwrap_or(false)
            })
            .map(|(at, _)| at);
        claim(&mut answered, held);
    }

    for session in performed {
        if session.role.is_some() {
            continue;
        }
        // **Unchanged from #185, including its ceiling.** The latest slot on
        // or before the day, claimed or not: a second unnamed session does not
        // reach further back for a slot of its own, because reaching back is
        // exactly what that rule rejected.
        let whose_turn = slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.discipline == session.discipline && slot.date <= session.date)
            .map(|(at, _)| at)
            .next_back();
        claim(&mut answered, whose_turn);
    }

    answered
}
