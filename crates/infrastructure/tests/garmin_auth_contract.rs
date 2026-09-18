//! The contract with Garmin's SSO, pinned against a local stub.
//!
//! **This is the test the adapter shipped without**, and its absence is why a
//! sign-in that could never have worked passed every check. The flow it drives
//! is unofficial twice over: Garmin's Health API is closed to new applications,
//! and the phone app's JSON endpoint has answered 429 from Cloudflare since
//! March 2026 whoever asks. What is left is the embed widget's HTML form, and
//! nothing obliges Garmin to keep serving that either.
//!
//! What is pinned is our reading of the flow: that the embed page is fetched
//! for its cookies, that the `_csrf` is read out of the sign-in form and
//! submitted back, that the ticket is read off the success page, that the
//! exchange names the same service the ticket was issued for, and that each of
//! the four ways this can fail says which step failed and why.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use application::SourceError;
use infrastructure::{GarminAuth, GarminCredentials, garmin::token_base_for};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

fn credentials() -> GarminCredentials {
    GarminCredentials::new("rider@example.com", "not-a-real-password")
}

/// The adapter under test, pointed at one stub for both hosts and without the
/// anti-bot pause, which would otherwise cost the suite eight seconds a test.
fn auth(server: &MockServer) -> GarminAuth {
    let base = server.uri();
    GarminAuth::new(base.clone(), token_base_for(&base), credentials(), None).without_pause()
}

/// The sign-in form, with the token in Garmin's own spelling.
fn signin_form() -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("set-cookie", "SESSION=from-the-embed; Path=/")
        .set_body_string(
            r#"<html><head><title>GARMIN Authentication Application</title></head>
            <body><form method="post">
            <input type="hidden" name="_csrf" value="CSRF-FROM-THE-FORM" />
            <input type="text" name="username" value="" />
            </form></body></html>"#,
        )
}

/// What Garmin answers a credential POST it accepted.
fn success_page(server: &MockServer) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_string(format!(
        r#"<html><head><title>Success</title></head><body><script>
        response_url = "{}/sso/embed?ticket=ST-98765-theTicket-cas";
        </script></body></html>"#,
        server.uri()
    ))
}

/// Everything up to the credential POST, which each test answers for itself.
async fn sso_pages(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/sso/embed"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("set-cookie", "GARMIN-SSO=widget; Path=/")
                .set_body_string("<html><body>the widget</body></html>"),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/sso/signin"))
        .respond_with(signin_form())
        .mount(server)
        .await;
}

async fn token_endpoint(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/di-oauth2-service/oauth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "the-bearer",
            "refresh_token": "the-refresh",
            "expires_in": 3600,
        })))
        .mount(server)
        .await;
}

/// The whole flow, end to end.
#[test]
fn a_sign_in_earns_a_bearer_token() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let server = MockServer::start().await;
        sso_pages(&server).await;
        Mock::given(method("POST"))
            .and(path("/sso/signin"))
            .respond_with(success_page(&server))
            .mount(&server)
            .await;
        token_endpoint(&server).await;

        let bearer = auth(&server).bearer().await.expect("a bearer token");
        assert_eq!(bearer, "the-bearer");
    });
}

/// **The ticket is bound to the service that asked for it.** The sign-in names
/// the embed page in `service`, so the exchange has to name the same one; the
/// adapter sent a mobile integration URL it had never signed in under.
#[test]
fn the_exchange_names_the_service_the_sign_in_asked_for() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let server = MockServer::start().await;
        sso_pages(&server).await;
        Mock::given(method("POST"))
            .and(path("/sso/signin"))
            .respond_with(success_page(&server))
            .mount(&server)
            .await;
        token_endpoint(&server).await;

        auth(&server).bearer().await.expect("a bearer token");

        let requests = server.received_requests().await.unwrap_or_default();
        let embed = format!("{}/sso/embed", server.uri());

        let signin = requests
            .iter()
            .find(|request| {
                request.method == wiremock::http::Method::GET && request.url.path() == "/sso/signin"
            })
            .expect("the sign-in form was fetched");
        let asked_for = signin
            .url
            .query_pairs()
            .find(|(name, _)| name == "service")
            .map(|(_, value)| value.into_owned());
        assert_eq!(asked_for.as_deref(), Some(embed.as_str()));

        let exchange = requests
            .iter()
            .find(|request| request.url.path() == "/di-oauth2-service/oauth/token")
            .expect("the ticket was exchanged");
        let body = String::from_utf8_lossy(&exchange.body);
        let sent = form_value(&body, "service_url");
        assert_eq!(
            sent.as_deref(),
            Some(embed.as_str()),
            "the exchange must repeat the service the ticket was issued for, got {body}"
        );
        assert_eq!(
            form_value(&body, "service_ticket").as_deref(),
            Some("ST-98765-theTicket-cas")
        );
    });
}

