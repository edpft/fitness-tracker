//! The contract with Withings, pinned against a local stub.
//!
//! What is pinned is our reading of the documentation, since no real payload
//! has been seen yet: the token exchange and its rotation, `getmeas` paging by
//! `offset` under one `lastupdate`, a group landed as the bytes served, and a
//! failure reported inside a 200.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use application::{SourceError, WorkoutEventSource as _};
use infrastructure::{TokenFile, WithingsAuth, WithingsClient, WithingsMeasurements};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, header, method, path},
};

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

fn client() -> WithingsClient {
    WithingsClient::new("the-client", "the-secret", "https://example.test/back")
}

fn issued(access: &str, refresh: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "status": 0,
        "body": {
            "userid": "363",
            "access_token": access,
            "refresh_token": refresh,
            "scope": "user.metrics",
            "expires_in": 10800,
            "token_type": "Bearer",
        }
    }))
}

/// A group as the documentation describes one, with spacing a re-serialiser
/// would not keep.
const FIRST_GROUP: &str = r#"{"grpid": 11, "attrib": 0, "date": 1757923200, "created": 1757923210, "modified": 1757923210, "category": 1, "deviceid": "abc", "measures": [{"value": 81450, "type": 1, "unit": -3, "algo": 0, "fm": 3}]}"#;
const SECOND_GROUP: &str = r#"{"grpid":12,"attrib":0,"date":1758009600,"created":1758009610,"modified":1758096000,"category":1,"deviceid":"abc","measures":[{"value":8120,"type":1,"unit":-2}]}"#;

/// **The sign-in exchanges the pasted code for a token and keeps it**, with
/// the client's secret sent to the token endpoint and nowhere else.
#[test]
fn a_sign_in_keeps_the_token_it_was_given() {
    let rt = runtime().expect("a current-thread runtime builds");
    rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v2/oauth2"))
            .and(body_string_contains("action=requesttoken"))
            .and(body_string_contains("grant_type=authorization_code"))
            .and(body_string_contains("code=the-code"))
            .and(body_string_contains("client_secret=the-secret"))
            .respond_with(issued("first-access", "first-refresh"))
            .expect(1)
            .mount(&server)
            .await;

        let directory = tempfile::tempdir().expect("a temporary directory");
        let cache = TokenFile::new(directory.path().join("withings.token.json"));
        let auth = WithingsAuth::new(server.uri(), server.uri(), client(), cache.clone());

        auth.sign_in("https://example.test/back?code=the-code&state=ours", "ours")
            .await
            .expect("the stubbed exchange completes");

        let kept = cache.read().expect("the token is written down");
        assert_eq!(kept.access(), "first-access");
        assert_eq!(kept.refresh(), Some("first-refresh"));
        assert!(kept.usable());
    });
}

/// **A spent token is renewed, and the renewed refresh token replaces the old
/// one on disk** — Withings rotates it, so keeping the old one would cost a
/// sign-in next time.
#[test]
fn a_spent_token_is_renewed_and_the_rotation_is_kept() {
    let rt = runtime().expect("a current-thread runtime builds");
    rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v2/oauth2"))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains("refresh_token=old-refresh"))
            .respond_with(issued("new-access", "new-refresh"))
            .expect(1)
            .mount(&server)
            .await;

        let directory = tempfile::tempdir().expect("a temporary directory");
        let cache = TokenFile::new(directory.path().join("withings.token.json"));
        let yesterday = jiff::Timestamp::now()
            .checked_sub(jiff::SignedDuration::from_hours(24))
            .expect("a time");
        cache
            .write(&infrastructure::Token::new(
                "old-access".to_owned(),
                Some("old-refresh".to_owned()),
                yesterday,
            ))
            .expect("the stale token is written");

        let auth = WithingsAuth::new(server.uri(), server.uri(), client(), cache.clone());
        assert_eq!(auth.bearer().await.expect("renewed"), "new-access");
        assert_eq!(
            auth.bearer().await.expect("held"),
            "new-access",
            "and not renewed twice"
        );
        assert_eq!(cache.read().expect("kept").refresh(), Some("new-refresh"));
    });
}

/// With no token at all there is nothing to renew: that is a sign-in, not a
/// network call.
#[test]
fn no_token_is_unauthorised_without_asking_withings() {
    let rt = runtime().expect("a current-thread runtime builds");
    rt.block_on(async {
        let server = MockServer::start().await;
        let directory = tempfile::tempdir().expect("a temporary directory");
        let auth = WithingsAuth::new(
            server.uri(),
            server.uri(),
            client(),
            TokenFile::new(directory.path().join("absent.json")),
        );
        assert!(matches!(
            auth.bearer().await,
            Err(SourceError::Unauthorised)
        ));
        assert!(
            server
                .received_requests()
                .await
                .unwrap_or_default()
                .is_empty()
        );
    });
}

