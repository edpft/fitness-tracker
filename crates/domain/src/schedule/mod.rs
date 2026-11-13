//! When there is room to train, and where.
//!
//! **Operator-level, not programme-level.** This is a fact about a life — when
//! work, family and social commitments leave room to train, and which zone that
//! happens in. Every discipline reads it; none owns it.
//!
//! **Time, and nothing about what fills it** (operator, 2026-10-31). The
//! schedule states the slots the operator is free in, whether each is longer
//! or shorter, and what departs from that for a run of days. Which discipline
//! and which session each slot takes is programming, and lives in
//! [`crate::planner`]: until #673 it was stated here, slot by slot, which
//! made a holiday that moved one slot a week of roles to type again.
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
//! ## Three shapes, and they are not versions of each other
//!
//! A [`TrainingPattern`] is the ordinary run of a week, in force from a date
//! until something supersedes it. An [`Alteration`] is a run of days that
//! departs from it. An [`Illness`] runs from a date until the operator says he
//! is better.
//!
//! **An alteration is only a change to when the operator is free** (#674): a
//! family holiday, a work trip, a session moved to another evening. They
//! differ only in their reason, so there is no kind to choose between. A late
//! finish is not one: a session can be performed any time in its window, and
//! the scheduling side need not know. A lasting change is a new
//! [`TrainingPattern`].
//!
//! **Illness is not an alteration.** The operator, 2026-10-31: it *"doesn't
//! just affect my ability to train but also implies a regression of fitness
//! which needs to be accounted for"*, and how long it lasts is not known when
//! it starts. So it has no length, only a start and, once he is better, an end.
//!
//! They are held apart rather than nested, because an alteration is a fact
//! about dates and not about which pattern happened to be in force when it was
//! recorded. Nesting them would lose every alteration already recorded the next
//! time the ordinary pattern changed.
//!
//! ## What an alteration can say
//!
//! - **the slots unchanged** — away, perhaps in another zone, training at the
//!   usual times.
//! - **no slots** — unable to train at all.
//! - **different slots** — able to train at times the ordinary pattern does
//!   not offer. A Friday evening becomes a Saturday morning. It is also how
//!   half a day is said: keep the morning, lose the rest.
//!
//! `None` slots are "the ordinary week stands" and `Some` of an empty set is
//! "none at all". Those are different facts, and collapsing them would make
//! training away as usual cancel every session of the trip.

mod holiday;
mod length;

use std::{collections::BTreeMap, num::NonZeroU8};

use jiff::civil::{Date, Weekday};

use crate::normalised::OperatorZone;

pub use holiday::{Holidays, PublicHoliday, SchoolHoliday, SchoolHolidayKind};
pub use length::{SlotLength, UnknownSlotLength};

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
    /// **A part in progress counts as still usable until it ends**: at eight in
    /// the evening there is still time for that evening's session. So this
    /// answers with the part the clock is in rather than the next one.
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
    slots: BTreeMap<TrainingSlot, SlotLength>,
}

impl TrainingPattern {
    /// A schedule may have no slots — a period with no room to train at all is a
    /// real thing to record, and refusing it would mean pretending otherwise.
    pub const fn new(
        from: Date,
        zone: OperatorZone,
        slots: BTreeMap<TrainingSlot, SlotLength>,
    ) -> Self {
        Self { from, zone, slots }
    }

    pub const fn from(&self) -> Date {
        self.from
    }

    pub const fn zone(&self) -> &OperatorZone {
        &self.zone
    }

    pub const fn slots(&self) -> &BTreeMap<TrainingSlot, SlotLength> {
        &self.slots
    }
}

/// What took a part of a day, with only what a session's state needs.
///
/// **An alteration and a skip carry their reason** (#391, #674). The operator,
/// 2026-10-10: a skip's reason is free text *"at the level of illness and
/// family holiday"*. Since #674 an alteration has no kind either — a holiday,
/// a work trip and a moved session differ only in why — so what it took reads
/// as its reason. Illness is its own record and needs none.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AbsenceKind {
    Alteration { reason: String },
    Illness,
    Skip { reason: String },
}

