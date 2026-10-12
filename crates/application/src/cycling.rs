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
    cycling::{
        CyclingMesocycle, CyclingMesocycleId, CyclingSession, DeliveredRide, Ftp, PlannedRide,
        RideVenue, SessionPosition,
    },
    measure::PositiveDuration,
    planner,
    provider::ProgrammeName,
    schedule::{Diary, Discipline, SessionRole, TrainingWeek},
    sequence::NonEmpty,
};
use jiff::civil::Date;

use crate::{
    CyclingDeliveryStore, CyclingMesocycleStore, DestinationName, FtpHistory, HoldingRides,
    PlanStore, PrescriptionError, RiddenVenues, RideDestination, RidesPlaced, SourceError,
    StoreError,
    holding::NoHoldingRide,
    reschedule::{Reschedule, Rescheduled},
};

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
    let ordinary = week.role_on(date.weekday())?;
    // A skipped essential ride earlier in the week rides here instead (#392).
    let asked = planner::role_on(diary, date, Discipline::Cycling).unwrap_or(ordinary);
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
    let (session, ride) = if eased == ordinary {
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

/// The operator's own cool-down, ridden after the minute Peloton builds in.
///
/// A generation parameter (§ 14) held here rather than in the authored
/// programme, so the record of what a class actually contains stays faithful.
const EXTRA_COOL_DOWN_SECONDS: u64 = 300;

/// Why the next ride could not be said.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RidingError {
    #[error(transparent)]
    Prescription(#[from] PrescriptionError),
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The schedule gives cycling no day to ride on.
    #[error(
        "the schedule gives cycling no day of the week as of {from}, so there is no day to ride on"
    )]
    NoDay { from: Date },
    /// A holding week's ride could not be chosen.
    #[error(transparent)]
    Holding(#[from] NoHoldingRide),
}

/// Why a ride was not delivered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RideDeliveryError {
    /// The destination holds rides, and replacing them was not asked for.
    #[error(
        "the stack already holds {holds} class(es), and it is shared with everything else \
         you queue. Delivering replaces the whole list, so this would discard them"
    )]
    Occupied { holds: usize },
    #[error(transparent)]
    Source(#[from] SourceError),
    /// Delivered, and the record of it could not be written.
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{detail}")]
    Unbuildable { detail: String },
}

/// The next ride, as it will be ridden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrescribedRide {
    /// The mesocycle it belongs to.
    pub mesocycle: CyclingMesocycle,
    pub next: NextRide,
    /// The published programme it was taken from. `None` for a holding ride,
    /// which nobody published.
    pub published: Option<ProgrammeName>,
    /// What is ridden, with the operator's own cool-down added.
    pub session: CyclingSession,
    /// The FTP in force on the ride's date, where a test or an override says.
    pub ftp: Option<Ftp>,
}

impl PrescribedRide {
    /// Whether it is a ride of a holding week rather than of the programme.
    #[must_use]
    pub const fn holding(&self) -> bool {
        self.published.is_none()
    }
}

/// Everything the next ride is read from.
pub struct RidingPorts<S, P, H, R, F, Y> {
    /// Cycling's mesocycles as authored; read through the reschedule each call.
    pub mesocycles: S,
    pub plans: P,
    /// The catalogue a holding ride is chosen from.
    pub holding: H,
    pub ridden: R,
    pub ftp: F,
    pub deliveries: Y,
}

/// The use case: which ride is next, and putting it where it is ridden.
///
/// **Reads the store and nothing else until it delivers.** What a class
/// contains was read when the programme was authored, so a source being
/// unavailable costs the delivery and not the answer (§ 36).
pub struct Riding<S, P, H, R, F, Y> {
    ports: RidingPorts<S, P, H, R, F, Y>,
    /// Where this build's rides are delivered, as the record names it.
    destination: DestinationName,
}

