//! The operator's free time, as programming reads it (#673).
//!
//! **Programming, not the schedule.** The operator, 2026-10-31: the schedule
//! *"should just be about stating available slots and whether they are
//! shorter or longer slots and altering that for some run of days. Everything
//! else is a programming concern."* So which discipline a slot is given to,
//! which session it takes, and every question asked of a week in those terms
//! are here, read off the [`Diary`] rather than held by it.

use std::collections::BTreeMap;

use jiff::civil::{Date, Weekday};

use super::{
    SessionRole,
    arrange::{self, Limits},
};
use crate::schedule::{Alteration, Diary, Illness, SlotLength, TrainingPattern, TrainingSlot};

/// What a training slot is given to.
///
/// **The activity, never the vendor.** Cycling is cycling whether the bike is a
/// Peloton, a turbo trainer or a road; naming the member after the app that
/// happens to record it would make the vocabulary a shape of a source, which is
/// the one thing § II.3 rules out. The same reason the exercise vocabulary is
/// ours rather than Hevy's.
///
/// Closed. A free slot the week has no session for — a seventh day, or a
/// second slot on a day that takes one session — is given to neither, and
/// [`arrange`](super::arrange()) leaves it out.
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
/// **Worked out, not stated** (#673). It was the operator's input until
/// 2026-10-31, asked for every slot of every pattern and every holiday; a slot
/// now says only whether it is longer or shorter, and [`arrange`](super::arrange()) does the rest.
///
/// **A role of `None` is a gym session whose split is not built**: three a
/// week alternate their primaries (#676), and which session each is cannot be
/// read off a comparison between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Allocation {
    pub discipline: Discipline,
    pub role: Option<SessionRole>,
}

impl Allocation {
    pub const fn new(discipline: Discipline, role: SessionRole) -> Self {
        Self {
            discipline,
            role: Some(role),
        }
    }
}

impl std::fmt::Display for Allocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.role {
            Some(role) => write!(f, "{} ({role})", self.discipline),
            None => write!(f, "{}", self.discipline),
        }
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
#[error("a training week with no day in it issues nothing")]
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

/// A diary's free time arranged into sessions, week by week ([`arrange`](super::arrange())).
///
/// **A trait over the diary rather than methods of it**, so the schedule names
/// no discipline and no role: what a week of free time becomes is read here,
/// within the [`Limits`] the operator stated.
pub trait Arrangement {
    fn arranged(&self, containing: Date, ordinary: bool) -> Vec<ScheduledSlot>;
    fn lost_disciplines(&self, containing: Date) -> Vec<Discipline>;
    fn planned_slots_of(&self, date: Date) -> Vec<ScheduledSlot>;
    fn unavailable(&self, from: Date, until: Date, discipline: Discipline) -> Vec<Date>;
    fn slots_on(
        &self,
        date: Date,
        discipline: Discipline,
    ) -> Vec<(TrainingSlot, Option<SessionRole>)>;
    fn ordinarily(&self, date: Date, discipline: Discipline) -> Option<Vec<Weekday>>;
    fn training_week(&self, date: Date, discipline: Discipline) -> Option<TrainingWeek>;
    fn ordinary_slots_of(&self, date: Date) -> Vec<ScheduledSlot>;
    fn first_ordinary_after(&self, date: Date) -> Option<ScheduledSlot>;
    fn slots_of(&self, date: Date) -> Vec<ScheduledSlot>;
    fn first_after(&self, date: Date) -> Option<ScheduledSlot>;
}

