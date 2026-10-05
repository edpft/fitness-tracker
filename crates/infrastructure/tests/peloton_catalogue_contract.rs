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
//! **And the walk's other half: reading every listed class's detail** (#369).
//! The reads were bounded at fifty a run, which left 1,002 of the operator's
//! 1,052 classes with no zone structure three weeks after the catalogue was
//! built — the operator, 2026-10-05: *"What made you think I'd only want to
//! read 50 of the available classes?"* So the read runs to exhaustion, a
//! quarter of a second apart, and the only things that end it early are the
//! source having no detail for one class or the source stopping altogether.
//!
//! Telling those two apart is what makes an unbounded read safe, and it is
//! pinned here: a class there is no detail for is recorded and never asked
//! again, while an outage keeps what was read and leaves the rest for the next
//! run. Conflate them and either those classes cost a request on every
//! `fitness next` for ever, or one outage writes off the whole library — which
//! a 403 would do, since #368 records that Peloton answers 403 for throttling
//! as well as for refusal.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use std::error::Error;

use infrastructure::{
    SqlitePelotonClassStore, connect,
    peloton::{
        ClassSummary,
        auth::{PelotonAuth, PelotonCredentials},
        class::PelotonClasses,
        power_zone_from, read_details, walk,
    },
};
use tempfile::TempDir;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, path_regex, query_param, query_param_is_missing},
};

type Fallible<T> = Result<T, Box<dyn Error>>;

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
            None,
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

/// **The distinction an unbounded read turns on** (#369). Reading the whole
/// library in one go means the two have to be told apart: a class there is no
/// detail for is recorded as such and never asked for again, while a source
/// that has stopped answering ends the reading and the next run resumes.
/// Conflate them and either those classes cost a request on every `fitness
/// next`, or one outage writes off a thousand classes.
///
/// **403 is on the outage side, and that is the load-bearing line here.** #368
/// records that Peloton answers 403 for Auth0 anomaly detection and for too
/// many logins from one address — neither of which says anything about what
/// this account may see, and both of which are what a burst of a thousand
/// requests provokes. Reading it as "no such class" would write off most of
/// the library in one run, silently and permanently. 429 goes the same way.
#[test]
fn only_a_class_that_is_not_there_is_an_answer_and_a_403_is_an_outage() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;

        for (id, status) in [("missing", 404), ("gone", 410)] {
            Mock::given(method("GET"))
                .and(path(format!("/api/ride/{id}/details")))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
        }
        for (id, status) in [("forbidden", 403), ("throttled", 429), ("failing", 503)] {
            Mock::given(method("GET"))
                .and(path(format!("/api/ride/{id}/details")))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path("/api/ride/served/details"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"ride":{}}"#))
            .mount(&server)
            .await;

        let classes = classes(&server);
        for id in ["missing", "gone"] {
            assert_eq!(
                classes
                    .detail_if_served(id)
                    .await
                    .expect("a class that is not there is not a failure"),
                None,
                "{id}: there is no such class to serve, and that is an answer"
            );
        }
        for id in ["forbidden", "throttled", "failing"] {
            assert!(
                classes.detail_if_served(id).await.is_err(),
                "{id}: a source that is refusing or throttling is not a thousand \
                 classes that do not exist"
            );
        }
        assert!(
            classes
                .detail_if_served("served")
                .await
                .expect("a served body answers")
                .is_some()
        );
    });
}

