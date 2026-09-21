//! What fills each slot of a calendar week.
//!
//! **The join issue #63 exists for.** A programme answers with a count of
//! sessions each carrying a role and knows nothing about weekdays; the
//! operator's week gives each slot a discipline and a role. Putting the two
//! together is this module, and it is the only place that knows both.
//!
//! The operator, 2026-09-16: *"the gym and cycling programmes shouldn't know
//! anything about the days of the week they're allocated, they're responsible
//! for providing a number of sessions with a role, heavier/lighter,
//! longer/shorter, the allocator/planner maps them to actual calendar weeks
//! but, for now at least, it can just take the available slots and
//! discipline/role I've already decided as an input."*
//!
//! **A role places a session; a published order does not.** Peloton files the
//! FTP test second in its test microcycle, which by published order lands it on
//! the Sunday. It is the higher-intensity, shorter session, so it goes to the
//! Wednesday — the slot the operator holds for exactly that.
//!
//! **Nothing here decides anything.** It reports what answers for each slot and
//! why nothing does where nothing does; rescheduling a lost session is #177's,
//! and the span across a whole plan is #64's.

use std::{collections::BTreeMap, fmt};

use jiff::civil::Date;

use crate::{
    cycling::{CyclingMesocycle, PlannedRide, SessionPosition},
    plan::Plan,
    prescription::{Mesocycle, NotScheduled, WeekKind},
    schedule::{AbsenceKind, DayPart, Diary, Discipline, Relative, ScheduledSlot, SessionRole},
};

/// What answers for one slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filled<'a> {
    /// The gym's programme in force, and where the date sits in it.
    ///
    /// No session object, because a gym session is derived when it is
    /// prescribed — from the programme, the record and the parameters — and
    /// deriving it needs a store this module has no business holding.
    Gym {
        mesocycle: &'a Mesocycle,
        week: WeekKind,
    },
    /// The ride itself, which is authored in full and needs nothing read.
    Cycling {
        mesocycle: &'a CyclingMesocycle,
        /// Which microcycle of it, counting from one.
        microcycle: usize,
        /// Where the ride sits in its week, in the mesocycle's own numbering.
        /// It is not what placed it — the role did.
        position: SessionPosition,
        ride: &'a PlannedRide,
    },
}

/// Why nothing answers for a slot.
///
/// **Every one of these is a real state rather than a fault.** A plan that has
/// not started, a week the block is away for, a discipline the plan does not
/// programme: the operator's week still holds the slot, and saying what is
/// missing is more use than leaving it out.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unfilled {
    #[error("this plan programmes no {discipline}")]
    NoProgramme { discipline: Discipline },
    #[error("no {discipline} mesocycle of this plan covers {date}")]
    NoMesocycle { discipline: Discipline, date: Date },
    #[error(transparent)]
    NotScheduled(#[from] NotScheduled),
    #[error("this microcycle rides nothing that is {role}")]
    NoSessionForRole { role: SessionRole },
}

/// One slot of a calendar week, and what answers for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned<'a> {
    pub slot: ScheduledSlot,
    /// Which of its discipline's sessions in this week, counting from one.
    ///
    /// **The plan's numbering, which is date order within the week.** The
    /// operator's *"the first gym session"* is the Monday whatever role it
    /// holds and whatever the published programme calls it; a published
    /// ordinal names a session and never a position (#63).
    pub number: u8,
    pub session: Result<Filled<'a>, Unfilled>,
}

/// The Monday of the calendar week a date falls in.
///
/// **Monday-first, unlike a block's own weeks.** A block counts its weeks from
/// the weekday it started on, because its interruptions are the same weeks its
/// sessions are; a *calendar* week is the one the operator lives in, and his
/// starts on a Monday.
#[must_use]
pub fn commencing(date: Date) -> Date {
    let back = i64::from(date.weekday().to_monday_zero_offset());
    date.checked_sub(jiff::Span::new().days(back))
        .unwrap_or(date)
}

/// Every slot of the calendar week containing a date, and what fills it.
///
/// In the order the week runs: by date, then by the slot within the day. A week
/// the diary says nothing about is empty rather than absent — there are no
/// slots to fill, which is a different thing from a plan with nothing in it.
///
/// **The ordinary week, so an absence does not delete a session.** This read
/// [`Diary::slots_of`] until 2026-09-20, which applies every alteration — and
/// an absence takes a day's slots away, so a session lost to a holiday had no
/// slot and did not appear at all. The operator's week of 14 September came
/// back as two sessions rather than four, silently missing the gym test he was
/// too ill to do. A session the operator could not train is still a session of
/// the microcycle, and #185 exists to say *why* it did not happen; the
/// alteration is read for that rather than to decide what is here.
#[must_use]
pub fn week<'a>(plan: &'a Plan, diary: &Diary, containing: Date) -> Vec<Planned<'a>> {
    let monday = commencing(containing);
    let mut counted: BTreeMap<Discipline, u8> = BTreeMap::new();
    (0..7)
        .filter_map(|offset| monday.checked_add(jiff::Span::new().days(offset)).ok())
        .flat_map(|date| diary.ordinary_slots_of(date))
        .map(|slot| {
            let number = counted
                .entry(slot.discipline)
                .and_modify(|seen| *seen = seen.saturating_add(1))
                .or_insert(1);
            Planned {
                number: *number,
                session: fill(plan, slot),
                slot,
            }
        })
        .collect()
}

