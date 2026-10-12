//! The cycling use cases, over this build's stores and catalogue.
//!
//! **The catalogue is read from the store, and falls through to Peloton only
//! where there is a login.** So a ride is prescribed without one, and what an
//! absent credential costs is the delivery and nothing else (§ 36).

use application::{
    DiaryStore as _, PublishedCycling, PublishedProgrammes as _, PublishedRead, RidesPlaced,
    SourceError,
    cycling::{PrescribedRide, RideDeliveryError, RidingError},
    holding::NoHoldingRide,
};
use domain::{
    cycling::{CyclingMesocycle, Ftp},
    normalised::OperatorZone,
    schedule::SessionRole,
};
use infrastructure::{
    SqlitePelotonClassStore,
    peloton::{
        PelotonHoldingRides, PelotonRideDestination, catalogue::ClassCatalogue,
        provider::PelotonProgrammes,
    },
};
use jiff::civil::Date;

use crate::{access::Peloton, next::NextFailure, rescheduling, store::Store};

/// The ride at or after a date, as the plan now stands.
///
/// The outer error is the store's or this build's; the inner one is the plan's
/// answer, which the operator is told in terms of the date asked about.
///
/// # Errors
///
/// [`NextFailure`] if the store cannot be read or the catalogue names no
/// destination for cycling.
pub async fn prescribed(
    store: &Store,
    zone: &OperatorZone,
    from: Date,
    ftp: Option<Ftp>,
    peloton: Option<&Peloton>,
) -> Result<Result<PrescribedRide, RidingError>, NextFailure> {
    let stored = SqlitePelotonClassStore::new(store.pool().clone());
    let catalogue = ClassCatalogue::over(&stored, peloton.map(|peloton| &peloton.classes));
    let riding = crate::next::riding(store.pool(), zone, catalogue)?;
    let reschedule = rescheduling::reschedule(store, zone).await?;
    let diary = store.diary().diary().await?;
    Ok(riding.next(from, reschedule, &diary, ftp).await)
}

/// Put a ride in Peloton's stack.
///
/// # Errors
///
/// [`NextFailure`] if the catalogue names no destination for cycling.
pub async fn deliver(
    store: &Store,
    zone: &OperatorZone,
    ride: &PrescribedRide,
    peloton: &Peloton,
    replace: bool,
) -> Result<Result<RidesPlaced, RideDeliveryError>, NextFailure> {
    let stored = SqlitePelotonClassStore::new(store.pool().clone());
    let riding = crate::next::riding(
        store.pool(),
        zone,
        ClassCatalogue::new(&stored, &peloton.classes),
    )?;
    Ok(riding
        .deliver(
            ride,
            &PelotonRideDestination::new(&peloton.classes, &peloton.stack),
            replace,
        )
        .await)
}

/// A week of the newest classes of each role the operator has not ridden.
///
/// # Errors
///
/// [`NoHoldingRide`] if Peloton cannot be read or every candidate has been
/// ridden.
pub async fn holding(
    store: &Store,
    peloton: &Peloton,
    start: Date,
    higher: SessionRole,
    lower: SessionRole,
) -> Result<CyclingMesocycle, NoHoldingRide> {
    let stored = SqlitePelotonClassStore::new(store.pool().clone());
    application::holding::mesocycle(
        &PelotonHoldingRides::new(ClassCatalogue::new(&stored, &peloton.classes)),
        &store.ridden_venues(),
        start,
        higher,
        lower,
    )
    .await
}

/// How many classes the local catalogue lists.
///
/// # Errors
///
/// [`SourceError`] if the catalogue cannot be read.
pub async fn listed(store: &Store) -> Result<u64, SourceError> {
    let stored = SqlitePelotonClassStore::new(store.pool().clone());
    Ok(ClassCatalogue::stored(&stored).held().await?.listed)
}

/// Read one published programme: every class it places, and what it trains.
///
/// **From the local catalogue first**, falling through to Peloton for a class
/// it has not read only where there is a login.
///
/// # Errors
///
/// [`SourceError`] from the first class that cannot be read.
pub async fn published(
    store: &Store,
    peloton: Option<&Peloton>,
    which: PublishedCycling,
) -> Result<PublishedRead, SourceError> {
    let stored = SqlitePelotonClassStore::new(store.pool().clone());
    let catalogue = ClassCatalogue::over(&stored, peloton.map(|peloton| &peloton.classes));
    PelotonProgrammes::new(catalogue).read(which).await
}