impl<S, P, H, R, F, Y> Riding<S, P, H, R, F, Y>
where
    S: CyclingMesocycleStore + Clone + Sync,
    P: PlanStore + Clone + Sync,
    H: HoldingRides + Sync,
    R: RiddenVenues + Sync,
    F: FtpHistory + Sync,
    Y: CyclingDeliveryStore + Sync,
{
    pub const fn new(ports: RidingPorts<S, P, H, R, F, Y>, destination: DestinationName) -> Self {
        Self { ports, destination }
    }

    /// The first ride at or after a date, as the plan now stands.
    ///
    /// **The FTP is the one in force on the ride's date, not on today's.** The
    /// next ride may be a fortnight away, and a test ridden between now and
    /// then is the one that applies to it. An override stands whatever the
    /// record says.
    ///
    /// # Errors
    ///
    /// [`RidingError`] if no programme covers the date, if the schedule gives
    /// cycling no day, if a holding ride cannot be chosen, or if the store is
    /// unavailable.
    pub async fn next(
        &self,
        from: Date,
        reschedule: Reschedule,
        diary: &Diary,
        ftp: Option<Ftp>,
    ) -> Result<PrescribedRide, RidingError> {
        let store = Rescheduled::new(
            self.ports.mesocycles.clone(),
            self.ports.plans.clone(),
            reschedule,
        );
        let week = diary
            .training_week(from, Discipline::Cycling)
            .ok_or(RidingError::NoDay { from })?;
        let (mesocycle, due) = next_ride(&store, from, &week, diary).await?;

        let (next, published) = match due {
            Due::Ride(next) => (next, mesocycle.programme().map(|from| from.name().clone())),
            Due::Holding(day) => (
                crate::holding::ride(&self.ports.holding, &self.ports.ridden, day).await?,
                None,
            ),
        };

        let ftp = match ftp {
            Some(asserted) => Some(asserted),
            None => self.ports.ftp.in_force_on(next.date).await?,
        };

        let extra = PositiveDuration::from_seconds(EXTRA_COOL_DOWN_SECONDS).map_err(|error| {
            StoreError::Corrupt {
                detail: error.to_string(),
            }
        })?;
        let session = next.ride.session().with_extra_cool_down(extra);

        Ok(PrescribedRide {
            mesocycle,
            next,
            published,
            session,
            ftp,
        })
    }

    /// Put a ride where it is ridden, and record what went.
    ///
    /// **The record is written after the destination, and only if the
    /// destination took it.** It is the evidence that a ride was prescribed —
    /// nothing else in the store says so, because a cycling session is authored
    /// in full and has no derived prescription to leave behind (#185).
    ///
    /// **What is recorded is what went, not what the programme holds.** The
    /// cool-down is the destination's choice and appears in no authored record.
    ///
    /// # Errors
    ///
    /// [`RideDeliveryError::Occupied`] if the destination holds rides and
    /// `replace` is false, and otherwise whatever the destination or the store
    /// gives.
    pub async fn deliver<T: RideDestination + Sync>(
        &self,
        ride: &PrescribedRide,
        to: &T,
        replace: bool,
    ) -> Result<RidesPlaced, RideDeliveryError> {
        if !replace {
            let holds = to.holding().await?;
            if holds > 0 {
                return Err(RideDeliveryError::Occupied { holds });
            }
        }
        let placed = to.deliver(ride.next.ride.at()).await?;

        let unbuildable = |error: &dyn std::fmt::Display| RideDeliveryError::Unbuildable {
            detail: error.to_string(),
        };
        let mut written: Vec<RideVenue> = ride.next.ride.at().iter().cloned().collect();
        written.push(placed.cool_down.clone());
        let microcycle = u32::try_from(ride.next.microcycle).map_err(|_| {
            unbuildable(&format!(
                "microcycle {} is past counting",
                ride.next.microcycle
            ))
        })?;

        self.ports
            .deliveries
            .record(&DeliveredRide {
                prescribed_for: ride.next.date,
                destination: self.destination.clone(),
                programme: ride.published.clone(),
                microcycle,
                session: ride.next.session,
                classes: NonEmpty::new(written).map_err(|error| unbuildable(&error))?,
                delivered_at: jiff::Timestamp::now(),
            })
            .await?;
        Ok(placed)
    }
}