/// **A body the reader cannot make a session of is still named and still
/// stored**, because keeping the response is what makes a corrected reader cost
/// a re-read of the store rather than a re-fetch of the library. The catalogue
/// needs a title and a length to hold a class the browse listing never carried.
#[test]
fn a_detail_names_itself_even_where_it_will_not_derive() {
    let body = serde_json::json!({
        "ride": { "title": "45 min Power Zone Ride" },
        "segments": { "segment_list": [
            { "name": "Warm Up", "length": 300 },
            { "name": "Power Zone", "length": 2_100 },
            { "name": "Cool Down", "length": 300 },
        ]},
        "target_metrics_data": { "target_metrics": [
            { "offsets": { "start": 300, "end": 2_399 }, "metrics": [{ "lower": 9 }] },
        ]},
    })
    .to_string();
    assert!(
        infrastructure::peloton::class::derive("a", &body).is_err(),
        "zone 9 is not a zone this build knows, so there is no session to derive"
    );
    assert_eq!(
        infrastructure::peloton::class::named(&body),
        Some(("45 min Power Zone Ride".to_owned(), 2_700)),
        "but it names itself and states its length"
    );
}

/// And a body that names nothing is `None`, so the caller falls back rather
/// than writing a row that claims a title it does not have.
#[test]
fn a_detail_that_names_nothing_is_not_named() {
    assert_eq!(infrastructure::peloton::class::named("not json"), None);
    assert_eq!(
        infrastructure::peloton::class::named(r#"{"ride":{"title":"   "}}"#),
        None,
        "a blank title names nothing"
    );
    assert_eq!(
        infrastructure::peloton::class::named(r#"{"ride":{"title":"A Ride"}}"#),
        None,
        "and a class of no stated length has no length"
    );
}

// ---------------------------------------------------------------------------
// Reading the details the walk listed (#369).
// ---------------------------------------------------------------------------

async fn catalogue() -> Fallible<(TempDir, SqlitePelotonClassStore)> {
    let directory = TempDir::new()?;
    let pool = connect(&directory.path().join("fitness.db")).await?;
    Ok((directory, SqlitePelotonClassStore::new(pool)))
}

fn summary(id: &str, aired_at: i64) -> ClassSummary {
    ClassSummary {
        id: id.to_owned(),
        title: "45 min Power Zone Ride".to_owned(),
        duration_seconds: 2_700,
        series: Some(ENDURANCE_SERIES.to_owned()),
        instructor: Some("an-instructor".to_owned()),
        aired_at: Some(aired_at),
    }
}

/// A detail the reader can make a session of.
fn served_detail() -> serde_json::Value {
    serde_json::json!({
        "ride": {
            "title": "45 min Power Zone Ride",
            "instructor": { "id": "an-instructor", "name": "A Teacher" },
        },
        "segments": { "segment_list": [
            { "name": "Warm Up", "length": 300 },
            { "name": "Power Zone", "length": 600 },
            { "name": "Cool Down", "length": 120 },
        ]},
        "target_metrics_data": { "target_metrics": [
            { "offsets": { "start": 300, "end": 899 }, "metrics": [{ "lower": 3 }] },
        ]},
    })
}

/// **Every other class detail answers.** The read begins with the forty-odd
/// classes the published skeletons place, whether or not the walk listed them,
/// so a suite that mocked only its own classes would see those forty-odd come
/// back 404 and be recorded as not served. Mounted at a lower priority than
/// the
/// per-class mocks, which is what lets one of them answer differently.
async fn every_other_class_answers(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/ride/[^/]+/details$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(served_detail()))
        .with_priority(10)
        .mount(server)
        .await;
}

async fn one_class_answers(server: &MockServer, id: &str, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(format!("/api/ride/{id}/details")))
        .respond_with(response)
        .mount(server)
        .await;
}