impl Arrangement for Diary {
    /// The calendar week containing a date, arranged ([`arrange`](super::arrange())): every slot
    /// it uses, by date and then by slot.
    ///
    /// **A holiday rearranges what its week has left** (operator, 2026-10-31:
    /// *"we always try and re-arrange to make a week work, until we
    /// can't"*). Away Tuesday to Friday, Monday evening and Sunday morning are
    /// the harder gym session and the harder ride, and the mesocycle moves on.
    /// `ordinary` asks for the week as the pattern alone would have it.
    fn arranged(&self, containing: Date, ordinary: bool) -> Vec<ScheduledSlot> {
        let back = i64::from(containing.weekday().to_monday_zero_offset());
        let monday = containing
            .checked_sub(jiff::Span::new().days(back))
            .unwrap_or(containing);
        let dates: Vec<Date> = (0..7)
            .filter_map(|offset| monday.checked_add(jiff::Span::new().days(offset)).ok())
            .collect();
        let free: BTreeMap<TrainingSlot, SlotLength> = dates
            .iter()
            .flat_map(|date| self.free_on(*date, ordinary))
            .collect();
        let allocated = arrange::allocations(&free, Limits::STATED);

        dates
            .iter()
            .flat_map(|date| {
                allocated
                    .iter()
                    .filter(move |(slot, _)| slot.weekday == date.weekday())
                    .map(move |(slot, allocated)| ScheduledSlot {
                        date: *date,
                        slot: *slot,
                        discipline: allocated.discipline,
                        role: allocated.role,
                    })
            })
            .collect()
    }

    /// The disciplines the ordinary week containing a date holds a session
    /// of and the arranged week does not.
    ///
    /// **What makes a microcycle run again when it cannot be rearranged.**
    /// Away all but Monday, the week holds only the essential gym session;
    /// cycling's was lost, and the microcycle runs again — which nothing in
    /// the week's own sessions could say, since there is no ride in it to
    /// read as skipped.
    fn lost_disciplines(&self, containing: Date) -> Vec<Discipline> {
        let planned = self.arranged(containing, false);
        let mut lost: Vec<Discipline> = self
            .arranged(containing, true)
            .into_iter()
            .map(|slot| slot.discipline)
            .filter(|discipline| !planned.iter().any(|slot| slot.discipline == *discipline))
            .collect();
        lost.sort_unstable();
        lost.dedup();
        lost
    }

    /// Every slot one date takes in its week as planned: the pattern as any
    /// family holiday rearranges it, and before any illness.
    ///
    /// **What a microcycle's sessions are.** An illness takes a session the
    /// week already held, and saying "skipped (illness)" is what a session's
    /// state is for, so it is not applied here; [`Self::taken`] reads it.
    fn planned_slots_of(&self, date: Date) -> Vec<ScheduledSlot> {
        self.arranged(date, false)
            .into_iter()
            .filter(|slot| slot.date == date)
            .collect()
    }

