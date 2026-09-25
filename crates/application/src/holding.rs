//! Choosing the rides a holding microcycle is built from.
//!
//! **A holding week is assembled, not provided** (#180). Every other cycling
//! mesocycle is microcycles of a published Peloton programme, taken whole; this
//! one is two classes picked out of the catalogue because the operator is
//! waiting for the other discipline to catch up, and the published programme it
//! would otherwise belong to does not exist.
//!
//! **Two facts decide each ride**, and the operator stated both on 2026-09-20:
//! *"The lower intensity ride will be a power zone endurance ride, 45 minutes,
//! the newest that hasn't already been taken. The higher intensity ride will be
//! a regular 45 minute power zone ride, again the newest that hasn't been
//! taken."*
//!
//! - **What kind of class**, which the catalogue answers by role.
//! - **Whether it has been ridden**, which the record answers.
//!
//! The two are separate ports on purpose. What a Power Zone Endurance Ride *is*
//! belongs to Peloton and changes when Peloton changes it; what has been ridden
//! belongs to the record and is nobody else's business. A single port answering
//! both would have made the catalogue read the store.
//!
//! **"Taken" means ridden, not prescribed.** A class this tool put in a plan
//! the operator never rode is still available to him; a class he rode under his
//! own steam last month is not. That is his rule and it is the stronger of the
//! two readings.

use std::collections::{BTreeMap, BTreeSet};

use domain::{
    cycling::{
        CyclingMesocycle, CyclingMicrocycle, CyclingProvenance, PlannedRide, RideVenue,
        SessionPosition,
    },
    schedule::SessionRole,
    sequence::NonEmpty,
};
use jiff::civil::Date;

use crate::{
    error::SourceError,
    ports::{HoldingRides, RiddenVenues},
};

/// Why a holding ride could not be chosen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NoHoldingRide {
    /// The catalogue holds classes of that shape and every one has been ridden.
    ///
    /// **A real answer rather than a fault**, and it names the count so the
    /// message can say how deep it looked: the operator has ridden 29 of the
    /// 45-minute endurance classes, and a day when he has ridden all of them is
    /// a day this rule needs revisiting rather than a bug.
    #[error(
        "every one of the {considered} classes that could be the {role} ride \
         has already been ridden"
    )]
    AllRidden {
        role: SessionRole,
        considered: usize,
    },
    /// The catalogue holds no class of that shape at all.
    #[error("no class in the catalogue could be the {role} ride")]
    NoneOffered { role: SessionRole },
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Store(#[from] crate::error::StoreError),
    /// The rides were chosen and will not make a week.
    ///
    /// **Not reachable from the catalogue.** The two roles are different by
    /// construction and the classes are distinct, so this is the domain
    /// refusing something this module built wrongly rather than a fact about
    /// Peloton — which is why it carries the domain's own words.
    #[error("the chosen rides do not make a holding microcycle: {detail}")]
    Unbuildable { detail: String },
}

/// The newest class of one role that has not been ridden.
///
/// **Newest is the catalogue's order, not a date compared here.** The source is
/// asked for its classes newest first and the first acceptable one is taken, so
/// nothing in this crate parses an air date or decides what "newest" means —
/// which is right, because it is Peloton's ordering of Peloton's catalogue.
///
/// # Errors
///
/// [`NoHoldingRide`] where nothing is offered, where everything offered has
/// been ridden, or where either port fails.
pub async fn choose<C: HoldingRides + Sync, R: RiddenVenues + Sync>(
    catalogue: &C,
    record: &R,
    role: SessionRole,
) -> Result<RideVenue, NoHoldingRide> {
    let offered = catalogue.candidates(role).await?;
    if offered.is_empty() {
        return Err(NoHoldingRide::NoneOffered { role });
    }
    let ridden = record.ridden().await?;
    let considered = offered.len();
    offered
        .into_iter()
        .find(|candidate| !ridden.contains(candidate))
        .ok_or(NoHoldingRide::AllRidden { role, considered })
}