/// The `_csrf` is read from the form and submitted back. A POST without it is
/// the one Garmin rejects, and it would reject it as a bad password.
#[test]
fn the_csrf_from_the_form_is_submitted_back() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let server = MockServer::start().await;
        sso_pages(&server).await;
        Mock::given(method("POST"))
            .and(path("/sso/signin"))
            .respond_with(success_page(&server))
            .mount(&server)
            .await;
        token_endpoint(&server).await;

        auth(&server).bearer().await.expect("a bearer token");

        let requests = server.received_requests().await.unwrap_or_default();
        let posted = requests
            .iter()
            .find(|request| {
                request.method == wiremock::http::Method::POST
                    && request.url.path() == "/sso/signin"
            })
            .expect("the credentials were posted");
        let body = String::from_utf8_lossy(&posted.body);
        assert_eq!(
            form_value(&body, "_csrf").as_deref(),
            Some("CSRF-FROM-THE-FORM")
        );
        assert_eq!(form_value(&body, "embed").as_deref(), Some("true"));
    });
}

/// **The failure that started this.** A non-2xx is reported as the status it
/// was, not as a field the body did not carry: the adapter parsed Garmin's 404
/// into an empty answer and reported a missing status, which said nothing about
/// the endpoint being wrong.
#[test]
fn a_missing_endpoint_is_reported_as_its_status() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sso/embed"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "status": 404,
                "error": "Not Found",
                "path": "/sso/embed",
            })))
            .mount(&server)
            .await;

        let Err(SourceError::Unavailable { detail }) = auth(&server).bearer().await else {
            panic!("a 404 should be unavailable, not malformed")
        };
        assert!(detail.contains("404"), "{detail}");
        assert!(detail.contains("/sso/embed"), "{detail}");
    });
}

/// Cloudflare's block, which is what the endpoint this replaced now answers.
#[test]
fn a_rate_limited_sign_in_says_so_and_does_not_retry() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sso/embed"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;

        let Err(SourceError::Unavailable { detail }) = auth(&server).bearer().await else {
            panic!("a 429 should be unavailable")
        };
        assert!(detail.contains("rate-limiting"), "{detail}");

        let attempts = server.received_requests().await.unwrap_or_default();
        assert_eq!(attempts.len(), 1, "a refused sign-in is not retried");
    });
}

/// A refused password is `Unauthorised`, so `fitness credentials garmin` says
/// the source would not have it rather than reporting a transport failure.
#[test]
fn a_refused_password_is_unauthorised() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let server = MockServer::start().await;
        sso_pages(&server).await;
        Mock::given(method("POST"))
            .and(path("/sso/signin"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "<html><head><title>Invalid username or password</title></head></html>",
            ))
            .mount(&server)
            .await;

        assert!(matches!(
            auth(&server).bearer().await,
            Err(SourceError::Unauthorised)
        ));
    });
}

/// MFA is reported, not driven, and not mistaken for a bad password.
#[test]
fn a_two_factor_challenge_names_the_next_step() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let server = MockServer::start().await;
        sso_pages(&server).await;
        Mock::given(method("POST"))
            .and(path("/sso/signin"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"<html><head><title>GARMIN Authentication Application</title></head>
                <body><script>var mfaMethod = "email"; var customerGuid = "abc";</script>
                </body></html>"#,
            ))
            .mount(&server)
            .await;

        let Err(SourceError::Unavailable { detail }) = auth(&server).bearer().await else {
            panic!("MFA should be reported, not treated as a refusal")
        };
        assert!(detail.contains("two-factor"), "{detail}");
    });
}

/// A form we cannot submit says which piece was missing.
#[test]
fn a_form_without_a_csrf_says_which_piece_was_missing() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sso/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html></html>"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/sso/signin"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html><form></form></html>"))
            .mount(&server)
            .await;

        let Err(SourceError::Malformed { detail }) = auth(&server).bearer().await else {
            panic!("a form with no token is malformed")
        };
        assert!(detail.contains("_csrf"), "{detail}");
    });
}

/// One `name=value` out of a form-encoded body.
fn form_value(body: &str, name: &str) -> Option<String> {
    body.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| value.replace("%2F", "/").replace("%3A", ":"))
    })
}