/// Where one session of the microcycle stands.
///
/// **A state machine, and the operator's own** (#185, 2026-09-19): *"What I'm
/// talking about here is essentially a state machine"*. The six are exhaustive
/// and none of them is a fault — a week that ran perfectly reports four of
/// these just as a week nobody trained does.
///
/// **Three reasons a session does not happen, and they are kept apart.** The
/// operator: *"intentional absences that are known about in advance (holidays,
/// gym closures, etc), unintentional absences that may or may not be known
/// about in advance (illness), and just not running the tool at all"*. Illness
/// is not a state of its own; it is the kind an absence carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Nothing has been issued and there is still time to issue it.
    ToBePrescribed,
    /// Issued and delivered somewhere it can be performed (§ 12.1), and — the
    /// operator's words — *"not performed yet, but there's still time"*.
    Prescribed,
    /// A session of its discipline accounts for it.
    Performed,
    /// An absence covered its slot before anything was prescribed.
    Skipped { absence: AbsenceKind },
    /// The window closed with nothing prescribed and no absence to explain it:
    /// the tool was not run, so *"nothing could be prescribed that should have
    /// been"*. The operator offered *missed*, *forgotten* and *not prescribed*
    /// as names, and chose the last.
    NotPrescribed,
    /// Prescribed, and no time left to perform it. The absence is named only
    /// where an absence is what closed the window — where the time simply ran
    /// out, nothing took it.
    NotPerformed { absence: Option<AbsenceKind> },
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ToBePrescribed => f.write_str("to be prescribed"),
            Self::Prescribed => f.write_str("prescribed"),
            Self::Performed => f.write_str("performed"),
            Self::Skipped { absence } => write!(f, "skipped ({absence})"),
            Self::NotPrescribed => f.write_str("not prescribed"),
            Self::NotPerformed { absence: None } => f.write_str("not performed"),
            Self::NotPerformed {
                absence: Some(absence),
            } => write!(f, "not performed ({absence})"),
        }
    }
}

/// What the store says about one planned session.
///
/// **Two facts, and neither is derivable from the other.** § 11 keeps
/// prescribed and performed separate and joinable, and this is that separation
/// arriving at the state machine: a session may be performed without this build
/// having recorded prescribing it, and one prescribed may never be performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recorded {
    /// Issued *and* delivered. A prescription that is drafted and nowhere else
    /// is not something the operator can train from (§ 12.1), so it does not
    /// count here.
    pub prescribed: bool,
    /// A session of its discipline accounts for it.
    pub performed: bool,
}

/// What is left of a session's window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remaining {
    /// At least one part of a day is still usable.
    Time,
    /// Every part that is left was taken by an absence.
    Taken { absence: AbsenceKind },
    /// The window has run out on its own: the next session has begun.
    Spent,
}

/// How much of a session's window is left.
///
/// **From the later of its own slot and now, to the start of the next slot of
/// any discipline.** Counted in parts of a day, and every part counts rather
/// than only the training ones: a session may be performed at any point in its
/// window. The part in progress counts until it ends, which is why the operator
/// can say that on a Saturday evening there is *"in theory"* still time for
/// Friday's gym test, and that running on the Sunday morning — the cycling slot
/// having begun — leaves *"no time left for the second gym session"*.
///
/// `closes` is `None` where the diary describes nothing after this session, in
/// which case the window is open: a week beyond everything the operator has
/// said is a week nothing is known about, not a deadline.
#[must_use]
pub fn remaining(slot: DayPart, closes: Option<DayPart>, now: DayPart, diary: &Diary) -> Remaining {
    let Some(closes) = closes else {
        return Remaining::Time;
    };

    let mut cursor = slot.max(now);
    let mut taken = None;
    while cursor < closes {
        match diary.taken(cursor) {
            None => return Remaining::Time,
            Some(absence) => taken = taken.or(Some(absence)),
        }
        let Some(next) = cursor.next() else {
            break;
        };
        cursor = next;
    }

    taken.map_or(Remaining::Spent, |absence| Remaining::Taken { absence })
}