/// Both rides of a holding week, chosen together.
///
/// **Together rather than twice, because the two must not collide.** A class
/// cannot be both the higher- and the lower-intensity ride — the series that
/// select them are disjoint, so it cannot happen today — but a week that
/// prescribed one class twice would be a week with one session in it, and
/// nothing downstream would say so. Choosing here is where that is checkable.
///
/// # Errors
///
/// [`NoHoldingRide`] as [`choose`] gives it, for whichever role fails first.
pub async fn both<C: HoldingRides + Sync, R: RiddenVenues + Sync>(
    catalogue: &C,
    record: &R,
    higher: SessionRole,
    lower: SessionRole,
) -> Result<(RideVenue, RideVenue), NoHoldingRide> {
    besides(catalogue, record, higher, lower, &BTreeSet::new()).await
}

/// Both rides of a holding week, neither of them one already `taken`.
///
/// **What lets a hold run longer than a week** (#222): each week's classes are
/// taken before the next week's are chosen, so two weeks of one hold never
/// prescribe the same class.
async fn besides<C: HoldingRides + Sync, R: RiddenVenues + Sync>(
    catalogue: &C,
    record: &R,
    higher: SessionRole,
    lower: SessionRole,
    taken: &BTreeSet<RideVenue>,
) -> Result<(RideVenue, RideVenue), NoHoldingRide> {
    let offered = catalogue.candidates(higher).await?;
    if offered.is_empty() {
        return Err(NoHoldingRide::NoneOffered { role: higher });
    }
    let ridden = record.ridden().await?;
    let considered = offered.len();
    let first = offered
        .into_iter()
        .find(|candidate| !ridden.contains(candidate) && !taken.contains(candidate))
        .ok_or(NoHoldingRide::AllRidden {
            role: higher,
            considered,
        })?;
    // The one already chosen is not available to the other role, whatever the
    // catalogue says. Cheaper than a second rule about series being disjoint,
    // and it holds if they ever stop being.
    let mut taken = taken.clone();
    taken.insert(first.clone());
    let offered = catalogue.candidates(lower).await?;
    if offered.is_empty() {
        return Err(NoHoldingRide::NoneOffered { role: lower });
    }
    let ridden = record.ridden().await?;
    let considered = offered.len();
    let second = offered
        .into_iter()
        .find(|candidate| !ridden.contains(candidate) && !taken.contains(candidate))
        .ok_or(NoHoldingRide::AllRidden {
            role: lower,
            considered,
        })?;
    Ok((first, second))
}

/// A whole holding microcycle: two rides, roled and ready to author.
///
/// **The roles are stated, not derived.** Both rides are the same length and
/// the volume comparison admits equality (the operator, 2026-09-20), so nothing
/// about a 45-minute Power Zone ride and a 45-minute Power Zone Endurance ride
/// says which is the harder one. What says it is the *kind* of class, which is
/// the catalogue's business and settled by the time the venues come back.
///
/// **The content is fetched after the choice, never before.** A listing answers
/// by the hundred and each class's zone plan is a request of its own; asking
/// for all of them to take one would be a hundred requests to discard
/// ninety-nine.
///
/// # Errors
///
/// [`NoHoldingRide`] where either role finds nothing, where everything offered
/// has been ridden, or where a chosen class will not read as a session.
pub async fn microcycle<C: HoldingRides + Sync, R: RiddenVenues + Sync>(
    catalogue: &C,
    record: &R,
    higher: SessionRole,
    lower: SessionRole,
) -> Result<CyclingMicrocycle, NoHoldingRide> {
    microcycle_besides(catalogue, record, higher, lower, &BTreeSet::new()).await
}

async fn microcycle_besides<C: HoldingRides + Sync, R: RiddenVenues + Sync>(
    catalogue: &C,
    record: &R,
    higher: SessionRole,
    lower: SessionRole,
    taken: &BTreeSet<RideVenue>,
) -> Result<CyclingMicrocycle, NoHoldingRide> {
    let (harder, easier) = besides(catalogue, record, higher, lower, taken).await?;

    let mut rides = BTreeMap::new();
    for (position, venue, role) in [(1_u8, &harder, higher), (2, &easier, lower)] {
        let session = catalogue.session_at(venue).await?;
        let position = SessionPosition::new(position).map_err(|_| NoHoldingRide::Unbuildable {
            detail: "a session position counting from zero".to_owned(),
        })?;
        rides.insert(
            position,
            PlannedRide::assembled(session, NonEmpty::of(venue.clone(), Vec::new()), role),
        );
    }

    CyclingMicrocycle::new(rides).map_err(|error| NoHoldingRide::Unbuildable {
        detail: error.to_string(),
    })
}

