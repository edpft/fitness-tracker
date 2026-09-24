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
    prescription::{GymMesocycle, NotScheduled, Skip, WeekKind},
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
        mesocycle: &'a GymMesocycle,
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

/// One session of the microcycle, as the microcycle's own machine reads it.
///
/// **Flat rather than the application's `Session`.** What decides a microcycle
/// is the discipline, the role and the state; the number and the part of the
/// day are the report's business and carrying them here would make this look
/// like a view model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub discipline: Discipline,
    pub role: SessionRole,
    pub state: SessionState,
}

/// The session a microcycle is not a microcycle without.
///
/// **Higher intensity, lower volume — and the code already said so twice.**
/// `GymMesocycle::gating_role` answers this for every progression, and
/// `Test::ROLE` is the same pair because the test *is* the heavy session. The
/// operator, 2026-09-19, on making it the rule at this level: *"That does make
/// life easier, because then we don't need a separate rule for test
/// microcycles."*
pub const ESSENTIAL: SessionRole = SessionRole::new(Relative::Higher, Relative::Lower);

/// What one discipline's microcycle did with the session the plan needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owed {
    /// Its essential session can still be performed.
    Running,
    /// Its essential session was performed. This programme may advance.
    Completed,
    /// The window closed with the essential session unperformed.
    Lost,
    /// The week holds no essential session of this discipline, so it owes
    /// nothing. A real state: the diary allocates the week, and a week it gives
    /// one discipline nothing is a week that discipline is not in.
    Absent,
}

/// Where the concurrent microcycle stands as a whole.
///
/// **The programme is the concurrent one** (the operator, 2026-09-21):
///
/// > From the perspective of `fitness next`, the "programme" is the concurrent
/// > programme, i.e. both the gym and cycling programmes moving together. [...]
/// > So, for the concurrent programme, there should probably be a partially
/// > completed state.
///
/// This is the second of the two machines. [`SessionState`] decides what is
/// delivered into a slot; this decides *which microcycle the week is*, and so
/// what re-runs. Asked session by session, the answer on Monday 21 September
/// is "Monday's gym session" — and nothing in that framing can say the test
/// microcycle did not complete, so the week would be silently spent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicrocycleState {
    /// A discipline's essential session can still be performed.
    Running,
    /// Every discipline's essential session was performed. Both programmes
    /// advance together.
    Completed,
    /// One discipline's essential session was performed and another's was not.
    ///
    /// The one that lost it re-runs its microcycle; the one that completed
    /// rides a holding week, so that the two still start the next mesocycle
    /// together (decision 0034). Cycling's holding week is #180's and the
    /// gym's is the linear template; choosing one here is #190's, and until
    /// then both disciplines re-run the week.
    ///
    /// **Two disciplines, which is what the tool runs.** A third makes this a
    /// pair of lists rather than a pair of names, and that is the edit to make
    /// when a third arrives rather than now.
    PartiallyCompleted {
        completed: Discipline,
        lost: Discipline,
    },
    /// No discipline's essential session was performed. Every one of them
    /// re-runs its microcycle, and no holding week is reached — which is the
    /// operator's week of 14 September 2026.
    Incomplete,
}

impl fmt::Display for MicrocycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Running => f.write_str("running"),
            Self::Completed => f.write_str("completed"),
            Self::PartiallyCompleted { completed, lost } => {
                write!(f, "partially completed ({completed} did, {lost} did not)")
            }
            Self::Incomplete => f.write_str("incomplete"),
        }
    }
}

/// What one discipline's microcycle did, from the states of its sessions.
///
/// **Only the essential session is read.** Every other session of the week is
/// a session the operator may lose without the plan standing still — which is
/// the whole reason the essential one is named. The light gym session lost to
/// Rome on 14 September cost the microcycle nothing; the entry test lost to
/// illness on the 18th cost it everything.
#[must_use]
pub fn owed_by(sessions: &[Placed], discipline: Discipline) -> Owed {
    let Some(essential) = sessions
        .iter()
        .find(|session| session.discipline == discipline && session.role == ESSENTIAL)
    else {
        return Owed::Absent;
    };

    match essential.state {
        SessionState::Performed => Owed::Completed,
        SessionState::ToBePrescribed | SessionState::Prescribed => Owed::Running,
        SessionState::Skipped { .. }
        | SessionState::NotPrescribed
        | SessionState::NotPerformed { .. } => Owed::Lost,
    }
}