/// Where a session stands, from its window and what the store holds.
///
/// **The one place the rule lives**, and it consults nothing it is not given:
/// the same session, window and record produce the same state on any machine
/// and at any time (§ 17).
///
/// The checks run in this order:
///
/// 1. Performed — whatever else is true.
/// 2. Prescribed: still time, or else not performed.
/// 3. Not prescribed, and an absence covers its own slot: skipped.
/// 4. Not prescribed: still time to, or else it never was.
///
/// **A performance is read first, and the operator is why.** His own record
/// has a ride performed on 16 September against a prescription this build
/// delivered and did not write down — *"it was prescribed before the idea of
/// the state machine existed"*. A session the record says happened did happen,
/// and reporting it as *not prescribed* would hide a fact the store holds in
/// order to protect an arrow on a diagram. What the state machine actually
/// rules out is reaching **performed** from anywhere the operator did not
/// train, and that it still does.
#[must_use]
pub fn state_of(
    slot: DayPart,
    closes: Option<DayPart>,
    now: DayPart,
    diary: &Diary,
    recorded: Recorded,
) -> SessionState {
    if recorded.performed {
        return SessionState::Performed;
    }

    let left = remaining(slot, closes, now, diary);

    if recorded.prescribed {
        return match left {
            Remaining::Time => SessionState::Prescribed,
            Remaining::Taken { absence } => SessionState::NotPerformed {
                absence: Some(absence),
            },
            Remaining::Spent => SessionState::NotPerformed { absence: None },
        };
    }

    if let Some(absence) = diary.taken(slot) {
        return SessionState::Skipped { absence };
    }

    match left {
        Remaining::Time => SessionState::ToBePrescribed,
        Remaining::Taken { .. } | Remaining::Spent => SessionState::NotPrescribed,
    }
}

/// What answers for one slot, by its discipline and its role.
fn fill(plan: &Plan, slot: ScheduledSlot) -> Result<Filled<'_>, Unfilled> {
    match slot.discipline {
        Discipline::Gym => {
            let programme = plan.gym().ok_or(Unfilled::NoProgramme {
                discipline: Discipline::Gym,
            })?;
            let (_, mesocycle) = programme.on(slot.date).ok_or(Unfilled::NoMesocycle {
                discipline: Discipline::Gym,
                date: slot.date,
            })?;
            // **The calendar is asked for the week, and the slot supplies the
            // role.** The two agree by construction: the calendar was built
            // against this same diary, so the role it would answer with is the
            // one already on the slot.
            let (week, _) = mesocycle.calendar().place(slot.date)?;
            Ok(Filled::Gym { mesocycle, week })
        }
        Discipline::Cycling => {
            let programme = plan.cycling().ok_or(Unfilled::NoProgramme {
                discipline: Discipline::Cycling,
            })?;
            let (_, mesocycle) = programme.on(slot.date).ok_or(Unfilled::NoMesocycle {
                discipline: Discipline::Cycling,
                date: slot.date,
            })?;
            let microcycle = mesocycle
                .microcycle_of(slot.date)
                .ok_or(Unfilled::NoMesocycle {
                    discipline: Discipline::Cycling,
                    date: slot.date,
                })?;
            let (position, ride) = mesocycle
                .microcycle(microcycle)
                .and_then(|week| week.for_role(slot.role))
                .ok_or(Unfilled::NoSessionForRole { role: slot.role })?;
            Ok(Filled::Cycling {
                mesocycle,
                microcycle,
                position,
                ride,
            })
        }
    }
}

/// The role a slot actually rides, once an illness in its own week is counted.
///
/// **The operator's rule, 2026-09-20**, settling what #177's rule 5 and #180's
/// description both got wrong:
///
/// > "The illness rule was that if one session was lost to illness, the
/// > remaining session would be easier, so by definition, there can't be two
/// > sessions after illness in a microcycle."
///
/// So an illness **consumes** a slot rather than refilling every slot with an
/// easier session. Within the microcycle it falls in, whatever slot survives
/// rides the lower-intensity session — whichever slot that is, and whatever its
/// own role says.
///
/// **Only illness, and only this discipline's.** A holiday is time away and
/// says nothing about fitness; illness is assumed to have cost some, which is
/// why the kind is recorded (#178). And a gym session lost to illness does not
/// ease a ride: the two disciplines are held apart deliberately, and what
/// happens when one waits for the other is the *plan's* business (#177) rather
/// than this slot's.
///
/// **It reads the ordinary week, not the altered one.** An absence takes a
/// day's slots away, so asking the altered week which slots this one shares a
/// microcycle with would hide the very slot that was lost.
#[must_use]
pub fn eased_by_illness(
    diary: &Diary,
    date: Date,
    discipline: Discipline,
    ordinary: SessionRole,
) -> SessionRole {
    let monday = commencing(date);
    let lost = (0..7)
        .filter_map(|offset| monday.checked_add(jiff::Span::new().days(offset)).ok())
        .flat_map(|day| diary.ordinary_slots_of(day))
        .filter(|slot| slot.discipline == discipline)
        // A slot is only *lost* if it is not this one: the session being
        // prescribed has not been missed, whatever the day around it says.
        .filter(|slot| slot.date != date)
        .any(|slot| {
            diary.taken(DayPart::new(slot.date, slot.slot.part)) == Some(AbsenceKind::Illness)
        });

    if lost {
        SessionRole::new(Relative::Lower, Relative::Higher)
    } else {
        ordinary
    }
}
