//! The contract with Peloton's class browse, as the catalogue walk asks it.
//!
//! **What is pinned is the walk, not the answer.** The catalogue holds every
//! power zone class Peloton serves — the operator, 2026-10-05: *"I don't just
//! want the classes I've already ridden, I want all power classes"* — so the
//! query must name the class type and must *not* name a series or a duration,
//! which is exactly the opposite of what `peloton_holding_contract` pins about
//! the selection search. A walk that inherited the holding query's `duration`
//! would answer 200 with a plausible page of 45-minute classes and the
//! catalogue would quietly hold a slice of the library.
//!
//! **And the walk must end where the source says it ends.** `show_next` is the
//! source's own statement that another page follows; a walk that stopped at the
//! first page would hold a hundred classes and look like a full catalogue.
//!
//! **A stub cannot catch a wrong default** (`CLAUDE.md`), so the composed query
//! is asserted parameter by parameter rather than read off a happy path.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use infrastructure::peloton::{
    auth::{PelotonAuth, PelotonCredentials},
    class::PelotonClasses,
    power_zone_from,
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param, query_param_is_missing},
};

/// The one class type all three Power Zone formats share.
const POWER_ZONE_TYPE: &str = "665395ff3abf4081bf315686227d1a51";
/// Two of the series that sit inside it, read off the operator's own record.
const ENDURANCE_SERIES: &str = "0f63c48726fa4533a928cae5358d94d7";
const MAX_SERIES: &str = "5e02288cccac46bbbba2eb1acb41f059";

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

/// A token endpoint that answers, so the browse can be reached.
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

fn listed(id: &str, series: &str, duration: u64) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "title": format!("{} min Power Zone Ride", duration / 60),
        "duration": duration,
        "series_id": series,
        "class_type_ids": [POWER_ZONE_TYPE],
        "instructor_id": "an-instructor",
        "original_air_time": 1_700_000_000_i64,
    })
}

/// **Every parameter the walk turns on, and the two it must not send.** With a
/// `duration` the catalogue holds one length; with a `series_id` it holds one
/// format. Either would answer 200 and look entirely healthy.
#[test]
fn the_walk_asks_for_every_power_zone_class_whatever_its_series_or_length() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v2/ride/archived"))
            .and(query_param("browse_category", "cycling"))
            .and(query_param("class_type_id", POWER_ZONE_TYPE))
            .and(query_param("sort_by", "original_air_time"))
            .and(query_param("desc", "true"))
            .and(query_param("page", "0"))
            .and(query_param_is_missing("duration"))
            .and(query_param_is_missing("series_id"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    listed("endurance", ENDURANCE_SERIES, 2_700),
                    listed("max", MAX_SERIES, 1_800),
                ],
                "show_next": false,
            })))
            .mount(&server)
            .await;

        let found = classes(&server)
            .power_zone_page(0)
            .await
            .expect("the browse answers");
        assert!(!found.more, "the source said there is no further page");
        let held: Vec<&str> = found.classes.iter().map(|one| one.id.as_str()).collect();
        assert_eq!(
            held,
            vec!["endurance", "max"],
            "every format is held, not just the two a holding week picks between"
        );
    });
}

/// The walk's own stopping rule, and the thing a single-page walk gets wrong.
#[test]
fn a_page_that_says_more_follows_is_not_the_end_of_the_catalogue() {
    let first = serde_json::json!({
        "data": [listed("a", ENDURANCE_SERIES, 2_700)],
        "show_next": true,
    })
    .to_string();
    let last = serde_json::json!({
        "data": [listed("b", ENDURANCE_SERIES, 2_700)],
        "show_next": false,
    })
    .to_string();

    let more = power_zone_from(&first).expect("a page reads");
    assert!(more.more, "show_next true means another page follows");
    let end = power_zone_from(&last).expect("a page reads");
    assert!(!end.more, "show_next false ends the walk");
}

/// A short page with no `show_next` ends the walk: the source did not fill the
/// limit it was given, so there was nothing more to give.
#[test]
fn a_short_page_without_show_next_ends_the_walk() {
    let body = serde_json::json!({ "data": [listed("a", ENDURANCE_SERIES, 2_700)] }).to_string();
    let page = power_zone_from(&body).expect("a page reads");
    assert!(!page.more);
    assert_eq!(page.classes.len(), 1);
}