/// A source holding a live token. Returns a `Result`: the test exemptions for
/// panicking do not reach a free function.
fn signed_in(
    server: &MockServer,
    directory: &std::path::Path,
) -> Result<WithingsMeasurements, String> {
    let cache = TokenFile::new(directory.join("withings.token.json"));
    let tomorrow = jiff::Timestamp::now()
        .checked_add(jiff::SignedDuration::from_hours(24))
        .map_err(|error| error.to_string())?;
    cache
        .write(&infrastructure::Token::new(
            "live-access".to_owned(),
            Some("a-refresh".to_owned()),
            tomorrow,
        ))
        .map_err(|error| error.to_string())?;
    Ok(WithingsMeasurements::new(
        server.uri(),
        WithingsAuth::new(server.uri(), server.uri(), client(), cache),
    ))
}

/// **Two pages under one `lastupdate`, each group landed as served.** The
/// second request carries the offset the first answered with, and both carry
/// the `lastupdate` the walk began with.
#[test]
fn measurements_page_by_offset_and_land_the_bytes_served() {
    let rt = runtime().expect("a current-thread runtime builds");
    rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/measure"))
            .and(header("authorization", "Bearer live-access"))
            .and(body_string_contains("action=getmeas"))
            .and(body_string_contains("category=1"))
            .and(body_string_contains("lastupdate=0"))
            .and(body_string_contains("offset=5"))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!(
                r#"{{"status":0,"body":{{"updatetime":1758100000,"timezone":"Europe/London","measuregrps":[{SECOND_GROUP}],"more":0,"offset":0}}}}"#
            )))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/measure"))
            .and(body_string_contains("lastupdate=0"))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!(
                r#"{{"status":0,"body":{{"updatetime":1758100000,"timezone":"Europe/London","measuregrps":[{FIRST_GROUP}],"more":1,"offset":5}}}}"#
            )))
            .mount(&server)
            .await;

        let directory = tempfile::tempdir().expect("a temporary directory");
        let source = signed_in(&server, directory.path()).expect("a signed-in source");

        let first = source.fetch(None, None).await.expect("the first page");
        assert_eq!(first.events.len(), 1);
        let event = first.events.first().expect("one group");
        assert_eq!(event.source_record_id.as_str(), "11");
        assert_eq!(
            event.payload.as_bytes(),
            FIRST_GROUP.as_bytes(),
            "the bytes as served, spacing and all"
        );
        let domain::landing::Provenance::Event(provenance) = &event.provenance;
        assert_eq!(provenance.endpoint().as_str(), "/measure");
        let resume = first.resume.expect("there is more");
        assert_eq!(resume.offset(), 5);

        let second = source
            .fetch(None, Some(resume))
            .await
            .expect("the second page");
        assert!(second.resume.is_none(), "that was everything");
        let event = second.events.first().expect("one group");
        assert_eq!(event.source_record_id.as_str(), "12");
        assert_eq!(
            event
                .provenance
                .occurred_at()
                .map(|at| at.as_timestamp().as_second()),
            Some(1_758_096_000),
            "the later of created and modified"
        );
    });
}

/// **Withings refuses a token inside a 200**, and that is a rejected
/// credential rather than an unreadable answer.
#[test]
fn a_refused_token_is_unauthorised() {
    let rt = runtime().expect("a current-thread runtime builds");
    rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/measure"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"status":401,"body":{},"error":"XRequestID: Not provided invalid_token: The access token provided is invalid"}"#,
            ))
            .mount(&server)
            .await;

        let directory = tempfile::tempdir().expect("a temporary directory");
        let source = signed_in(&server, directory.path()).expect("a signed-in source");
        assert!(matches!(
            source.fetch(None, None).await,
            Err(SourceError::Unauthorised)
        ));
    });
}

/// A page that says there is more without saying where would loop forever if
/// believed.
#[test]
fn more_without_an_offset_is_malformed() {
    let rt = runtime().expect("a current-thread runtime builds");
    rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/measure"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"status":0,"body":{"measuregrps":[],"more":1}}"#),
            )
            .mount(&server)
            .await;

        let directory = tempfile::tempdir().expect("a temporary directory");
        let source = signed_in(&server, directory.path()).expect("a signed-in source");
        assert!(matches!(
            source.fetch(None, None).await,
            Err(SourceError::Malformed { .. })
        ));
    });
}
