//! The contract with Peloton's class search, as a holding microcycle asks it.
//!
//! **What is pinned is the query and the filter, not the answer.** A holding
//! week's two rides are told apart by `series_id` and nothing else: *Power Zone
//! Endurance Ride*, *Power Zone Ride* and *Power Zone Max Ride* share one
//! `class_type_id`, verified across the operator's 309 landed cycling workouts
//! on 2026-09-20. A search that forgot the series still answers 200 with *a*
//! 45-minute Power Zone class, and it would silently be the wrong format — a
//! Max ride where an endurance ride was asked for.
//!
//! **So the filter is asserted twice**, once as a request parameter and once
//! against what came back. This adapter does not own the endpoint and cannot
//! promise the parameter is honoured; a server that ignored it would otherwise
//! hand back every format and the newest of the lot would be taken.
//!
//! **Newest first is load-bearing**, as it is for a cool-down: the operator
//! rides *"the newest that hasn't already been taken"*, so a search that forgot
//! to sort would return a stable but arbitrary class and nothing would look
//! wrong.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use application::{HoldingRides as _, RiddenVenues, StoreError};
use domain::{
    cycling::RideVenue,
    schedule::{Relative, SessionRole},
};
use infrastructure::peloton::{
    PelotonHoldingRides,
    auth::{PelotonAuth, PelotonCredentials},
    class::PelotonClasses,
    in_series_from,
};
use std::collections::BTreeSet;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

/// The series ids Peloton really uses, read off the operator's own record.
const ENDURANCE_SERIES: &str = "0f63c48726fa4533a928cae5358d94d7";
const POWER_ZONE_SERIES: &str = "9fde039566054ea499130bed1c289eb3";
/// The one class type all three Power Zone formats share.
const POWER_ZONE_TYPE: &str = "665395ff3abf4081bf315686227d1a51";
/// The series the *Max* rides sit in, which must never be taken.
const MAX_SERIES: &str = "5e02288cccac46bbbba2eb1acb41f059";

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

/// A token endpoint that answers, so the search can be reached.
async fn authenticated(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/authorize"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("set-cookie", "_csrf=csrf; Path=/")
                .insert_header("location", "/login?state=carried"),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>a login page</html>"))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/usernamepassword/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"<html><body><form method="post" action="{}/login/callback">
            <input type="hidden" name="wa" value="wsignin1.0" />
            <input type="hidden" name="wresult" value="signed" />
            <input type="hidden" name="wctx" value="{{&quot;tenant&quot;:&quot;peloton-prod&quot;}}" />
            </form></body></html>"#,
            server.uri()
        )))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/login/callback"))
        .respond_with(ResponseTemplate::new(302).insert_header(
            "location",
            "https://members.onepeloton.com/callback?code=the-code&state=carried",
        ))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "the-access-token",
            "token_type": "Bearer",
            "expires_in": 172_800,
        })))
        .mount(server)
        .await;
}

fn classes(server: &MockServer) -> PelotonClasses {
    PelotonClasses::new(
        server.uri(),
        PelotonAuth::new(
            server.uri(),
            PelotonCredentials::new("rider@example.com", "not-a-real-password"),
        ),
    )
}

/// **Every filter the selection turns on.** Drop the series and the search
/// still answers with a 45-minute Power Zone class of some format; drop the
/// sort and it answers an arbitrary one. `CLAUDE.md`'s warning about defaults
/// is why each is asserted rather than trusted from a happy-path read.
#[test]
fn the_search_asks_for_one_series_at_one_length_newest_first() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v2/ride/archived"))
            .and(query_param("browse_category", "cycling"))
            .and(query_param("duration", "2700"))
            .and(query_param("class_type_id", POWER_ZONE_TYPE))
            .and(query_param("series_id", ENDURANCE_SERIES))
            .and(query_param("sort_by", "original_air_time"))
            .and(query_param("desc", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "id": "newest-endurance",
                    "title": "45 min Power Zone Endurance Ride",
                    "duration": 2700,
                    "series_id": ENDURANCE_SERIES,
                }],
            })))
            .mount(&server)
            .await;

        let found = PelotonHoldingRides::new(&classes(&server))
            .candidates(lower())
            .await
            .expect("the stubbed search answers");
        assert_eq!(
            found,
            vec![
                RideVenue::new("newest-endurance", "45 min Power Zone Endurance Ride")
                    .expect("a class names itself")
            ],
        );
    });
}

/// **The higher-intensity role takes the plain series, never the Max one.** The
/// operator named *"a regular 45 minute power zone ride"* on 2026-09-20, and a
/// Max ride is a different series by Peloton's own reckoning.
#[test]
fn the_higher_intensity_role_asks_for_the_plain_power_zone_series() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v2/ride/archived"))
            .and(query_param("series_id", POWER_ZONE_SERIES))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "id": "newest-power-zone",
                    "title": "45 min Power Zone Ride",
                    "duration": 2700,
                    "series_id": POWER_ZONE_SERIES,
                }],
            })))
            .mount(&server)
            .await;

        let found = PelotonHoldingRides::new(&classes(&server))
            .candidates(higher())
            .await
            .expect("the stubbed search answers");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reference(), "newest-power-zone");
    });
}