/// **A full page continues the walk even with no `show_next`.** This is the
/// wrong default a stub cannot catch: this adapter has never seen
/// `/api/v2/ride/archived` answer, and if it serves no `show_next` then
/// trusting that field alone would make a hundred classes look like the whole
/// library. A page that filled its limit says otherwise.
#[test]
fn a_full_page_without_show_next_continues_the_walk() {
    let full: Vec<serde_json::Value> = (0..100)
        .map(|n| listed(&format!("class-{n}"), ENDURANCE_SERIES, 2_700))
        .collect();
    let body = serde_json::json!({ "data": full }).to_string();
    let page = power_zone_from(&body).expect("a page reads");
    assert!(
        page.more,
        "a page that filled the limit it was given is not the end of the library"
    );
}

/// **And the count is taken before filtering.** A full page of which only three
/// are power zone classes still means the endpoint had more to give; counting
/// the survivors would end the walk on the first heavily-filtered page.
#[test]
fn a_full_page_mostly_filtered_away_still_continues_the_walk() {
    let mut data: Vec<serde_json::Value> = (0..97)
        .map(|n| {
            serde_json::json!({
                "id": format!("other-{n}"),
                "title": "a class of another kind",
                "duration": 1_800,
                "class_type_ids": ["something-else"],
            })
        })
        .collect();
    for n in 0..3 {
        data.push(listed(&format!("power-{n}"), ENDURANCE_SERIES, 2_700));
    }
    let body = serde_json::json!({ "data": data }).to_string();
    let page = power_zone_from(&body).expect("a page reads");
    assert_eq!(
        page.classes.len(),
        3,
        "only the power zone classes are held"
    );
    assert!(page.more, "but the endpoint still had more to give");
}

/// The listing carries what the catalogue sorts and filters on, and a page that
/// dropped any of it would store a class that could never be chosen.
#[test]
fn a_listed_class_carries_its_series_its_length_and_its_age() {
    let body = serde_json::json!({
        "data": [listed("a", ENDURANCE_SERIES, 2_700)],
        "show_next": false,
    })
    .to_string();
    let page = power_zone_from(&body).expect("a page reads");
    let one = page.classes.first().expect("one class");
    assert_eq!(one.series.as_deref(), Some(ENDURANCE_SERIES));
    assert_eq!(one.duration_seconds, 2_700);
    assert_eq!(one.aired_at, Some(1_700_000_000));
    assert_eq!(one.instructor.as_deref(), Some("an-instructor"));
}

/// **A class of another type is dropped, and one that states no type is kept.**
/// The query asked for one type, so a listing that omits the field has not
/// contradicted it; a listing that states a different one has.
#[test]
fn a_class_of_another_type_is_not_held_and_a_silent_one_is() {
    let body = serde_json::json!({
        "data": [
            { "id": "power", "title": "a", "duration": 1_800, "class_type_ids": [POWER_ZONE_TYPE] },
            { "id": "other", "title": "b", "duration": 1_800, "class_type_ids": ["something-else"] },
            { "id": "silent", "title": "c", "duration": 1_800 },
        ],
        "show_next": false,
    })
    .to_string();
    let page = power_zone_from(&body).expect("a page reads");
    let held: Vec<&str> = page.classes.iter().map(|one| one.id.as_str()).collect();
    assert_eq!(held, vec!["power", "silent"]);
}

/// **A class of no length, or no name, is dropped rather than failing the
/// page.** Migration 0066 already ruled that an activity of no duration is not
/// an activity; the catalogue's job is to hold what can be ridden, and a page
/// refused over one unrideable entry would hold nothing.
#[test]
fn a_class_of_no_length_or_no_name_is_not_held() {
    let body = serde_json::json!({
        "data": [
            { "id": "fine", "title": "45 min Power Zone Ride", "duration": 2_700 },
            { "id": "no-length", "title": "45 min Power Zone Ride", "duration": 0 },
            { "id": "no-name", "title": "   ", "duration": 2_700 },
        ],
        "show_next": false,
    })
    .to_string();
    let page = power_zone_from(&body).expect("a page reads");
    let held: Vec<&str> = page.classes.iter().map(|one| one.id.as_str()).collect();
    assert_eq!(held, vec!["fine"]);
}

/// A body that is not a listing is a source that has changed, and it says so
/// rather than reporting an empty catalogue.
#[test]
fn a_body_that_is_not_a_listing_is_refused() {
    let refused = power_zone_from("<html>not json</html>");
    assert!(refused.is_err(), "an unreadable page is not an empty one");
}