/// **Every unread class, in one run.** Fifty a run was invisible on three
/// classes, so what this pins is the state the operator's catalogue never
/// reached: nothing outstanding after one refresh, and nothing asked for on the
/// refresh after that.
#[test]
fn one_read_takes_every_class_the_catalogue_has_not_read() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;
        every_other_class_answers(&server).await;

        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[
                summary("newest", 300),
                summary("middle", 200),
                summary("oldest", 100),
            ])
            .await
            .expect("the listing records");

        let read = read_details(&classes(&server), &store)
            .await
            .expect("the reading answers");
        assert_eq!(read.not_served, 0);
        assert!(read.stopped.is_none());
        assert_eq!(read.held.outstanding(), 0, "the count reaches all read");
        for id in ["newest", "middle", "oldest"] {
            assert!(
                store.class(id).await.expect("a read answers").is_some(),
                "{id} was read"
            );
        }

        let again = read_details(&classes(&server), &store)
            .await
            .expect("the reading answers");
        assert_eq!(again.read, 0, "a converged catalogue asks for nothing");
        assert_eq!(again.not_served, 0);
        assert_eq!(again.held.outstanding(), 0, "and stays read");
    });
}

/// **A class Peloton has no detail for leaves the outstanding set.**
/// Unbounded, leaving it unread would mean one request for it on every single
/// `fitness next` and a count that never reached `all read`. `mapping` already
/// records a candidate: a *Peak Your Power Zones* ride the account cannot
/// start.
#[test]
fn a_class_peloton_has_no_detail_for_is_recorded_and_not_asked_again() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;
        one_class_answers(&server, "unserved", ResponseTemplate::new(404)).await;
        every_other_class_answers(&server).await;

        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[summary("served", 200), summary("unserved", 100)])
            .await
            .expect("the listing records");

        let read = read_details(&classes(&server), &store)
            .await
            .expect("a class that is not there is not a failed refresh");
        assert_eq!(read.not_served, 1);
        assert!(read.stopped.is_none(), "one class is not an outage");
        assert_eq!(read.held.outstanding(), 0, "nothing is left to ask");

        let again = read_details(&classes(&server), &store)
            .await
            .expect("the reading answers");
        assert_eq!(
            (again.read, again.not_served),
            (0, 0),
            "it was recorded, so this run asked about nothing"
        );
    });
}

/// **A source that has stopped answering ends the reading and keeps what it
/// read.** A thousand requests against a failing source is not politeness, and
/// discarding what was already read would make every outage cost the library
/// again. The newest are read first, so what survives an interruption is what
/// the operator is most likely to be prescribed.
#[test]
fn an_outage_stops_the_reading_and_keeps_what_was_read() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;
        one_class_answers(&server, "middle", ResponseTemplate::new(503)).await;
        every_other_class_answers(&server).await;

        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[
                summary("newest", 300),
                summary("middle", 200),
                summary("oldest", 100),
            ])
            .await
            .expect("the listing records");

        let read = read_details(&classes(&server), &store)
            .await
            .expect("a partial read is not a failed refresh");
        assert!(read.stopped.is_some(), "and it says why there are more");
        assert!(
            store
                .class("newest")
                .await
                .expect("a read answers")
                .is_some(),
            "the newest was read before the source stopped"
        );
        assert!(
            store
                .class("oldest")
                .await
                .expect("a read answers")
                .is_none(),
            "and the reading stopped rather than carrying on through the failure"
        );
        assert_eq!(
            read.held.outstanding(),
            2,
            "the two unread are still outstanding, for the next run"
        );
    });
}

/// **The walk lists and does not read, which is what there is to announce.**
/// The caller prints how long the reading will take before it starts, and this
/// is the number it reads off.
#[test]
fn the_walk_leaves_every_class_it_listed_outstanding() {
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
                    listed("a", ENDURANCE_SERIES, 2_700),
                    listed("b", ENDURANCE_SERIES, 2_700),
                ],
                "show_next": false,
            })))
            .mount(&server)
            .await;

        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        let walked = walk(&classes(&server), &store)
            .await
            .expect("the walk answers");
        assert_eq!(walked.listed, 2);
        assert_eq!(walked.added, 2);
        assert_eq!(
            walked.held.outstanding(),
            2,
            "listing a class does not read it"
        );
        assert!(
            !walked.reading_takes().is_zero(),
            "so the caller has something to announce"
        );
    });
}
