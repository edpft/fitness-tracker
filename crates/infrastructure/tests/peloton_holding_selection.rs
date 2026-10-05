//! Which class fills a holding week's two roles, chosen from the catalogue.
//!
//! **This was an HTTP contract until #246 and is no longer one.** The selection
//! used to be a browse query per role — `series_id` and an exact `duration` —
//! and what needed pinning was that the query carried both, because a search
//! missing the series still answered 200 with *a* 45-minute Power Zone class
//! and it would silently have been the wrong format. The classes now come from
//! the local catalogue, so the query is gone and what the walk asks for is
//! pinned in `peloton_catalogue_contract` instead.
//!
//! What survives is the part that was never about HTTP:
//!
//! - **The series tells the two formats apart**, because the class type cannot:
//!   *Power Zone Endurance Ride*, *Power Zone Ride* and *Power Zone Max Ride*
//!   share one `class_type_id`, verified across the operator's 309 landed
//!   cycling workouts on 2026-09-20. A Max ride where an endurance ride was
//!   asked for is the failure this guards.
//! - **Newest first, and the newest not already ridden.** The operator, 2026-09-20:
//!   the holding rides are *"the newest that hasn't already been taken"*, and
//!   taken means ridden. A selection that forgot to sort would return a stable
//!   but arbitrary class and nothing would look wrong.
//! - **Never the same class for both roles.**
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use std::{collections::BTreeSet, error::Error};

use application::{HoldingRides as _, RiddenVenues, StoreError};
use domain::{
    cycling::RideVenue,
    schedule::{Relative, SessionRole},
};
use infrastructure::{
    SqlitePelotonClassStore, connect,
    peloton::{ClassSummary, PelotonHoldingRides, catalogue::ClassCatalogue},
};
use tempfile::TempDir;

type Fallible<T> = Result<T, Box<dyn Error>>;

/// The series ids Peloton really uses, read off the operator's own record.
const ENDURANCE_SERIES: &str = "0f63c48726fa4533a928cae5358d94d7";
const POWER_ZONE_SERIES: &str = "9fde039566054ea499130bed1c289eb3";
/// The series the *Max* rides sit in, which must never be taken.
const MAX_SERIES: &str = "5e02288cccac46bbbba2eb1acb41f059";
/// A themed series, which must not be taken either.
const EIGHTIES_SERIES: &str = "8a420d594a094a798f7cf7936f3e4b2d";
/// How long both holding rides are.
const HOLDING: u64 = 2_700;

const fn higher() -> SessionRole {
    SessionRole::new(Relative::Higher, Relative::Lower)
}

const fn lower() -> SessionRole {
    SessionRole::new(Relative::Lower, Relative::Higher)
}

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

async fn catalogue() -> Fallible<(TempDir, SqlitePelotonClassStore)> {
    let directory = TempDir::new()?;
    let pool = connect(&directory.path().join("fitness.db")).await?;
    Ok((directory, SqlitePelotonClassStore::new(pool)))
}

/// A class as the catalogue holds it. `aired_at` is what "newest" means.
fn listed(id: &str, title: &str, series: &str, duration: u64, aired_at: i64) -> ClassSummary {
    ClassSummary {
        id: id.to_owned(),
        title: title.to_owned(),
        duration_seconds: duration,
        series: Some(series.to_owned()),
        instructor: Some("an-instructor".to_owned()),
        aired_at: Some(aired_at),
    }
}

fn plain(id: &str, aired_at: i64) -> ClassSummary {
    listed(
        id,
        "45 min Power Zone Ride",
        POWER_ZONE_SERIES,
        HOLDING,
        aired_at,
    )
}

fn endurance(id: &str, aired_at: i64) -> ClassSummary {
    listed(
        id,
        "45 min Power Zone Endurance Ride",
        ENDURANCE_SERIES,
        HOLDING,
        aired_at,
    )
}

/// A fake record, so the choosing rule can be driven without the normalised
/// layer behind it.
struct Ridden(BTreeSet<RideVenue>);

impl RiddenVenues for Ridden {
    async fn ridden(&self) -> Result<BTreeSet<RideVenue>, StoreError> {
        Ok(self.0.clone())
    }
}

fn venue(id: &str, title: &str) -> Fallible<RideVenue> {
    Ok(RideVenue::new(id, title)?)
}

/// The lower-intensity role takes the endurance series.
#[test]
fn the_lower_intensity_role_offers_the_endurance_series() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[
                endurance("an-endurance-ride", 100),
                plain("a-plain-ride", 200),
            ])
            .await
            .expect("the listing records");

        let found = PelotonHoldingRides::new(ClassCatalogue::stored(&store))
            .candidates(lower())
            .await
            .expect("the catalogue answers");
        assert_eq!(
            found,
            vec![
                venue("an-endurance-ride", "45 min Power Zone Endurance Ride")
                    .expect("a class names itself")
            ],
            "the plain ride is the other role's"
        );
    });
}

/// **The higher-intensity role takes the plain series, never the Max one.** The
/// operator named *"a regular 45 minute power zone ride"* on 2026-09-20, and a
/// Max ride is a different series by Peloton's own reckoning.
#[test]
fn the_higher_intensity_role_offers_the_plain_power_zone_series() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[
                listed(
                    "a-max-ride",
                    "45 min Power Zone Max Ride",
                    MAX_SERIES,
                    HOLDING,
                    300,
                ),
                listed(
                    "a-themed-ride",
                    "45 min Power Zone 80s Ride",
                    EIGHTIES_SERIES,
                    HOLDING,
                    250,
                ),
                plain("the-plain-one", 100),
            ])
            .await
            .expect("the listing records");

        let found = PelotonHoldingRides::new(ClassCatalogue::stored(&store))
            .candidates(higher())
            .await
            .expect("the catalogue answers");
        assert_eq!(found.len(), 1, "the Max and themed rides are not offered");
        assert_eq!(found[0].reference(), "the-plain-one");
    });
}

