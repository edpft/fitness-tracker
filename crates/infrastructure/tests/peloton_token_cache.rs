//! The token kept between runs (#54).
//!
//! **What this is really testing is that a second run does not log in.** Every
//! invocation of `fitness` walked the whole Auth0 flow — five round trips
//! through the SSO domain — to obtain a token the last one already had. The
//! assertion that matters is a request count against the token endpoint, not a
//! round trip through a struct.
//!
//! **Losing the file costs a login, not a fact** (§ II, reconstructible state),
//! so every way of failing to read it is `None` rather than an error, and every
//! way of failing to write it is ignored. Those are the cases below.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use infrastructure::peloton::{
    TokenFile,
    auth::{PelotonAuth, PelotonCredentials, Token},
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

/// The whole Auth0 flow, stubbed, with the token endpoint counted.
async fn login_flow(server: &MockServer, expires_in: u64) {
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
            "refresh_token": "the-refresh-token",
            "token_type": "Bearer",
            "expires_in": expires_in,
        })))
        .mount(server)
        .await;
}

fn credentials() -> PelotonCredentials {
    PelotonCredentials::new("rider@example.com", "not-a-real-password")
}

/// How many times the token endpoint was called.
async fn logins(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == "/oauth/token")
        .count()
}

/// **The whole point of #54.** Two processes, one login.
#[test]
fn a_second_run_reads_the_token_rather_than_logging_in_again() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    let Ok(directory) = tempfile::tempdir() else {
        panic!("a temporary directory is available")
    };
    let cache = TokenFile::new(directory.path().join("peloton.token.json"));

    rt.block_on(async {
        let server = MockServer::start().await;
        login_flow(&server, 172_800).await;

        // The first run: nothing written down, so it logs in.
        let first = PelotonAuth::new(server.uri(), credentials()).caching_in(cache.clone());
        assert_eq!(
            first.bearer().await.expect("the stubbed flow completes"),
            "the-access-token",
        );
        assert_eq!(logins(&server).await, 1);

        // A different `PelotonAuth`, as a second invocation would build.
        let second = PelotonAuth::new(server.uri(), credentials()).caching_in(cache.clone());
        assert_eq!(
            second
                .bearer()
                .await
                .expect("the written token is read back"),
            "the-access-token",
        );
        assert_eq!(
            logins(&server).await,
            1,
            "the second run must not have logged in",
        );
    });
}

/// Without a cache, nothing is written down and the old behaviour stands — which
/// is what the contract tests rely on, and what a composition that does not own
/// a disk should get.
#[test]
fn without_a_cache_every_run_logs_in() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let server = MockServer::start().await;
        login_flow(&server, 172_800).await;

        for _ in 0..2 {
            let auth = PelotonAuth::new(server.uri(), credentials());
            auth.bearer().await.expect("the stubbed flow completes");
        }
        assert_eq!(logins(&server).await, 2);
    });
}

/// **A token that has expired is not used**, and the file does not stop the
/// refresh happening. Written with an expiry in the past, the next run must go
/// back to Auth0 rather than send a dead token.
#[test]
fn an_expired_token_on_disk_is_not_used() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    let Ok(directory) = tempfile::tempdir() else {
        panic!("a temporary directory is available")
    };
    let cache = TokenFile::new(directory.path().join("peloton.token.json"));
    let Ok(yesterday) = jiff::Timestamp::now().checked_sub(jiff::Span::new().hours(24)) else {
        panic!("yesterday is a time")
    };
    let stale = Token::new("a-dead-token".to_owned(), None, yesterday);
    cache.write(&stale).expect("the cache is writable");

    rt.block_on(async {
        let server = MockServer::start().await;
        login_flow(&server, 172_800).await;

        let auth = PelotonAuth::new(server.uri(), credentials()).caching_in(cache.clone());
        assert_eq!(
            auth.bearer().await.expect("it logs in again"),
            "the-access-token",
        );
        assert_eq!(logins(&server).await, 1, "the dead token was not sent");
    });
}

/// Every way of failing to read is the same answer, because every one costs the
/// same thing: one login. A file this program did not write is not a crash.
#[test]
fn an_unreadable_cache_is_absent_rather_than_fatal() {
    let Ok(directory) = tempfile::tempdir() else {
        panic!("a temporary directory is available")
    };

    let missing = TokenFile::new(directory.path().join("nothing-here.json"));
    assert_eq!(missing.read(), None, "no file");

    let nonsense = directory.path().join("nonsense.json");
    std::fs::write(&nonsense, "<html>not json</html>").expect("the file writes");
    assert_eq!(TokenFile::new(nonsense).read(), None, "not this format");

    let undated = directory.path().join("undated.json");
    std::fs::write(
        &undated,
        r#"{"access_token": "t", "expires_at": "the day before yesterday"}"#,
    )
    .expect("the file writes");
    assert_eq!(TokenFile::new(undated).read(), None, "not a time");
}

/// A refresh token is optional and its absence must round trip as absence — a
/// token that claims a refresh it does not have would spend a request finding
/// out.
#[test]
fn a_token_round_trips_through_the_file_exactly() {
    let Ok(directory) = tempfile::tempdir() else {
        panic!("a temporary directory is available")
    };
    let cache = TokenFile::new(directory.path().join("peloton.token.json"));
    let Ok(expires_at) = "2026-09-08T09:30:00Z".parse::<jiff::Timestamp>() else {
        panic!("that is a timestamp")
    };

    let with_refresh = Token::new("access".to_owned(), Some("refresh".to_owned()), expires_at);
    cache.write(&with_refresh).expect("the cache is writable");
    assert_eq!(cache.read(), Some(with_refresh));

    let without = Token::new("access".to_owned(), None, expires_at);
    cache.write(&without).expect("the cache is writable");
    assert_eq!(cache.read(), Some(without));
}

/// **Owner-only, and set as the file is created.** Narrowing afterwards leaves a
/// window in which the token is world-readable, and on a shared machine that
/// window is the whole vulnerability — `credentials.rs`'s reasoning, applied to
/// the file that holds the shorter-lived secret.
#[cfg(unix)]
#[test]
fn the_cache_is_readable_only_by_its_owner() {
    use std::os::unix::fs::PermissionsExt as _;

    let Ok(directory) = tempfile::tempdir() else {
        panic!("a temporary directory is available")
    };
    let cache = TokenFile::new(directory.path().join("peloton.token.json"));
    let Ok(expires_at) = "2026-09-08T09:30:00Z".parse::<jiff::Timestamp>() else {
        panic!("that is a timestamp")
    };
    cache
        .write(&Token::new("access".to_owned(), None, expires_at))
        .expect("the cache is writable");

    let mode = std::fs::metadata(cache.path())
        .expect("the file is there")
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "owner read and write, and nothing else"
    );
}

/// Forgetting one that is not there is not a failure — `logout` should be
/// idempotent, and the ordinary state on a fresh machine is no file.
#[test]
fn forgetting_an_absent_token_succeeds() {
    let Ok(directory) = tempfile::tempdir() else {
        panic!("a temporary directory is available")
    };
    let cache = TokenFile::new(directory.path().join("peloton.token.json"));
    assert!(cache.forget().is_ok());
}
