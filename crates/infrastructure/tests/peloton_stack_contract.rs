//! The contract with Peloton's stack, pinned against a local stub.
//!
//! **The identifier is the whole risk.** A stack write with a wrong identifier
//! answers 200 with a success payload and changes nothing (#70) — a real ride id
//! and the string `not-a-class-id-at-all` are indistinguishable to the caller.
//! Nothing this adapter can observe at runtime tells the two apart, so what the
//! token is made of is asserted here or nowhere.
//!
//! **Reading the counters is not reading the stack.** `numClasses` and
//! `totalTime` are zero both for an empty stack and for a write that missed,
//! which is how #70 stayed open on the wrong verdict. `view` reads the list.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use infrastructure::peloton::{
    PelotonStack,
    auth::{PelotonAuth, PelotonCredentials},
    join_token,
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, method, path},
};

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

const RIDE: &str = "725d618516674f7581d4d566fe3f0655";

/// The token for [`RIDE`], as the live API accepted it on 2026-09-06.
fn expected_token() -> String {
    STANDARD.encode(format!(
        r#"{{"home_peloton_id": null, "ride_id": "{RIDE}", "studio_peloton_id": null, "type": "on_demand"}}"#
    ))
}

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

fn stack(server: &MockServer) -> PelotonStack {
    PelotonStack::new(
        format!("{}/graphql", server.uri()),
        PelotonAuth::new(
            server.uri(),
            PelotonCredentials::new("rider@example.com", "not-a-real-password"),
        ),
    )
}

/// **The one assertion nothing at runtime can make for us.** A wrong token is
/// accepted, answered 200 and ignored, so the encoding is pinned against the
/// exact bytes the live API took.
#[test]
fn a_class_is_stacked_by_its_join_token_and_not_its_id() {
    let token = join_token(RIDE);
    assert_ne!(token, RIDE, "a raw ride id decodes to nothing");

    let decoded = STANDARD.decode(&token).expect("the token is base64");
    let json: serde_json::Value =
        serde_json::from_slice(&decoded).expect("it decodes to an object");

    assert_eq!(json["ride_id"], RIDE);
    assert_eq!(json["type"], "on_demand");
    assert!(json["home_peloton_id"].is_null());
    assert!(json["studio_peloton_id"].is_null());
    assert_eq!(
        token,
        expected_token(),
        "byte for byte what Peloton accepted"
    );
}

/// **Setting is a clear, then one add per class, then a read back.**
/// `modifyStack` answers success and changes nothing whatever it is given, so
/// only `addClassToStack` writes — and the result is checked rather than
/// believed.
#[test]
fn setting_the_stack_adds_each_class_and_checks_what_landed() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;

        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_string_contains("modifyStack"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "modifyStack": {
                    "totalTime": 0,
                    "userStack": { "stackedClassList": [] },
                }}
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_string_contains("addClassToStack"))
            .and(body_string_contains(expected_token()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "addClassToStack": {
                    "totalTime": 2700,
                    "userStack": { "stackedClassList": [{ "pelotonClassId": expected_token() }] },
                }}
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_string_contains("viewUserStack"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "viewUserStack": {
                    "totalTime": 2700,
                    "userStack": { "stackedClassList": [{ "pelotonClassId": expected_token() }] },
                }}
            })))
            .mount(&server)
            .await;

        let stacked = stack(&server)
            .set(&[RIDE.to_owned()])
            .await
            .expect("the stubbed sequence answers");
        assert_eq!(stacked.classes, vec![expected_token()]);
    });
}

/// **A write that reports success and does nothing is a failure here.** Twice
/// this adapter believed a 200 that had changed nothing — once for the whole of
/// #70, and once again for `modifyStack` after the identifier was fixed. The
/// stack is read back and compared, so the third time says so.
#[test]
fn a_write_that_changes_nothing_is_refused_however_it_answers() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;

        // Everything answers a cheerful, empty success — which is exactly what
        // Peloton does.
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "modifyStack": { "totalTime": 0, "userStack": { "stackedClassList": [] } },
                    "addClassToStack": { "totalTime": 0, "userStack": { "stackedClassList": [] } },
                    "viewUserStack": { "totalTime": 0, "userStack": { "stackedClassList": [] } },
                }
            })))
            .mount(&server)
            .await;

        let refused = stack(&server).set(&[RIDE.to_owned()]).await;
        assert!(
            refused.is_err(),
            "an empty stack after a successful write is a failure, not a success",
        );
    });
}

/// Order is the order they will play, so it is the order they are sent.
#[test]
fn the_classes_are_sent_in_the_order_they_will_be_ridden() {
    let cool_down = "df190d7077c244208bdf6f9e0dae12f5";
    let sent = format!("[\"{}\", \"{}\"]", join_token(RIDE), join_token(cool_down));
    assert!(
        sent.find(&join_token(RIDE)) < sent.find(&join_token(cool_down)),
        "the ride is stacked before its cool down",
    );
}

/// **An empty list is how the stack is emptied**, and there is no other way:
/// the schema has no `clearStack` and no `removeClassFromStack`. It is also the
/// one thing `modifyStack` actually does.
#[test]
fn an_empty_list_clears_the_stack() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;

        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_string_contains("pelotonClassIdList: []"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "modifyStack": {
                    "totalTime": 0,
                    "userStack": { "stackedClassList": [] },
                }}
            })))
            .mount(&server)
            .await;

        stack(&server).clear().await.expect("clearing answers");
    });
}

/// **GraphQL says no in a 200.** A gateway that answers "cannot query field"
/// with an HTTP 200 must not read as success — reading the status alone is the
/// whole family of faults this adapter has already been caught by once.
#[test]
fn errors_in_a_200_are_failures() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;

        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "errors": [{ "message": "Cannot query field \"stack\" on type \"Query\"." }]
            })))
            .mount(&server)
            .await;

        let refused = stack(&server).view().await;
        assert!(refused.is_err(), "a 200 carrying errors is not a success");
    });
}

/// Reading an empty stack is an answer, not a fault: it is the ordinary state
/// between rides.
#[test]
fn an_empty_stack_reads_as_empty() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;

        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_string_contains("viewUserStack"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "viewUserStack": {
                    "totalTime": 0,
                    "userStack": { "stackedClassList": [] },
                }}
            })))
            .mount(&server)
            .await;

        let stacked = stack(&server).view().await.expect("the stub answers");
        assert!(stacked.is_empty());
        assert_eq!(stacked.total_seconds, 0);
    });
}