    /// Every date in a range on which a discipline ordinarily trains and no
    /// longer can.
    ///
    /// **A week that keeps a session of the discipline has lost nothing**
    /// (#673). A family holiday rearranges what its week has left, so a gym
    /// session that moved from the Friday to the Saturday is still the week's,
    /// and a block counting the Friday as lost would skip a week the gym
    /// trains in. Only a week the rearrangement leaves the discipline nothing
    /// in loses its ordinary days — every one of them.
    ///
    /// **An illness takes its own days** whatever the week holds: it is not
    /// known in advance, so it rearranges nothing, and the session it took is
    /// lost where it fell.
    fn unavailable(&self, from: Date, until: Date, discipline: Discipline) -> Vec<Date> {
        let mut lost = Vec::new();
        let mut cursor = from;

        while cursor <= until {
            let ordinarily = self
                .ordinary_slots_of(cursor)
                .iter()
                .any(|slot| slot.discipline == discipline);

            if ordinarily {
                let kept = self
                    .arranged(cursor, false)
                    .iter()
                    .any(|slot| slot.discipline == discipline);
                if !kept || self.ill_on(cursor) {
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
    fn slots_on(
        &self,
        date: Date,
        discipline: Discipline,
    ) -> Vec<(TrainingSlot, Option<SessionRole>)> {
        self.slots_of(date)
            .into_iter()
            .filter(|slot| slot.discipline == discipline)
            .map(|slot| (slot.slot, slot.role))
            .collect()
    }

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
    fn ordinarily(&self, date: Date, discipline: Discipline) -> Option<Vec<Weekday>> {
        let pattern = self.pattern_on(date)?;
        // `TrainingSlot` orders Monday-first and then by part of day, so the
        // map is already in the order this returns and equal weekdays are
        // adjacent.
        let mut days: Vec<Weekday> = allocated(pattern)
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
    ///
    /// `None` too where a session of the week has no role, which is a gym
    /// week of three sessions until their split is built (#676): a calendar
    /// with a session it cannot role would gate on nothing.
    fn training_week(&self, date: Date, discipline: Discipline) -> Option<TrainingWeek> {
        let pattern = self.pattern_on(date)?;
        // `TrainingSlot` orders Monday-first and then by part of day, so equal
        // weekdays are adjacent and the earlier part of the day comes first.
        let days: Option<Vec<(Weekday, SessionRole)>> = allocated(pattern)
            .iter()
            .filter(|(_, held)| held.discipline == discipline)
            .map(|(slot, held)| held.role.map(|role| (slot.weekday, role)))
            .collect();
        TrainingWeek::new(days?).ok()
    }

    /// Every slot the ordinary week gives one date, before any alteration.
    ///
    /// What the week would have been, which is what an illness is read
    /// against ([`crate::planner::eased_by_illness`]) and what a lost day is
    /// a loss from ([`Self::unavailable`]). A microcycle's sessions are
    /// [`Self::planned_slots_of`].
    fn ordinary_slots_of(&self, date: Date) -> Vec<ScheduledSlot> {
        self.arranged(date, true)
            .into_iter()
            .filter(|slot| slot.date == date)
            .collect()
    }

    /// The first slot the ordinary week gives a day after this one.
    ///
    /// [`Self::first_after`]'s counterpart, and one bound on a session's window:
    /// no session may be performed after the next one starts, and an absence
    /// must not be able to *extend* that by deleting the slot that ends it.
    /// Bounded exactly as [`Self::first_after`] is.
    fn first_ordinary_after(&self, date: Date) -> Option<ScheduledSlot> {
        let described = self
            .patterns()
            .iter()
            .map(TrainingPattern::from)
            .chain(self.alterations().iter().map(Alteration::last))
            .chain(self.illnesses().iter().filter_map(Illness::ended_on))
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

    /// Every slot on one date, in the order the day runs, after every
    /// alteration covering it.
    fn slots_of(&self, date: Date) -> Vec<ScheduledSlot> {
        // What the day has free after every alteration, illness included,
        // arranged as its week is.
        let Some(free) = self.on(date) else {
            return Vec::new();
        };
        self.arranged(date, false)
            .into_iter()
            .filter(|slot| slot.date == date && free.slots.contains_key(&slot.slot))
            .collect()
    }

    /// The first slot on a day after this one.
    ///
    /// **Bounded by what the diary says.** Past the last pattern, the last
    /// alteration and the last illness's end the week only repeats, so a week beyond both that holds no
    /// slot means none ever comes — and `None` says so, rather than searching
    /// for ever.
    fn first_after(&self, date: Date) -> Option<ScheduledSlot> {
        let described = self
            .patterns()
            .iter()
            .map(TrainingPattern::from)
            .chain(self.alterations().iter().map(Alteration::last))
            .chain(self.illnesses().iter().filter_map(Illness::ended_on))
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

/// A pattern's own week, arranged.
fn allocated(pattern: &TrainingPattern) -> BTreeMap<TrainingSlot, Allocation> {
    arrange::allocations(pattern.slots(), Limits::STATED)
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
    /// `None` where the gym's split for the week is not built (#676).
    pub role: Option<SessionRole>,
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
    pub const fn unnamed(date: Date, discipline: Discipline) -> Self {
        Self {
            date,
            discipline,
            role: None,
        }
    }

    /// A session the record names.
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
                    && slot.role == Some(role)
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
