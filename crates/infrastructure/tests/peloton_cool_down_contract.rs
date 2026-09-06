//! The contract with Peloton's class search, pinned against a local stub.
//!
//! **What is pinned is the query, not the answer.** The operator resolves a
//! cool-down by hand with four filters in the app — cycling, five minutes, class
//! type *Cool Down Ride*, that instructor — and this is those filters as a
//! request. Get one wrong and the search still answers 200 with *a* class, which
//! would be silently the wrong ride: a 10-minute cool-down, a running one, or
//! somebody else's. So every parameter is asserted, and `CLAUDE.md`'s warning
//! about defaults is the reason it is asserted here rather than trusted from a
//! happy-path read.
//!
//! **Newest first is load-bearing.** What he rides is the *most recent* one, so
//! a search that forgot to sort would return a stable but arbitrary class and
//! nothing would look wrong.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use infrastructure::peloton::{
    ClassSummary,
    auth::{PelotonAuth, PelotonCredentials},
    class::PelotonClasses,
    cool_down_from,
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

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

/// Matt Wilpers' id, and the class the live API really answers with.
const INSTRUCTOR: &str = "304389e2bfe44830854e071bffc137c9";
const COOL_DOWN: &str = "df190d7077c244208bdf6f9e0dae12f5";

fn one_result() -> serde_json::Value {
    serde_json::json!({
        "data": [{
            "id": COOL_DOWN,
            "title": "5 min Cool Down Ride",
            "duration": 300,
            "instructor_id": INSTRUCTOR,
        }],
        "total": 42,
    })
}

/// **All four of the operator's filters, and the ordering.** A search missing
/// any one of them still answers with a class, and it would be the wrong one.
#[test]
fn the_search_asks_for_the_operators_own_four_filters() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;

        Mock::given(method("GET"))
            .and(path("/api/v2/ride/archived"))
            .and(query_param("browse_category", "cycling"))
            .and(query_param("duration", "300"))
            .and(query_param(
                "class_type_id",
                "a1fa617f3ba14c0a8c25468d5c88b3ea",
            ))
            .and(query_param("instructor_id", INSTRUCTOR))
            .and(query_param("sort_by", "original_air_time"))
            .and(query_param("desc", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(one_result()))
            .mount(&server)
            .await;

        let classes = PelotonClasses::new(
            server.uri(),
            PelotonAuth::new(
                server.uri(),
                PelotonCredentials::new("rider@example.com", "not-a-real-password"),
            ),
        );

        let found = classes
            .cool_down_for(INSTRUCTOR)
            .await
            .expect("the stubbed search answers");
        assert_eq!(
            found,
            Some(ClassSummary {
                id: COOL_DOWN.to_owned(),
                title: "5 min Cool Down Ride".to_owned(),
                duration_seconds: 300,
            }),
        );
    });
}

/// **An instructor with no cool-down is an answer, not a fault.** The co-taught
/// "Denis & Matt" is a single instructor id and may well be exactly this. What a
/// session missing its cool-down means is the caller's to decide (§ 37); it is
/// not this function's business to invent one.
#[test]
fn an_instructor_with_no_cool_down_answers_nothing_rather_than_failing() {
    let empty = serde_json::json!({ "data": [], "total": 0 }).to_string();
    let found = cool_down_from(INSTRUCTOR, &empty).expect("an empty list reads");
    assert_eq!(found, None);
}

/// The newest is the one taken, because that is what the operator rides. The
/// search asks for one, and a source that ignores the limit must not change the
/// answer.
#[test]
fn the_first_result_is_the_one_taken() {
    let two = serde_json::json!({
        "data": [
            { "id": COOL_DOWN, "title": "5 min Cool Down Ride", "duration": 300 },
            { "id": "0000", "title": "5 min Cool Down Ride", "duration": 300 },
        ]
    })
    .to_string();
    let found = cool_down_from(INSTRUCTOR, &two).expect("the list reads");
    assert_eq!(found.map(|class| class.id), Some(COOL_DOWN.to_owned()));
}

/// A body that is not a listing is this reader having misunderstood the source,
/// which is a different thing from an instructor having no cool-down.
#[test]
fn a_body_that_is_not_a_listing_is_malformed() {
    assert!(cool_down_from(INSTRUCTOR, "<html>an error page</html>").is_err());
}