/// One holding microcycle as a mesocycle, ready to sit in a plan.
///
/// **A mesocycle of one week, not a new kind of thing.** A plan holds
/// mesocycles and a holding week has to be one of them, or `cycling next` would
/// need a second place to look. What makes it a holding week is its provenance:
/// nobody published it.
///
/// # Errors
///
/// [`NoHoldingRide`] as [`microcycle`] gives it, and where the week will not
/// make a mesocycle.
pub async fn mesocycle<C: HoldingRides + Sync, R: RiddenVenues + Sync>(
    catalogue: &C,
    record: &R,
    start: Date,
    higher: SessionRole,
    lower: SessionRole,
) -> Result<CyclingMesocycle, NoHoldingRide> {
    let week = microcycle(catalogue, record, higher, lower).await?;
    CyclingMesocycle::new(
        CyclingProvenance::Assembled,
        start,
        NonEmpty::of(week, Vec::new()),
    )
    .map_err(|error| NoHoldingRide::Unbuildable {
        detail: error.to_string(),
    })
}

/// Several holding microcycles as one mesocycle: the holding weeks of a hold
/// (#222), before the test week that ends it.
///
/// **No class twice.** Each week's rides are the newest not yet ridden and not
/// already taken by an earlier week of this hold.
///
/// # Errors
///
/// [`NoHoldingRide`] as [`microcycle`] gives it, for whichever week fails first.
pub async fn weeks<C: HoldingRides + Sync, R: RiddenVenues + Sync>(
    catalogue: &C,
    record: &R,
    start: Date,
    count: std::num::NonZeroU32,
    higher: SessionRole,
    lower: SessionRole,
) -> Result<CyclingMesocycle, NoHoldingRide> {
    let mut taken = BTreeSet::new();
    let mut held = Vec::new();
    for _ in 0..count.get() {
        let week = microcycle_besides(catalogue, record, higher, lower, &taken).await?;
        taken.extend(
            week.rides()
                .values()
                .flat_map(|ride| ride.at().iter().cloned().collect::<Vec<_>>()),
        );
        held.push(week);
    }
    let weeks = NonEmpty::new(held).map_err(|_| NoHoldingRide::Unbuildable {
        detail: "a hold of no weeks".to_owned(),
    })?;
    CyclingMesocycle::new(CyclingProvenance::Assembled, start, weeks).map_err(|error| {
        NoHoldingRide::Unbuildable {
            detail: error.to_string(),
        }
    })
}

/// The ride a day of a holding week asks for, chosen now (#190).
///
/// **One ride, not the week.** A holding week that follows a test is worked out
/// on every run rather than written to the plan, so its rides are chosen as
/// each is delivered: the newest class of the day's role not yet ridden. The
/// harder ride is the week's first session and the easier its second, as in
/// [`microcycle`].
///
/// # Errors
///
/// [`NoHoldingRide`] as [`choose`] gives it, or where the chosen class will not
/// read as a session.
pub async fn ride<C: HoldingRides + Sync, R: RiddenVenues + Sync>(
    catalogue: &C,
    record: &R,
    day: crate::cycling::HoldingDay,
) -> Result<crate::cycling::NextRide, NoHoldingRide> {
    let venue = choose(catalogue, record, day.role).await?;
    let session = catalogue.session_at(&venue).await?;
    let position = match day.role.intensity() {
        domain::schedule::Relative::Higher => 1,
        domain::schedule::Relative::Lower => 2,
    };
    let session_position =
        SessionPosition::new(position).map_err(|_| NoHoldingRide::Unbuildable {
            detail: "a session position counting from zero".to_owned(),
        })?;
    Ok(crate::cycling::NextRide {
        programme: day.programme,
        date: day.date,
        microcycle: day.week,
        session: session_position,
        ride: PlannedRide::assembled(session, NonEmpty::of(venue, Vec::new()), day.role),
    })
}
