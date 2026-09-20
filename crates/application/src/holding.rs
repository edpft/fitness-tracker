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

use std::collections::BTreeSet;

use domain::{cycling::RideVenue, schedule::SessionRole};

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
    let first = choose(catalogue, record, higher).await?;
    // The one already chosen is not available to the other role, whatever the
    // catalogue says. Cheaper than a second rule about series being disjoint,
    // and it holds if they ever stop being.
    let taken: BTreeSet<RideVenue> = std::iter::once(first.clone()).collect();
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
