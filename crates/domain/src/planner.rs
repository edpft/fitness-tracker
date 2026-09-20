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

use jiff::civil::Date;

use crate::{
    cycling::{CyclingMesocycle, PlannedRide, SessionPosition},
    plan::Plan,
    prescription::{Mesocycle, NotScheduled, WeekKind},
    schedule::{Diary, Discipline, ScheduledSlot, SessionRole},
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
#[must_use]
pub fn week<'a>(plan: &'a Plan, diary: &Diary, containing: Date) -> Vec<Planned<'a>> {
    let monday = commencing(containing);
    (0..7)
        .filter_map(|offset| monday.checked_add(jiff::Span::new().days(offset)).ok())
        .flat_map(|date| diary.slots_of(date))
        .map(|slot| Planned {
            session: fill(plan, slot),
            slot,
        })
        .collect()
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