impl std::fmt::Display for AbsenceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Alteration { reason } | Self::Skip { reason } => reason,
            Self::Illness => "illness",
        })
    }
}

/// Time the operator will not train in, from when he says so to the end of the
/// window then open (#391).
///
/// **Time, not a session.** The operator, 2026-10-10: a skip *"marks that time
/// as unavailable for training, not that it skips a specific session"*. Run on
/// a Monday afternoon with the gym slotted that evening and the next session a
/// Wednesday evening ride, it takes every part from the Monday afternoon to the
/// Wednesday afternoon. Taking only the evening could leave time to perform the
/// session, and then it was not skipped. What happens to the session follows
/// from the time being gone, which is what lets an essential one move to a
/// later slot of its discipline (#392).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skip {
    from: DayPart,
    until: DayPart,
    reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidSkip {
    #[error("a skip says why")]
    NoReason,
    #[error("a skip ends after it begins")]
    Empty,
}

impl Skip {
    /// Every part of a day from `from` up to, and not including, `until`: the
    /// moment the next session's slot begins.
    ///
    /// # Errors
    ///
    /// [`InvalidSkip`] if the reason is blank, or `until` is not after `from`.
    pub fn new(from: DayPart, until: DayPart, reason: &str) -> Result<Self, InvalidSkip> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(InvalidSkip::NoReason);
        }
        if until <= from {
            return Err(InvalidSkip::Empty);
        }
        Ok(Self {
            from,
            until,
            reason: reason.to_owned(),
        })
    }

    pub const fn from(&self) -> DayPart {
        self.from
    }

    /// Where it stops: the start of the slot that closed the window.
    pub const fn until(&self) -> DayPart {
        self.until
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }

    pub fn covers(&self, at: DayPart) -> bool {
        at >= self.from && at < self.until
    }

    /// What it takes a part of a day as.
    pub fn kind(&self) -> AbsenceKind {
        AbsenceKind::Skip {
            reason: self.reason.clone(),
        }
    }
}

/// The weekdays a run of days actually covers, in the order it meets them.
///
/// **A run meets only some weekdays.** A trip from Friday to Monday has no
/// Wednesday in it, and a slot stated for one would be a fact about a day the
/// alteration never touches — silently meaningless, and reasonable to expect
/// otherwise. A run of a week or more meets every weekday, once each.
///
/// Run order rather than Monday-first, because that is the order the operator
/// lives it: the Friday they leave, then the weekend, then the Monday back.
pub fn covered_weekdays(start: Date, days: NonZeroU8) -> Vec<Weekday> {
    let mut covered: Vec<Weekday> = Vec::new();
    let mut cursor = start;

    for _ in 0..days.get() {
        if !covered.contains(&cursor.weekday()) {
            covered.push(cursor.weekday());
        }
        if covered.len() == 7 {
            break;
        }
        let Ok(next) = cursor.tomorrow() else { break };
        cursor = next;
    }

    covered
}

/// A run of days that departs from the ordinary pattern: when the operator is
/// free while it lasts, where, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alteration {
    start: Date,
    days: NonZeroU8,
    zone: Option<OperatorZone>,
    slots: Option<BTreeMap<TrainingSlot, SlotLength>>,
    reason: String,
}

