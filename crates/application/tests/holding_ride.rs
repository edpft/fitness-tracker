//! One ride of a holding week, chosen when it is delivered (#190).
//!
//! The choosing rule — the newest class of the role not yet ridden — is pinned
//! against Peloton's search in `peloton_holding_contract`. What is pinned here is
//! what the day adds to it: which session of the week the ride is, and that the
//! session is the chosen class's own.

use std::collections::BTreeSet;

use application::{
    HoldingRides, RiddenVenues, SourceError, StoreError, cycling::HoldingDay, holding,
};
use domain::{
    cycling::{CyclingMesocycleId, CyclingSession, Interval, PowerZone, Ride, RideVenue},
    measure::PositiveDuration,
    schedule::{Relative, SessionRole},
    sequence::NonEmpty,
};
use jiff::civil::Date;

/// A catalogue offering one class per role, each a 45-minute ride in its zone.
struct Catalogue;

impl HoldingRides for Catalogue {
    async fn candidates(&self, role: SessionRole) -> Result<Vec<RideVenue>, SourceError> {
        let venue = match role.intensity() {
            Relative::Higher => RideVenue::new("pz", "45 min Power Zone Ride"),
            Relative::Lower => RideVenue::new("pze", "45 min Power Zone Endurance Ride"),
        };
        Ok(venue.into_iter().collect())
    }

    async fn session_at(&self, venue: &RideVenue) -> Result<CyclingSession, SourceError> {
        let zone = if venue.reference() == "pz" {
            PowerZone::Four
        } else {
            PowerZone::Two
        };
        let (Ok(five), Ok(thirty_five)) = (
            PositiveDuration::from_seconds(300),
            PositiveDuration::from_seconds(2100),
        ) else {
            return Err(SourceError::Malformed {
                detail: "a positive duration".to_owned(),
            });
        };
        Ok(CyclingSession::new(
            five,
            Ride::Intervals(NonEmpty::of(Interval::new(zone, thirty_five), Vec::new())),
            Some(five),
        ))
    }
}

/// Nothing ridden.
struct Fresh;

impl RiddenVenues for Fresh {
    async fn ridden(&self) -> Result<BTreeSet<RideVenue>, StoreError> {
        Ok(BTreeSet::new())
    }
}

const fn day(role: SessionRole) -> HoldingDay {
    HoldingDay {
        programme: CyclingMesocycleId::new(1),
        date: Date::constant(2026, 9, 30),
        week: 1,
        role,
    }
}

/// **The harder ride is the week's first session**, as a holding microcycle
/// authored whole numbers it, and it carries the class's own zone plan.
#[test]
fn the_harder_holding_ride_is_the_first_session_and_its_classes_own() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a current-thread runtime builds");
    let ride = runtime
        .block_on(holding::ride(
            &Catalogue,
            &Fresh,
            day(SessionRole::new(Relative::Higher, Relative::Lower)),
        ))
        .expect("a class is offered and none ridden");

    assert_eq!(ride.session.as_u8(), 1);
    assert_eq!(ride.microcycle, 1);
    assert_eq!(ride.date, Date::constant(2026, 9, 30));
    assert_eq!(ride.ride.at().first().reference(), "pz");
    assert_eq!(
        ride.ride.session().ride().peak_zone(),
        Some(PowerZone::Four)
    );
    assert_eq!(
        ride.ride.role(),
        SessionRole::new(Relative::Higher, Relative::Lower)
    );
}

/// **The easier ride is the second**, and an illness that eased the day asks
/// for exactly this one.
#[test]
fn the_easier_holding_ride_is_the_second_session() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a current-thread runtime builds");
    let ride = runtime
        .block_on(holding::ride(
            &Catalogue,
            &Fresh,
            day(SessionRole::new(Relative::Lower, Relative::Higher)),
        ))
        .expect("a class is offered and none ridden");

    assert_eq!(ride.session.as_u8(), 2);
    assert_eq!(ride.ride.at().first().reference(), "pze");
}
