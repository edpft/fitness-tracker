//! The contract with Hevy, pinned against recorded responses.
//!
//! These run offline against a local stub, so they need no credential and no
//! network — which is what lets them sit in the primary suite and gate merge.
//! What they pin is not our code so much as our reading of someone else's API:
//! when Hevy changes, one of these fails instead of a run quietly landing
//! nothing.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use std::error::Error;

use application::{
    SourceError, WorkoutEventSource,
    paging::{PageCount, PageNumber},
};
use domain::landing::{EventKind, EventTime, Watermark};
use infrastructure::{HevyWorkoutEvents, RetryPolicy};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

type Fallible<T> = Result<T, Box<dyn Error>>;

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

fn source(base: &str) -> Fallible<HevyWorkoutEvents> {
    // No sleeping in tests: the retry *count* is the behaviour worth pinning,
    // and waiting to prove a delay exists only makes the suite slow.
    Ok(HevyWorkoutEvents::with_retry(
        base,
        "00000000-0000-0000-0000-000000000000",
        RetryPolicy::immediate(3),
    )?)
}

/// One update and one deletion, in the shape the live API serves them.
const POPULATED: &str = r#"{
  "page": 1,
  "page_count": 17,
  "events": [
    {"type":"updated","workout":{"id":"b459cba5","title":"Morning 💪","updated_at":"2026-08-10T19:29:47.199Z","exercises":[{"index":0,"superset_id":null}]}},
    {"type":"deleted","id":"93d50b8d","deleted_at":"2025-11-05T20:02:27.905Z"}
  ]
}"#;

// --- The one that would bite ------------------------------------------------

/// **The empty response uses a different key.** `workouts`, not `events`,
/// while the published schema marks `events` required and never mentions this
/// shape.
///
/// This is the steady state: every run after extraction has caught up receives
/// exactly this. A deserialiser written from the schema passes a first run and
/// fails every one after it — which is the worst kind of bug, because the
/// first run is the one anybody tests.
#[test]
fn an_empty_page_uses_the_workouts_key_and_is_not_an_error() {
    runtime().expect("a tokio runtime").block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/workouts/events"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"page":1,"page_count":1,"workouts":[]}"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let page = source(&server.uri())
            .expect("a source")
            .fetch_page(None, PageNumber::first())
            .await
            .expect("an empty page is a success, not a parse error");

        assert!(page.events.is_empty());
        assert_eq!(page.page_count, PageCount::new(1));
    });
}

/// The same must hold if the source ever drops the array entirely.
#[test]
fn a_page_with_neither_key_is_empty_rather_than_broken() {
    runtime().expect("a tokio runtime").block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"page":1,"page_count":1}"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let page = source(&server.uri())
            .expect("a source")
            .fetch_page(None, PageNumber::first())
            .await
            .expect("a missing array is an empty page");
        assert!(page.events.is_empty());
    });
}

// --- Splitting a page -------------------------------------------------------

/// FR-001 and FR-002 together: one record per workout, bytes untouched.
#[test]
fn a_page_splits_into_one_event_per_workout_with_bytes_intact() {
    runtime().expect("a tokio runtime").block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(POPULATED, "application/json"))
            .mount(&server)
            .await;

        let page = source(&server.uri())
            .expect("a source")
            .fetch_page(None, PageNumber::first())
            .await
            .expect("a populated page");

        assert_eq!(page.events.len(), 2);
        assert_eq!(page.page_count, PageCount::new(17));

        let update = page.events.first().expect("an update");
        assert_eq!(update.kind, EventKind::Updated);
        assert_eq!(update.source_record_id.as_str(), "b459cba5");
        assert_eq!(
            update.event_time,
            Some(EventTime::parse("2026-08-10T19:29:47.199Z").expect("valid"))
        );

        // The payload is the event object as served. `superset_id` is the
        // singular spelling the API actually uses, against the documented
        // `supersets_id` — proof that nothing here re-serialises a parsed
        // value, which would be free to normalise it away.
        let bytes = String::from_utf8(update.payload.as_bytes().to_vec()).expect("utf-8");
        assert!(bytes.contains("\"superset_id\""), "payload was: {bytes}");
        assert!(bytes.contains('💪'), "non-ascii must survive: {bytes}");
        assert!(bytes.starts_with('{') && bytes.ends_with('}'));

        // A deletion names its workout at the top level rather than inside a
        // body, and carries `deleted_at` as its event time.
        let deletion = page.events.get(1).expect("a deletion");
        assert_eq!(deletion.kind, EventKind::Deleted);
        assert_eq!(deletion.source_record_id.as_str(), "93d50b8d");
        assert_eq!(
            deletion.event_time,
            Some(EventTime::parse("2025-11-05T20:02:27.905Z").expect("valid"))
        );
    });
}

/// An unrecognised kind is landed with its kind verbatim rather than refused:
/// a kind the source adds later is unknown, not illegal.
#[test]
fn an_unrecognised_event_kind_survives() {
    runtime().expect("a tokio runtime").block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"page":1,"page_count":1,"events":[{"type":"archived","id":"w9"}]}"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let page = source(&server.uri())
            .expect("a source")
            .fetch_page(None, PageNumber::first())
            .await
            .expect("an unknown kind is still an event");

        let event = page.events.first().expect("one event");
        assert_eq!(event.kind.as_source_str(), "archived");
        assert_eq!(event.source_record_id.as_str(), "w9");
    });
}