impl Alteration {
    /// A `None` zone is "the zone is unchanged", which is not the same as any
    /// zone; `None` slots are "the ordinary week stands", and an empty set is
    /// "no room to train at all".
    pub const fn new(
        start: Date,
        days: NonZeroU8,
        zone: Option<OperatorZone>,
        slots: Option<BTreeMap<TrainingSlot, SlotLength>>,
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

    /// The zone it is spent in, when it is not the ordinary one.
    pub const fn zone(&self) -> Option<&OperatorZone> {
        self.zone.as_ref()
    }

    /// The slots while it lasts, which replace the ordinary week's. `None`
    /// when the ordinary week stands.
    pub const fn slots(&self) -> Option<&BTreeMap<TrainingSlot, SlotLength>> {
        self.slots.as_ref()
    }

    /// Why. An unexplained trip is unreadable six months later — § II.2's
    /// obligation on an edit overlay, which this is the authored-data analogue
    /// of — and it is the only thing that tells a holiday from a work trip.
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

    /// What it takes a part of a day as.
    pub fn kind(&self) -> AbsenceKind {
        AbsenceKind::Alteration {
            reason: self.reason.clone(),
        }
    }
}

/// Too ill to train, from a date until the operator says he is better (#674).
///
/// **Open until ended, because its length is not known when it starts.** While
/// it is open nothing is prescribed. Ending it is a declaration rather than
/// the first session performed afterwards: a session done while still ill is
/// not recovery, and the first session after it is the one a return from
/// illness shapes.
///
/// **Bad enough to prevent training is what makes it illness**, so it takes
/// every part of every day it covers, and where the operator is does not
/// matter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Illness {
    from: Date,
    ended: Option<Date>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidIllness {
    #[error("an illness ends on or after the day it began, {from}")]
    EndsBeforeItBegins { from: Date },
}

impl Illness {
    /// Ill from this date, and still ill.
    pub const fn new(from: Date) -> Self {
        Self { from, ended: None }
    }

    /// The same illness, ended: `on` is the first day the operator is better.
    ///
    /// **Ending it on the day it began covers nothing**, which is what saying
    /// "ill today" and then "better today" means: today is available again.
    ///
    /// # Errors
    ///
    /// [`InvalidIllness`] if `on` is before the day it began.
    pub fn ended(self, on: Date) -> Result<Self, InvalidIllness> {
        if on < self.from {
            return Err(InvalidIllness::EndsBeforeItBegins { from: self.from });
        }
        Ok(Self {
            from: self.from,
            ended: Some(on),
        })
    }

    pub const fn from(&self) -> Date {
        self.from
    }

    /// The first day better, once he has said so.
    pub const fn ended_on(&self) -> Option<Date> {
        self.ended
    }

    pub const fn is_open(&self) -> bool {
        self.ended.is_none()
    }

    pub fn covers(&self, date: Date) -> bool {
        date >= self.from && self.ended.is_none_or(|ended| date < ended)
    }
}

/// What a given day actually looks like: where the operator is, and the
/// slots free in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Availability {
    pub zone: OperatorZone,
    /// The day's own free slots, after every alteration covering it.
    pub slots: BTreeMap<TrainingSlot, SlotLength>,
}

impl Availability {
    /// Is this date one the operator could train on at all?
    pub fn open(&self, date: Date) -> bool {
        self.slots.keys().any(|slot| slot.weekday == date.weekday())
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
    illnesses: Vec<Illness>,
    skips: Vec<Skip>,
}

impl Diary {
    /// Schedules are sorted on the way in, so `on` can take the last that
    /// applies without every caller having to have sorted them.
    pub fn new(mut patterns: Vec<TrainingPattern>, alterations: Vec<Alteration>) -> Self {
        patterns.sort_by_key(TrainingPattern::from);
        Self {
            patterns,
            alterations,
            illnesses: Vec::new(),
            skips: Vec::new(),
        }
    }

    /// The same diary, holding these illnesses as well.
    #[must_use]
    pub fn with_illnesses(mut self, illnesses: Vec<Illness>) -> Self {
        self.illnesses = illnesses;
        self
    }

    /// The same diary, holding these skips as well.
    #[must_use]
    pub fn with_skips(mut self, skips: Vec<Skip>) -> Self {
        self.skips = skips;
        self
    }

    pub fn patterns(&self) -> &[TrainingPattern] {
        &self.patterns
    }