/// **A server that ignores the filter must not change the answer.** This is the
/// failure the class type cannot catch: three formats, one type id, and the Max
/// ride listed first. Reading position rather than series would take it.
#[test]
fn a_listing_carrying_other_series_keeps_only_the_one_asked_for() {
    let mixed = serde_json::json!({
        "data": [
            { "id": "a-max-ride", "title": "45 min Power Zone Max Ride",
              "duration": 2700, "series_id": MAX_SERIES },
            { "id": "a-themed-ride", "title": "45 min Power Zone 80s Ride",
              "duration": 2700, "series_id": "8a420d594a094a798f7cf7936f3e4b2d" },
            { "id": "the-plain-one", "title": "45 min Power Zone Ride",
              "duration": 2700, "series_id": POWER_ZONE_SERIES },
        ]
    })
    .to_string();
    let found = in_series_from(POWER_ZONE_SERIES, &mixed).expect("the listing reads");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, "the-plain-one");
}

/// **A listing that states no series at all is a refusal, not an empty answer.**
/// The endpoint is not ours; if it stops serving the field this selection turns
/// on, saying "no class matched" would be a lie that authors nothing and
/// explains nothing.
#[test]
fn a_listing_with_no_series_on_it_is_refused_rather_than_read_as_empty() {
    let silent = serde_json::json!({
        "data": [
            { "id": "one", "title": "45 min Power Zone Ride", "duration": 2700 },
            { "id": "two", "title": "45 min Power Zone Ride", "duration": 2700 },
        ]
    })
    .to_string();
    let error = in_series_from(POWER_ZONE_SERIES, &silent)
        .expect_err("a listing with no series is refused");
    let said = error.to_string();
    assert!(
        said.contains("none of them stated a series"),
        "the refusal says why: {said}"
    );
}

/// An empty catalogue is an answer, not a fault — as an instructor with no
/// cool-down is.
#[test]
fn an_empty_listing_is_an_empty_answer() {
    let empty = serde_json::json!({ "data": [] }).to_string();
    let found = in_series_from(POWER_ZONE_SERIES, &empty).expect("an empty list reads");
    assert!(found.is_empty());
}

/// A fake record, so the choosing rule can be driven without a store.
struct Ridden(BTreeSet<RideVenue>);

impl RiddenVenues for Ridden {
    async fn ridden(&self) -> Result<BTreeSet<RideVenue>, StoreError> {
        Ok(self.0.clone())
    }
}

/// **The newest that has not been ridden**, which is the whole of the rule.
/// The first two are in the record, so the third is taken — and the assertion
/// is that it is the third rather than merely "not the first".
#[test]
fn the_newest_unridden_class_is_the_one_chosen() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v2/ride/archived"))
            .and(query_param("series_id", POWER_ZONE_SERIES))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "id": "ridden-last-week", "title": "45 min Power Zone Ride",
                      "duration": 2700, "series_id": POWER_ZONE_SERIES },
                    { "id": "ridden-in-june", "title": "45 min Power Zone Ride",
                      "duration": 2700, "series_id": POWER_ZONE_SERIES },
                    { "id": "never-ridden", "title": "45 min Power Zone Ride",
                      "duration": 2700, "series_id": POWER_ZONE_SERIES },
                ]
            })))
            .mount(&server)
            .await;

        let record = Ridden(
            [
                RideVenue::new("ridden-last-week", "45 min Power Zone Ride")
                    .expect("a class names itself"),
                RideVenue::new("ridden-in-june", "45 min Power Zone Ride")
                    .expect("a class names itself"),
            ]
            .into_iter()
            .collect(),
        );

        let chosen = application::holding::choose(
            &PelotonHoldingRides::new(&classes(&server)),
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
        let server = MockServer::start().await;
        authenticated(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v2/ride/archived"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "id": "one", "title": "45 min Power Zone Ride",
                      "duration": 2700, "series_id": POWER_ZONE_SERIES },
                    { "id": "two", "title": "45 min Power Zone Ride",
                      "duration": 2700, "series_id": POWER_ZONE_SERIES },
                ]
            })))
            .mount(&server)
            .await;

        let record = Ridden(
            [
                RideVenue::new("one", "45 min Power Zone Ride").expect("a class names itself"),
                RideVenue::new("two", "45 min Power Zone Ride").expect("a class names itself"),
            ]
            .into_iter()
            .collect(),
        );

        let error = application::holding::choose(
            &PelotonHoldingRides::new(&classes(&server)),
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
#[test]
fn one_class_is_never_chosen_for_both_roles() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;

        // A server that answers the same two classes whatever series is asked
        // for — the collision the disjoint series are supposed to prevent.
        Mock::given(method("GET"))
            .and(path("/api/v2/ride/archived"))
            .and(query_param("series_id", POWER_ZONE_SERIES))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "id": "shared", "title": "45 min Power Zone Ride",
                      "duration": 2700, "series_id": POWER_ZONE_SERIES },
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v2/ride/archived"))
            .and(query_param("series_id", ENDURANCE_SERIES))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "id": "shared", "title": "45 min Power Zone Ride",
                      "duration": 2700, "series_id": ENDURANCE_SERIES },
                    { "id": "the-endurance-one", "title": "45 min Power Zone Endurance Ride",
                      "duration": 2700, "series_id": ENDURANCE_SERIES },
                ]
            })))
            .mount(&server)
            .await;

        let (harder, easier) = application::holding::both(
            &PelotonHoldingRides::new(&classes(&server)),
            &Ridden(BTreeSet::new()),
            higher(),
            lower(),
        )
        .await
        .expect("both roles find a class");
        assert_eq!(harder.reference(), "shared");
        assert_eq!(easier.reference(), "the-endurance-one");
        assert_ne!(harder, easier);
    });
}
