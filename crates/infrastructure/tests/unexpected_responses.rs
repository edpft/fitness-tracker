//! An answer we did not expect is kept in the store, whoever asked for it (#164).
//!
//! The operator, 2026-09-18: *"if we receive a response we don't expect, to
//! any request we make, we should store the bytes so that we can investigate
//! later."* What cost that day was a Garmin sign-in step answering 404 with a
//! body that said so, which was dropped and took a live probe to recover. The
//! first test here is that day.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use std::error::Error;

use application::{SourceError, WorkoutEventSource as _};
use infrastructure::{
    GarminAuth, GarminCredentials, HevyWorkoutEvents, RetryPolicy, connect, garmin::token_base_for,
};
use sqlx::SqlitePool;
use tempfile::TempDir;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

type Fallible<T> = Result<T, Box<dyn Error>>;

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

/// A store, opened: which is what turns the keeping on.
async fn store() -> Fallible<(TempDir, SqlitePool)> {
    let directory = TempDir::new()?;
    let pool = connect(&directory.path().join("fitness.db")).await?;
    Ok((directory, pool))
}

fn garmin(server: &MockServer) -> GarminAuth {
    let base = server.uri();
    GarminAuth::new(
        base.clone(),
        token_base_for(&base),
        GarminCredentials::new("rider@example.com", "not-a-real-password"),
        None,
    )
    .without_pause()
}

fn hevy(server: &MockServer) -> HevyWorkoutEvents {
    HevyWorkoutEvents::with_retry(
        server.uri(),
        "00000000-0000-0000-0000-000000000000",
        RetryPolicy::immediate(3),
    )
}

/// One kept answer, as the columns hold it.
#[derive(Debug, sqlx::FromRow)]
struct Kept {
    method: String,
    url: String,
    status: String,
    succeeded: i64,
    headers: Vec<u8>,
    body: Vec<u8>,
    detail: String,
}

async fn kept(pool: &SqlitePool) -> Result<Vec<Kept>, sqlx::Error> {
    sqlx::query_as(
        "SELECT method, url, status, succeeded, headers, body, detail
         FROM unexpected_response ORDER BY id",
    )
    .fetch_all(pool)
    .await
}

/// **2026-09-18, from the store alone.** A sign-in step on the wrong path
/// answers 404 with a body naming the path; the row holds the address, the
/// status, the headers — `Set-Cookie` included, as the operator ruled — and
/// the body as served.
#[test]
fn a_sign_in_step_that_answers_404_is_kept_whole() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = store().await.expect("a store");
        let server = MockServer::start().await;
        let said = r#"{"status":404,"error":"Not Found","path":"/sso/embed"}"#;
        Mock::given(method("GET"))
            .and(path("/sso/embed"))
            .respond_with(
                ResponseTemplate::new(404)
                    .insert_header("set-cookie", "GARMIN-SSO=widget; Path=/")
                    .set_body_string(said),
            )
            .mount(&server)
            .await;

        let refused = garmin(&server).bearer().await;
        assert!(refused.is_err(), "a 404 is not a sign-in: {refused:?}");

        let rows = kept(&pool).await.expect("the rows read back");
        assert_eq!(rows.len(), 1, "one answer, one row: {rows:?}");
        let row = rows.first().expect("a row");
        assert_eq!(row.method, "GET");
        assert!(
            row.url.starts_with(&format!("{}/sso/embed?", server.uri())),
            "the address asked, query and all: {}",
            row.url
        );
        assert_eq!(row.status, "404 Not Found");
        assert_eq!(row.succeeded, 0);
        assert_eq!(row.body, said.as_bytes(), "the body as served");
        let headers = String::from_utf8_lossy(&row.headers);
        assert!(
            headers.contains("set-cookie: GARMIN-SSO=widget; Path=/\r\n"),
            "the headers as served: {headers}"
        );
        assert!(
            row.detail.contains("/sso/embed answered 404"),
            "what the reader made of it: {}",
            row.detail
        );
    });
}

/// **An answer that succeeded and could not be read is kept too**, and says it
/// succeeded: the status was fine, the shape was not.
#[test]
fn a_success_that_cannot_be_read_is_kept() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = store().await.expect("a store");
        let server = MockServer::start().await;
        let said = "<html>the page moved</html>";
        Mock::given(method("GET"))
            .and(path("/v1/workouts/events"))
            .respond_with(ResponseTemplate::new(200).set_body_string(said))
            .mount(&server)
            .await;

        let refused = hevy(&server).fetch(None, None).await;
        assert!(
            matches!(refused, Err(SourceError::Malformed { .. })),
            "an unreadable page is malformed: {refused:?}"
        );

        let rows = kept(&pool).await.expect("the rows read back");
        assert_eq!(rows.len(), 1, "{rows:?}");
        let row = rows.first().expect("a row");
        assert_eq!(row.status, "200 OK");
        assert_eq!(row.succeeded, 1);
        assert_eq!(row.body, said.as_bytes());
    });
}

/// **A refusal that is retried past is still kept.** The retry is what makes
/// the run succeed; the 503 is still an answer nobody wanted, and whether a
/// source is throttling or failing is read off exactly these bytes (#368).
#[test]
fn an_answer_retried_past_is_kept() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = store().await.expect("a store");
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/workouts/events"))
            .respond_with(ResponseTemplate::new(503).set_body_string("try later"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/workouts/events"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"page":1,"page_count":1,"workouts":[]}"#),
            )
            .mount(&server)
            .await;

        hevy(&server)
            .fetch(None, None)
            .await
            .expect("the retry succeeds");

        let rows = kept(&pool).await.expect("the rows read back");
        assert_eq!(rows.len(), 1, "only the 503: {rows:?}");
        let row = rows.first().expect("a row");
        assert_eq!(row.status, "503 Service Unavailable");
        assert_eq!(row.body, b"try later");
    });
}

/// **What was expected is not kept.** A whole sign-in that works — three pages
/// and a token — leaves nothing behind.
#[test]
fn a_sign_in_that_works_keeps_nothing() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = store().await.expect("a store");
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/sso/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_string("the widget"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/sso/signin"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"<form><input type="hidden" name="_csrf" value="CSRF" /></form>"#,
            ))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/sso/signin"))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!(
                r#"<title>Success</title><script>u = "{}/sso/embed?ticket=ST-1-x-cas";</script>"#,
                server.uri()
            )))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/di-oauth2-service/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "the-bearer",
                "refresh_token": "the-refresh",
                "expires_in": 3600,
            })))
            .mount(&server)
            .await;

        garmin(&server).bearer().await.expect("a bearer token");

        let rows = kept(&pool).await.expect("the rows read back");
        assert!(rows.is_empty(), "nothing unexpected: {rows:?}");
    });
}

/// **Kept as it was answered.** No row is rewritten or removed.
#[test]
fn a_kept_answer_cannot_be_changed() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = store().await.expect("a store");
        sqlx::query(
            "INSERT INTO unexpected_response
                 (method, url, status, succeeded, headers, body, detail, answered_at)
             VALUES ('GET', 'http://x/', '404 Not Found', 0, x'', x'', 'gone', 'now')",
        )
        .execute(&pool)
        .await
        .expect("a row goes in");

        let updated = sqlx::query("UPDATE unexpected_response SET body = x'00'")
            .execute(&pool)
            .await;
        let deleted = sqlx::query("DELETE FROM unexpected_response")
            .execute(&pool)
            .await;
        assert!(updated.is_err(), "no update");
        assert!(deleted.is_err(), "no delete");
    });
}
