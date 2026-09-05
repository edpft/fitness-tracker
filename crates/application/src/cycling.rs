//! Authoring a cycling programme, and asking it what is next.
//!
//! **Named for the discipline where its neighbours are named for the act**, and
//! that is decision 0026 rather than untidiness. Authoring a mesocycle and
//! issuing a session are the same *acts* [`prescribe`](crate::prescribe)
//! performs for the gym; what differs is the bounded context, and putting a
//! cycling mesocycle through a module built around a primary lift, an anchor and
//! a gating role would be exactly the mixing that decision forbids. The two
//! share their rule about succession and share nothing else.
//!
//! **Nothing here reads the network.** What a class contains was read when the
//! programme was authored and is stored in full (§ 13), so a prescription issued
//! last month stays reproducible and a source being unavailable costs nothing
//! (§ 36).

use domain::cycling::{CyclingProgramme, CyclingProgrammeId, PlannedRide, SessionPosition};
use jiff::civil::Date;

use crate::{Authored, CyclingProgrammeStore, PrescriptionError};

/// Author a cycling mesocycle, refusing one that would compete for a day.
///
/// **The gym's rule, applied to cycling's own set.** Two cycling programmes
/// answering for one date would make which of them answers depend on the order
/// rows came back in. A cycling programme overlapping the *gym* block beside it
/// is not merely allowed but is the point, and nothing here can see one.
///
/// Versions of one programme never conflict: a shared name is a re-authoring,
/// which [`ProgrammeWindow::overlaps`](domain::prescription::ProgrammeWindow::overlaps)
/// already knows.
///
/// # Errors
///
/// [`PrescriptionError::OverlappingProgramme`] if another cycling programme
/// covers any of the same days, or [`PrescriptionError::Store`] if the store is
/// unavailable.
pub async fn author<S: CyclingProgrammeStore + Sync>(
    store: &S,
    programme: &CyclingProgramme,
) -> Result<(CyclingProgrammeId, Authored), PrescriptionError> {
    let proposed = programme.window();
    let mut authored = Authored::Created;
    for existing in store.windows().await? {
        if existing.name() == proposed.name() {
            authored = Authored::Modified;
            continue;
        }
        if proposed.overlaps(&existing) {
            return Err(PrescriptionError::OverlappingProgramme { proposed, existing });
        }
    }
    Ok((store.author(programme).await?, authored))
}

/// The next ride, and everything needed to say where it sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextRide {
    pub programme: CyclingProgrammeId,
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
/// [`PrescriptionError::NoProgramme`] if no cycling programme covers the date or
/// follows it, [`PrescriptionError::NoSessionScheduled`] if the programmes that
/// do have no riding day left, or [`PrescriptionError::Store`] if the store is
/// unavailable.
pub async fn next_ride<S: CyclingProgrammeStore + Sync>(
    store: &S,
    from: Date,
) -> Result<(CyclingProgramme, NextRide), PrescriptionError> {
    let covering = store.on(from).await?;
    let covered = covering.is_some();
    // The programme covering the date answers first, and only a programme with
    // no riding day left defers to the one after it — a mesocycle whose last
    // ride is on the Sunday is still the programme in force on the Saturday.
    if let Some((id, programme)) = covering
        && let Some(found) = ride_in(&programme, id, from)
    {
        return Ok((programme, found));
    }

    // **Two different answers, and the message is the whole difference.** A date
    // no programme covers is a gap in the plan; a date one covers with no ride
    // left in it and nothing after is a block that has finished.
    let Some((id, programme)) = store.following(from).await? else {
        return Err(if covered {
            PrescriptionError::NoSessionScheduled { from }
        } else {
            PrescriptionError::NoProgramme { date: from }
        });
    };
    let found =
        ride_in(&programme, id, from).ok_or(PrescriptionError::NoSessionScheduled { from })?;
    Ok((programme, found))
}

fn ride_in(programme: &CyclingProgramme, id: CyclingProgrammeId, from: Date) -> Option<NextRide> {
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