/// FR-003: a record that cannot say what it is about is worse than a visible
/// failure, so the run fails rather than landing it.
#[test]
fn an_event_without_an_identifier_fails_the_run() {
    runtime().expect("a tokio runtime").block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"page":1,"page_count":1,"events":[{"type":"updated","workout":{"title":"nameless"}}]}"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let failure = source(&server.uri())
            .expect("a source")
            .fetch_page(None, PageNumber::first())
            .await
            .expect_err("an event with no identifier must fail");
        assert!(matches!(failure, SourceError::Malformed { .. }));
    });
}

// --- Request shape ----------------------------------------------------------

/// The stored watermark is passed through unmodified. `since` is inclusive at
/// the source, so the boundary event is re-served and deduplicated — no
/// epsilon, and no chance of skipping a sibling that shares its timestamp.
#[test]
fn the_watermark_is_sent_unmodified_and_the_epoch_stands_in_for_none() {
    runtime().expect("a tokio runtime").block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header("api-key", "00000000-0000-0000-0000-000000000000"))
            .and(query_param("since", "2026-08-10T19:29:47.199Z"))
            .and(query_param("page", "1"))
            // The source caps page size at 10 and rejects more with a 400.
            .and(query_param("pageSize", "10"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"page":1,"page_count":1,"workouts":[]}"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        let mark = Watermark::parse("2026-08-10T19:29:47.199Z").expect("valid");
        source(&server.uri())
            .expect("a source")
            .fetch_page(Some(mark), PageNumber::first())
            .await
            .expect("the watermark must be sent verbatim");
    });

    runtime().expect("a tokio runtime").block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(query_param("since", "1970-01-01T00:00:00Z"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                r#"{"page":1,"page_count":1,"workouts":[]}"#,
                "application/json",
            ))
            .mount(&server)
            .await;

        source(&server.uri())
            .expect("a source")
            .fetch_page(None, PageNumber::first())
            .await
            .expect("no watermark means the epoch");
    });
}

// --- Failure handling -------------------------------------------------------

/// Terminal, and never retried: a rejected credential will not un-reject
/// itself, and retrying looks like an attack. The body is the bare string
/// `InvalidApiKey` rather than JSON, so nothing may assume otherwise.
#[test]
fn a_rejected_credential_is_terminal_and_its_body_is_not_json() {
    runtime().expect("a tokio runtime").block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401).set_body_raw("InvalidApiKey", "text/plain"))
            .expect(1)
            .mount(&server)
            .await;

        let failure = source(&server.uri())
            .expect("a source")
            .fetch_page(None, PageNumber::first())
            .await
            .expect_err("401 must fail");
        assert_eq!(failure, SourceError::Unauthorised);
    });
}

/// Throttling and server faults are transient, so they are retried and only
/// then reported as the source being unavailable.
#[test]
fn throttling_is_retried_then_reported_as_unavailable() {
    runtime().expect("a tokio runtime").block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429))
            .expect(3)
            .mount(&server)
            .await;

        let failure = source(&server.uri())
            .expect("a source")
            .fetch_page(None, PageNumber::first())
            .await
            .expect_err("exhausted retries must fail");
        assert!(matches!(failure, SourceError::Unavailable { .. }));
    });
}

#[test]
fn a_server_fault_is_retried_and_then_succeeds() {
    runtime().expect("a tokio runtime").block_on(async {
        let server = MockServer::start().await;
        // First attempt fails, second succeeds. Mounted most-specific-first so
        // wiremock serves the 503 only once.
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(POPULATED, "application/json"))
            .mount(&server)
            .await;

        let page = source(&server.uri())
            .expect("a source")
            .fetch_page(None, PageNumber::first())
            .await
            .expect("a transient fault must be ridden out");
        assert_eq!(page.events.len(), 2);
    });
}

/// A 400 or a 404 is a fault in our request rather than a passing condition at
/// the source. Asking again gets the same answer, so it is not retried.
#[test]
fn a_bad_request_is_terminal_and_not_retried() {
    runtime().expect("a tokio runtime").block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(400).set_body_raw(
                r#"{"error":"pageSize must be less than or equal to 10"}"#,
                "application/json",
            ))
            .expect(1)
            .mount(&server)
            .await;

        let failure = source(&server.uri())
            .expect("a source")
            .fetch_page(None, PageNumber::first())
            .await
            .expect_err("400 must fail");
        assert!(matches!(failure, SourceError::Malformed { .. }));
    });
}

/// A body that is not JSON at all must not panic the adapter.
#[test]
fn an_unreadable_body_is_reported_rather_than_panicking() {
    runtime().expect("a tokio runtime").block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("<html>oops</html>", "text/html"))
            .mount(&server)
            .await;

        let failure = source(&server.uri())
            .expect("a source")
            .fetch_page(None, PageNumber::first())
            .await
            .expect_err("a non-JSON body must fail");
        assert!(matches!(failure, SourceError::Malformed { .. }));
    });
}

/// § 36: a source being unreachable degrades the system rather than failing
/// it. Here that means a typed error, not a panic and not a hang.
#[test]
fn an_unreachable_source_is_unavailable() {
    runtime().expect("a tokio runtime").block_on(async {
        // A port nothing is listening on: the connection is refused at once.
        let failure = source("http://127.0.0.1:1")
            .expect("a source")
            .fetch_page(None, PageNumber::first())
            .await
            .expect_err("an unreachable source must fail");
        assert!(matches!(failure, SourceError::Unavailable { .. }));
    });
}