/// Where the concurrent microcycle stands, from every session in it.
///
/// **Derived on every read, never stored**, as `progress_after` is: a stored
/// position is a second source of truth about a series the record already
/// determines, and asking twice would then advance it twice.
///
/// A week holding no session at all is [`MicrocycleState::Completed`] — it owes
/// nothing, so nothing is waiting on it. That is the same answer a week every
/// discipline finished gets, and deliberately: what the caller does with this
/// is decide whether the plan moves on, and in both cases it does.
#[must_use]
pub fn microcycle_state(sessions: &[Placed]) -> MicrocycleState {
    let read: Vec<(Discipline, Owed)> = Discipline::ALL
        .iter()
        .copied()
        .map(|discipline| (discipline, owed_by(sessions, discipline)))
        .collect();

    if read.iter().any(|(_, owed)| *owed == Owed::Running) {
        return MicrocycleState::Running;
    }

    let completed: Vec<Discipline> = read
        .iter()
        .filter(|(_, owed)| *owed == Owed::Completed)
        .map(|(discipline, _)| *discipline)
        .collect();
    let lost: Vec<Discipline> = read
        .iter()
        .filter(|(_, owed)| *owed == Owed::Lost)
        .map(|(discipline, _)| *discipline)
        .collect();

    match (completed.first(), lost.first()) {
        (_, None) => MicrocycleState::Completed,
        (None, Some(_)) => MicrocycleState::Incomplete,
        (Some(completed), Some(lost)) => MicrocycleState::PartiallyCompleted {
            completed: *completed,
            lost: *lost,
        },
    }
}