    pub fn alterations(&self) -> &[Alteration] {
        &self.alterations
    }

    pub fn illnesses(&self) -> &[Illness] {
        &self.illnesses
    }

    /// The illness he has not yet said he is better from, if there is one.
    pub fn open_illness(&self) -> Option<&Illness> {
        self.illnesses.iter().rfind(|illness| illness.is_open())
    }

    /// Was he ill on this date?
    pub fn ill_on(&self, date: Date) -> bool {
        self.illnesses.iter().any(|illness| illness.covers(date))
    }

    pub fn skips(&self) -> &[Skip] {
        &self.skips
    }

    /// The skip that took what was left of a window closing at `closes`.
    ///
    /// **Keyed on where the window closes**, because that is where every skip
    /// ends: the session whose window it is reads *skipped* however much of the
    /// window had already gone, and the session before it — whose window closed
    /// before this skip began — is untouched. The latest wins, so saying it
    /// again with another reason corrects it.
    ///
    /// **Running past the close still takes it.** A skip written while a window
    /// ran to the next slot ends there, after the window now closes (#463).
    pub fn skipped_until(&self, closes: DayPart) -> Option<&Skip> {
        self.skips
            .iter()
            .rfind(|skip| skip.from < closes && closes <= skip.until)
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

        let mut zone = pattern.zone().clone();
        for alteration in self
            .alterations
            .iter()
            .filter(|alteration| alteration.covers(date))
        {
            if let Some(stated) = alteration.zone() {
                zone.clone_from(stated);
            }
        }

        // **Illness takes the day whatever an alteration states**: it is not
        // known in advance, so nothing arranged around it can give it back.
        let slots = if self.ill_on(date) {
            BTreeMap::new()
        } else {
            self.free_on(date, false).into_iter().collect()
        };

        Some(Availability { zone, slots })
    }

    /// The free slots a date offers: the pattern in force, or — unless only
    /// the ordinary week is asked for — the slots an alteration covering it
    /// states instead. The last to state slots wins, as [`Self::on`]
    /// resolves an overlap.
    ///
    /// **Illness is never read here**, for the reason [`Self::on`] gives.
    pub fn free_on(&self, date: Date, ordinary: bool) -> Vec<(TrainingSlot, SlotLength)> {
        let Some(pattern) = self.pattern_on(date) else {
            return Vec::new();
        };
        let mut slots = pattern.slots();
        if !ordinary {
            for alteration in self
                .alterations
                .iter()
                .filter(|alteration| alteration.covers(date))
            {
                if let Some(stated) = alteration.slots() {
                    slots = stated;
                }
            }
        }
        slots
            .iter()
            .filter(|(slot, _)| slot.weekday == date.weekday())
            .map(|(slot, length)| (*slot, *length))
            .collect()
    }

    /// The pattern in force on a date, before any alteration.
    pub fn pattern_on(&self, date: Date) -> Option<&TrainingPattern> {
        self.patterns.iter().rfind(|pattern| pattern.from() <= date)
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
        // **Illness is read first**: it takes a toll an alteration or a skip
        // does not (#180), so where it covers a part, the part is the
        // illness's.
        if self.ill_on(at.date) {
            return Some(AbsenceKind::Illness);
        }

        let mut stated: Option<(&BTreeMap<TrainingSlot, SlotLength>, &Alteration)> = None;
        for alteration in self
            .alterations
            .iter()
            .filter(|alteration| alteration.covers(at.date))
        {
            if let Some(slots) = alteration.slots() {
                stated = Some((slots, alteration));
            }
        }

        if let Some((slots, alteration)) = stated {
            let kept = slots
                .keys()
                .any(|slot| slot.weekday == at.date.weekday() && slot.part == at.part);
            if !kept {
                return Some(alteration.kind());
            }
        }
        self.skips
            .iter()
            .rfind(|skip| skip.covers(at))
            .map(Skip::kind)
    }
}
