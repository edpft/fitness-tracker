//! Asking the authored cycling programme what is next.
//!
//! **Named for the discipline where its neighbours are named for the act**, and
//! that is decision 0026 rather than untidiness. Issuing a session is the same
//! *act* [`prescribe`](crate::prescribe) performs for the gym; what differs is
//! the bounded context, and putting a cycling mesocycle through a module built
//! around a primary lift, an anchor and a gating role would be exactly the
//! mixing that decision forbids.
//!
//! **Authoring left here on 2026-09-06.** A cycling mesocycle is not authored on
//! its own any more: it is part of a plan, and the plan is what is written and
//! what the overlap rule reads (issue #86).
//!
//! **Nothing here reads the network.** What a class contains was read when the
//! programme was authored and is stored in full (§ 13), so a prescription issued
//! last month stays reproducible and a source being unavailable costs nothing
//! (§ 36).

use domain::{
    cycling::{CyclingMesocycle, CyclingMesocycleId, PlannedRide, SessionPosition},
    planner,
    schedule::{Diary, Discipline, SessionRole, TrainingWeek},
};
use jiff::civil::Date;

use crate::{CyclingMesocycleStore, PrescriptionError};

/// The next ride, and everything needed to say where it sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextRide {
    pub programme: CyclingMesocycleId,
    /// The date it is ridden.
    pub date: Date,
    /// Which microcycle of the authored programme, counting from one.
    pub microcycle: usize,
    /// Which session of that microcycle.
    pub session: SessionPosition,
    pub ride: PlannedRide,
}

/// What is due on the next riding day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Due {
    /// A ride of the programme, authored in full.
    Ride(NextRide),
    /// A ride of a holding week (#190), which nobody has chosen yet.
    ///
    /// **Chosen when it is delivered**, from the catalogue: the newest class of
    /// this role not yet ridden ([`crate::holding::choose`]). So it needs the
    /// network where a ride of the programme does not.
    Holding(HoldingDay),
}

impl Due {
    /// The ride of the programme, where that is what is due.
    #[must_use]
    pub fn ride(self) -> Option<NextRide> {
        match self {
            Self::Ride(ride) => Some(ride),
            Self::Holding(_) => None,
        }
    }
}

/// A day of a holding week, and what kind of ride it asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoldingDay {
    pub programme: CyclingMesocycleId,
    pub date: Date,
    /// Which of this mesocycle's holding weeks, counting from one.
    pub week: usize,
    /// The role the slot rides, once an illness in the same week is counted.
    pub role: SessionRole,
}

/// The first ride at or after a date.
///
/// **It crosses a mesocycle boundary.** The autumn authors four cycling
/// programmes back to back, so a question asked in the last days of one has its
/// answer in the next — and answering "nothing" there would be the store's shape
/// showing through as a gap in the plan.
///
/// **The week is cycling's slots, as the diary gives them**, and it is passed
/// in rather than looked up: which weekday takes which ride is a fact about the
/// operator's life rather than about the programme (issue #63), and the caller
/// is what holds the diary.
///
/// # Errors
///
/// [`PrescriptionError::NoPlan`] if no cycling programme covers the date or
/// follows it, [`PrescriptionError::NoSessionScheduled`] if the programmes that
/// do have no riding day left, or [`PrescriptionError::Store`] if the store is
/// unavailable.
pub async fn next_ride<S: CyclingMesocycleStore + Sync>(
    store: &S,
    from: Date,
    week: &TrainingWeek,
    diary: &Diary,
) -> Result<(CyclingMesocycle, Due), PrescriptionError> {
    let covering = store.on(from).await?;
    let covered = covering.is_some();
    // The programme covering the date answers first, and only a programme with
    // no riding day left defers to the one after it — a mesocycle whose last
    // ride is on the Sunday is still the programme in force on the Saturday.
    if let Some((id, _, programme)) = covering
        && let Some(found) = ride_in(&programme, id, from, week, diary)
    {
        return Ok((programme, found));
    }

    // **Two different answers, and the message is the whole difference.** A date
    // no programme covers is a gap in the plan; a date one covers with no ride
    // left in it and nothing after is a block that has finished.
    let Some((id, _, programme)) = store.following(from).await? else {
        return Err(if covered {
            PrescriptionError::NoSessionScheduled { from }
        } else {
            PrescriptionError::NoPlan { date: from }
        });
    };
    let found = ride_in(&programme, id, from, week, diary)
        .ok_or(PrescriptionError::NoSessionScheduled { from })?;
    Ok((programme, found))
}

/// What one programme puts on the next day it rides, or holds.
///
/// **An illness in the same microcycle eases what survives it** (#180). The
/// slot's own role says what the week ordinarily asks of this day; if another
/// cycling slot that week was lost to illness, the operator's rule is that the
/// remaining session is the easier one, and the week is asked for that ride
/// instead.
///
/// A microcycle that has no ride in the eased role falls back to the one the
/// slot asked for: a week holding a single session is still a week to ride,
/// and answering nothing would be worse than answering the harder ride.
fn ride_in(
    programme: &CyclingMesocycle,
    id: CyclingMesocycleId,
    from: Date,
    week: &TrainingWeek,
    diary: &Diary,
) -> Option<Due> {
    let date = programme.next_riding_day(from, week)?;
    let asked = week.role_on(date.weekday())?;
    let eased = planner::eased_by_illness(diary, date, Discipline::Cycling, asked);

    if programme.holds_on(date) {
        let week = programme
            .held()
            .iter()
            .take_while(|monday| **monday <= date)
            .count();
        return Some(Due::Holding(HoldingDay {
            programme: id,
            date,
            week,
            role: eased,
        }));
    }
    let (microcycle, session, ride) = programme.on(date, week)?;
    let (session, ride) = if eased == asked {
        (session, ride)
    } else {
        programme
            .microcycle(microcycle)
            .and_then(|held| held.for_role(eased))
            .unwrap_or((session, ride))
    };

    Some(Due::Ride(NextRide {
        programme: id,
        date,
        microcycle,
        session,
        ride: ride.clone(),
    }))
}
