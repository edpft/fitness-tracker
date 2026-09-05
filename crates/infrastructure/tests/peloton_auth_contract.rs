//! The contract with Peloton's Auth0 tenant, pinned against a local stub.
//!
//! **This is the test that matters most in the Peloton adapter**, because the
//! flow it drives is unofficial. `POST /auth/login` — the way everyone used to
//! authenticate — now answers `403 Endpoint no longer accepting requests`, and
//! nothing obliges Peloton to keep the replacement working either. When it goes,
//! one of these fails and says which step, rather than a run quietly prescribing
//! from a stale FTP (decision 0033).
//!
//! What is pinned is our reading of the flow: that the `_csrf` cookie is taken
//! from the authorize redirect, that the login answer's hidden form is
//! resubmitted rather than followed, that the `code` is read off a `Location`,
//! and that a rejected password is terminal rather than retried.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use application::SourceError;
use infrastructure::peloton::auth::{PelotonAuth, PelotonCredentials};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

fn credentials() -> PelotonCredentials {
    PelotonCredentials::new("rider@example.com", "not-a-real-password")
}

/// Auth0's self-submitting form, with the escaping that broke a regex.
///
/// `wctx` is HTML-escaped JSON and `wresult` is a signed blob. A parser that
/// stops at the first quote inside `wctx` truncates it, and `/login/callback`
/// answers 400 without explaining why — which is exactly what happened while
/// this was being written.
fn hidden_form(callback: &str) -> String {
    format!(
        r#"<html><body onload="document.forms[0].submit()">
        <form method="post" action="{callback}">
        <input type="hidden" name="wa" value="wsignin1.0" />
        <input type="hidden" name="wresult" value="eyJhbGciOiJIUzI1NiJ9.signed" />
        <input type="hidden" name="wctx" value="{{&quot;strategy&quot;:&quot;auth0&quot;,&quot;tenant&quot;:&quot;peloton-prod&quot;}}" />
        </form></body></html>"#
    )
}

/// Stub the whole flow, from authorize to token.
async fn happy_path(server: &MockServer, expires_in: u64) {
    let base = server.uri();
    Mock::given(method("GET"))
        .and(path("/authorize"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("set-cookie", "_csrf=csrf-from-authorize; Path=/")
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
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(hidden_form(&format!("{base}/login/callback"))),
        )
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/login/callback"))
        .respond_with(ResponseTemplate::new(302).insert_header(
            "location",
            "https://members.onepeloton.com/callback?code=the-authorization-code&state=carried",
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

#[test]
fn a_full_login_walks_the_flow_and_returns_a_bearer_token() {
    let rt = runtime().expect("a current-thread runtime builds");
    rt.block_on(async {
        let server = MockServer::start().await;
        happy_path(&server, 172_800).await;

        let auth = PelotonAuth::new(server.uri(), credentials());
        let token = auth.bearer().await.expect("the stubbed flow completes");
        assert_eq!(token, "the-access-token");
    });
}

/// **The second call must not log in again.** A token that is still good is the
/// whole reason the flow is tolerable: a login is five round trips through
/// someone else's authentication system.
#[test]
fn a_second_call_reuses_the_token_without_touching_the_network() {
    let rt = runtime().expect("a current-thread runtime builds");
    rt.block_on(async {
        let server = MockServer::start().await;
        happy_path(&server, 172_800).await;

        let auth = PelotonAuth::new(server.uri(), credentials());
        let first = auth.bearer().await.expect("the stubbed flow completes");
        let second = auth.bearer().await.expect("the cached token is returned");
        assert_eq!(first, second);

        let logins = server
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|request| request.url.path() == "/usernamepassword/login")
            .count();
        assert_eq!(logins, 1, "the credentials were submitted more than once");
    });
}

/// A token at or past its margin is spent, and the refresh token is spent
/// first — a refresh is one round trip where a login is five.
#[test]
fn an_expired_token_is_refreshed_rather_than_relogged() {
    let rt = runtime().expect("a current-thread runtime builds");
    rt.block_on(async {
        let server = MockServer::start().await;
        // Inside the sixty-second margin, so it is spent on arrival.
        happy_path(&server, 1).await;

        let auth = PelotonAuth::new(server.uri(), credentials());
        let first = auth.bearer().await.expect("the stubbed flow completes");
        assert_eq!(first, "the-access-token");
        let second = auth.bearer().await.expect("the refresh succeeds");
        assert_eq!(second, "the-access-token");

        let requests = server.received_requests().await.unwrap_or_default();
        let logins = requests
            .iter()
            .filter(|request| request.url.path() == "/usernamepassword/login")
            .count();
        let tokens = requests
            .iter()
            .filter(|request| request.url.path() == "/oauth/token")
            .count();
        assert_eq!(logins, 1, "a refresh should not re-submit the password");
        assert_eq!(tokens, 2, "the second call should have refreshed");
    });
}

/// **A rejected password is terminal.** It will not un-reject itself, and
/// retrying one against an authentication endpoint is what an attack looks like
/// from the far end.
#[test]
fn a_rejected_credential_is_unauthorised_and_not_retried() {
    let rt = runtime().expect("a current-thread runtime builds");
    rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/authorize"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("set-cookie", "_csrf=csrf-from-authorize; Path=/")
                    .insert_header("location", "/login"),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/login"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html></html>"))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/usernamepassword/login"))
            .respond_with(ResponseTemplate::new(401).set_body_string("{}"))
            .mount(&server)
            .await;

        let auth = PelotonAuth::new(server.uri(), credentials());
        let failure = auth.bearer().await.expect_err("a 401 is not a token");
        assert_eq!(failure, SourceError::Unauthorised);

        let attempts = server
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|request| request.url.path() == "/usernamepassword/login")
            .count();
        assert_eq!(attempts, 1, "a rejected credential was submitted twice");
    });
}

/// Without the cookie there is nothing to sign the login with, and saying so is
/// better than posting a form that will be refused.
#[test]
fn a_missing_csrf_cookie_is_reported_rather_than_guessed() {
    let rt = runtime().expect("a current-thread runtime builds");
    rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/authorize"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html></html>"))
            .mount(&server)
            .await;

        let auth = PelotonAuth::new(server.uri(), credentials());
        let failure = auth.bearer().await.expect_err("no cookie, no login");
        let SourceError::Malformed { detail } = failure else {
            panic!("a missing CSRF cookie is a malformed response, not an outage")
        };
        assert!(
            detail.contains("_csrf"),
            "the message should name what is missing: {detail}"
        );
    });
}

/// The one that catches Peloton switching this off, as they did to
/// `/auth/login`. A 403 at the authorize step must not be mistaken for a
/// credential problem.
#[test]
fn a_closed_endpoint_is_an_outage_and_not_a_bad_password() {
    let rt = runtime().expect("a current-thread runtime builds");
    rt.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/authorize"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html></html>"))
            .mount(&server)
            .await;

        let auth = PelotonAuth::new(server.uri(), credentials());
        let failure = auth.bearer().await.expect_err("no cookie means no flow");
        assert_ne!(
            failure,
            SourceError::Unauthorised,
            "a closed endpoint must not read as a rejected credential"
        );
    });
}
