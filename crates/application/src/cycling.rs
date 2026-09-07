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

use domain::cycling::{CyclingMesocycle, CyclingMesocycleId, PlannedRide, SessionPosition};
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

/// The first ride at or after a date.
///
/// **It crosses a mesocycle boundary.** The autumn authors four cycling
/// programmes back to back, so a question asked in the last days of one has its
/// answer in the next — and answering "nothing" there would be the store's shape
/// showing through as a gap in the plan.
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
) -> Result<(CyclingMesocycle, NextRide), PrescriptionError> {
    let covering = store.on(from).await?;
    let covered = covering.is_some();
    // The programme covering the date answers first, and only a programme with
    // no riding day left defers to the one after it — a mesocycle whose last
    // ride is on the Sunday is still the programme in force on the Saturday.
    if let Some((id, _, programme)) = covering
        && let Some(found) = ride_in(&programme, id, from)
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
    let found =
        ride_in(&programme, id, from).ok_or(PrescriptionError::NoSessionScheduled { from })?;
    Ok((programme, found))
}

fn ride_in(programme: &CyclingMesocycle, id: CyclingMesocycleId, from: Date) -> Option<NextRide> {
    let date = programme.next_riding_day(from)?;
    let (microcycle, session, ride) = programme.on(date)?;
    Some(NextRide {
        programme: id,
        date,
        microcycle,
        session,
        ride: ride.clone(),
    })
}