/// **A class the listing gave no series for is never offered.** It is the
/// honest answer: what format it is has not been stated, and offering it for a
/// role defined by a series would be this build deciding for Peloton.
#[test]
fn a_class_with_no_series_is_not_offered_for_either_role() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        let mut silent = plain("says-no-series", 500);
        silent.series = None;
        store
            .record_listed(&[silent, plain("says-its-series", 100)])
            .await
            .expect("the listing records");

        let found = PelotonHoldingRides::new(ClassCatalogue::stored(&store))
            .candidates(higher())
            .await
            .expect("the catalogue answers");
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].reference(),
            "says-its-series",
            "and not the newer one, which stated no format"
        );
    });
}

/// A class of the right series but the wrong length is not a holding ride.
#[test]
fn a_class_of_another_length_is_not_offered() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[
                listed(
                    "a-thirty",
                    "30 min Power Zone Ride",
                    POWER_ZONE_SERIES,
                    1_800,
                    500,
                ),
                plain("a-forty-five", 100),
            ])
            .await
            .expect("the listing records");

        let found = PelotonHoldingRides::new(ClassCatalogue::stored(&store))
            .candidates(higher())
            .await
            .expect("the catalogue answers");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reference(), "a-forty-five");
    });
}

/// **The newest that has not been ridden**, which is the whole of the rule. The
/// first two are in the record, so the third is taken — and the assertion is
/// that it is the third rather than merely "not the first".
#[test]
fn the_newest_unridden_class_is_the_one_chosen() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[
                plain("ridden-last-week", 300),
                plain("ridden-in-june", 200),
                plain("never-ridden", 100),
            ])
            .await
            .expect("the listing records");

        let record = Ridden(
            [
                venue("ridden-last-week", "45 min Power Zone Ride").expect("a class names itself"),
                venue("ridden-in-june", "45 min Power Zone Ride").expect("a class names itself"),
            ]
            .into_iter()
            .collect(),
        );

        let chosen = application::holding::choose(
            &PelotonHoldingRides::new(ClassCatalogue::stored(&store)),
            &record,
            higher(),
        )
        .await
        .expect("a class remains");
        assert_eq!(chosen.reference(), "never-ridden");
    });
}

/// **A class ridden under his own steam is taken, not only one this tool
/// prescribed.** The operator settled on 2026-09-20 that "taken" means ridden,
/// which is the stronger of the two readings, and this is the case that tells
/// them apart: nothing here has ever been prescribed.
#[test]
fn every_class_having_been_ridden_is_a_refusal_that_says_how_far_it_looked() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[plain("one", 200), plain("two", 100)])
            .await
            .expect("the listing records");

        let record = Ridden(
            [
                venue("one", "45 min Power Zone Ride").expect("a class names itself"),
                venue("two", "45 min Power Zone Ride").expect("a class names itself"),
            ]
            .into_iter()
            .collect(),
        );

        let error = application::holding::choose(
            &PelotonHoldingRides::new(ClassCatalogue::stored(&store)),
            &record,
            higher(),
        )
        .await
        .expect_err("everything offered has been ridden");
        let said = error.to_string();
        assert!(
            said.contains("2 classes"),
            "the refusal says how far it looked: {said}"
        );
    });
}

/// **Both rides at once, and never the same class twice.** The series are
/// disjoint today so this cannot fire, which is exactly why it is pinned: a
/// week prescribing one class for both slots would be a week with one session
/// in it, and nothing downstream would say so.
///
/// A class carrying both series is not representable in the catalogue — a row
/// holds one — so the collision is staged the only way it can now happen: one
/// class is the sole candidate for the harder role and also the newest for the
/// easier one.
#[test]
fn one_class_is_never_chosen_for_both_roles() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[
                plain("the-harder-one", 300),
                endurance("the-endurance-one", 100),
            ])
            .await
            .expect("the listing records");

        let (harder, easier) = application::holding::both(
            &PelotonHoldingRides::new(ClassCatalogue::stored(&store)),
            &Ridden(BTreeSet::new()),
            higher(),
            lower(),
        )
        .await
        .expect("both roles find a class");
        assert_eq!(harder.reference(), "the-harder-one");
        assert_eq!(easier.reference(), "the-endurance-one");
        assert_ne!(harder, easier);
    });
}

/// **An unfilled catalogue offers nothing, and does not pretend to reach
/// Peloton.** This is what the first run after #246 looks like before the
/// refresh inside `fitness next` has listed anything.
#[test]
fn an_unfilled_catalogue_offers_no_candidates() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        let found = PelotonHoldingRides::new(ClassCatalogue::stored(&store))
            .candidates(higher())
            .await
            .expect("an empty catalogue is an answer, not a fault");
        assert!(found.is_empty());
    });
}

/// A role a holding week never asks for offers nothing, rather than inventing a
/// series for it.
#[test]
fn a_role_a_holding_week_does_not_ask_for_offers_nothing() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[
                plain("a-plain-ride", 100),
                endurance("an-endurance-ride", 200),
            ])
            .await
            .expect("the listing records");

        let rides = PelotonHoldingRides::new(ClassCatalogue::stored(&store));
        for role in [
            SessionRole::new(Relative::Higher, Relative::Higher),
            SessionRole::new(Relative::Lower, Relative::Lower),
        ] {
            let found = rides.candidates(role).await.expect("a role answers");
            assert!(found.is_empty(), "{role:?} is not a holding role");
        }
    });
}