/// Why a plan could not be rescheduled.
///
/// **Not reachable from a plan that authored**, as far as anything here can
/// tell: moving a mesocycle later or skipping a week inside it only ever gives
/// a calendar more room. It is a type rather than a panic because § 26 forbids
/// the panic, and because a diary that changed under a plan might yet find a
/// way.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unreschedulable {
    #[error(transparent)]
    Calendar(#[from] crate::prescription::InvalidCalendar),
    #[error(transparent)]
    Programme(#[from] crate::plan::InvalidProgramme),
    #[error(transparent)]
    Plan(#[from] crate::plan::EmptyPlan),
}

/// The plan as it now stands, given the weeks the concurrent microcycle was
/// lost in.
///
/// **The operator's rule, 2026-09-21: "It should shift all start dates.
/// That's the point."** And, asked whether the plan as authored should still be
/// shown: *"Okay, we could see what we originally planned, but that's not what
/// we did, so why should we care?"* So this is the plan, not a view of it.
///
/// **A lost week is an interruption of the mesocycle in force.** The gym's
/// calendar already has the rule that re-runs it — a week with no session left
/// in it is not a training week — and a cycling mesocycle is given the same
/// rule by [`CyclingMesocycle::losing`]. What was issued in the lost week still
/// belongs to the mesocycle it was issued for, so nothing already recorded is
/// re-described (§ 12).
///
/// **Every later mesocycle starts where the one before it now ends**, and is
/// read against the diary as it now stands: the interruptions authored with it
/// named days in weeks it no longer occupies, so they are re-derived for the
/// weeks it does. A mesocycle nothing pushed is left exactly as authored.
///
/// `lost` is the Monday of each lost week, and is the same for both
/// disciplines: the programme is the concurrent one, and the two start their
/// mesocycles together. A shift that leaves a later mesocycle unable to
/// complete is #201's, not this function's.
///
/// # Errors
///
/// [`Unreschedulable`] where a moved calendar or the programme it belongs to
/// will not build.
pub fn rescheduled(plan: &Plan, lost: &[Date], diary: &Diary) -> Result<Plan, Unreschedulable> {
    if lost.is_empty() {
        return Ok(plan.clone());
    }

    let gym = plan
        .gym()
        .map(|programme| -> Result<_, Unreschedulable> {
            let mut moved = Vec::new();
            let mut cursor: Option<Date> = None;
            for mesocycle in programme.mesocycles() {
                let next = gym_rescheduled(mesocycle, cursor, lost, diary)?;
                cursor = Some(crate::plan::Occupies::span(&next).end());
                moved.push(next);
            }
            Ok(crate::plan::Programme::new(moved)?)
        })
        .transpose()?;

    let cycling = plan
        .cycling()
        .map(|programme| -> Result<_, Unreschedulable> {
            let mut moved = Vec::new();
            let mut cursor: Option<Date> = None;
            for mesocycle in programme.mesocycles() {
                let next = cycling_rescheduled(mesocycle, cursor, lost);
                cursor = Some(crate::plan::Occupies::span(&next).end());
                moved.push(next);
            }
            Ok(crate::plan::Programme::new(moved)?)
        })
        .transpose()?;

    Ok(Plan::new(
        plan.name().clone(),
        plan.authored_at(),
        gym,
        cycling,
    )?)
}

/// One gym mesocycle, started no earlier than `cursor` and with every lost
/// week inside it skipped.
fn gym_rescheduled(
    mesocycle: &GymMesocycle,
    cursor: Option<Date>,
    lost: &[Date],
    diary: &Diary,
) -> Result<GymMesocycle, Unreschedulable> {
    let calendar = mesocycle.calendar();
    let authored = calendar.start();
    let start = cursor.map_or(authored, |cursor| cursor.max(authored));

    // **Authored skips where it has not moved; the diary's where it has.**
    let base: Vec<Skip> = if start == authored {
        calendar.interruptions().iter().collect()
    } else {
        Vec::new()
    };

    // The span grows with what it skips, and what it skips depends on the span:
    // walked to a fixed point, which it reaches because every pass can only
    // add weeks, and a plan's lost weeks are finite.
    let mut moved = calendar.moved(start, &base)?;
    for _ in 0..=lost.len().saturating_add(8) {
        let end = crate::plan::Occupies::span(&mesocycle.recalendared(moved.clone())).end();
        let mut skips = base.clone();
        if start != authored {
            let final_day = end.yesterday().unwrap_or(end);
            skips.extend(
                diary
                    .unavailable(start, final_day, Discipline::Gym)
                    .into_iter()
                    .map(Skip::day),
            );
        }
        skips.extend(
            lost.iter()
                .filter(|monday| **monday >= start && **monday < end)
                .map(|monday| Skip::new(*monday, SEVEN_DAYS)),
        );
        let next = calendar.moved(start, &skips)?;
        if next == moved {
            break;
        }
        moved = next;
    }

    Ok(mesocycle.recalendared(moved))
}

/// One cycling mesocycle, started no earlier than `cursor` and with every lost
/// week inside it lost.
fn cycling_rescheduled(
    mesocycle: &CyclingMesocycle,
    cursor: Option<Date>,
    lost: &[Date],
) -> CyclingMesocycle {
    let authored = mesocycle.start();
    let start = cursor.map_or(authored, |cursor| cursor.max(authored));
    let mut moved = if start == authored {
        mesocycle.clone()
    } else {
        mesocycle.starting_on(start)
    };
    // As for the gym: losing a week lengthens the span, which may reach the
    // next lost week. `losing` ignores a Monday outside the span, so repeating
    // until nothing changes is enough.
    for _ in 0..=lost.len() {
        let next = lost
            .iter()
            .fold(moved.clone(), |held, monday| held.losing(*monday));
        if next == moved {
            break;
        }
        moved = next;
    }
    moved
}

/// A calendar week, as a run of skipped days.
const SEVEN_DAYS: std::num::NonZeroU8 = match std::num::NonZeroU8::new(7) {
    Some(days) => days,
    None => std::num::NonZeroU8::MIN,
};
