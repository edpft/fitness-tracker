//! The walk of Garmin's exercise sets, pinned against a local stub.
//!
//! What is pinned is which activities are asked about and what a second run
//! does, not what a set list holds: the stub's sets are shaped for the test,
//! and what Garmin actually serves is read off the operator's store (#173).
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use application::{
    Clock, LandingStore as _, WorkoutExtractor as _,
    extract::{Extraction, ExtractionPorts},
};
use domain::landing::FetchedAt;
use infrastructure::{
    FileRunLock, GarminAuth, GarminCredentials, GarminExerciseSetLandingStore,
    SqliteExtractionRunLog, SqliteResumptionPointStore, connect,
    garmin::{GarminExerciseSets, token_base_for},
};
use wiremock::{
    Mock, MockServer, Request, Respond, ResponseTemplate,
    matchers::{method, path},
};

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> FetchedAt {
        FetchedAt::EPOCH
    }
}

/// A gym session the list says has sets, at a second.
fn with_sets(id: i64, second: i64) -> serde_json::Value {
    serde_json::json!({
        "activityId": id,
        "beginTimestamp": second * 1000,
        "activityType": { "typeKey": "strength_training" },
        "summarizedExerciseSets": [
            { "category": "SQUAT", "sets": 3, "reps": 24, "volume": 2_880_000.0 }
        ],
    })
}

/// An activity the list says has none: a ride, whose summary is empty.
fn without_sets(id: i64, second: i64) -> serde_json::Value {
    serde_json::json!({
        "activityId": id,
        "beginTimestamp": second * 1000,
        "activityType": { "typeKey": "cycling" },
        "summarizedExerciseSets": [],
    })
}

/// The list: three activities on the first walk, and [`LATER`] prepended after
/// that, as a list served newest first would be once a session had been done.
struct ActivityList {
    walks: Arc<AtomicUsize>,
}

const LATER: i64 = 4;
const BOUNDARY: i64 = 1;
const OLDER: i64 = 3;

impl Respond for ActivityList {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let start = request
            .url
            .query_pairs()
            .find(|(name, _)| name == "start")
            .map(|(_, value)| value.into_owned());
        if start.as_deref() != Some("0") {
            return ResponseTemplate::new(200).set_body_json(serde_json::json!([]));
        }

        let first = vec![
            with_sets(BOUNDARY, 2_000),
            without_sets(2, 1_500),
            with_sets(OLDER, 1_000),
        ];
        let body = if self.walks.fetch_add(1, Ordering::SeqCst) == 0 {
            first
        } else {
            let mut later = vec![with_sets(LATER, 3_000)];
            later.extend(first);
            later
        };
        ResponseTemplate::new(200).set_body_json(body)
    }
}

/// One activity's sets, in a different key order every other time it is
/// served — which is what Garmin does to the list, and so what a second run
/// has to see through.
struct SetList {
    served: AtomicUsize,
}

impl Respond for SetList {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let body = if self.served.fetch_add(1, Ordering::SeqCst).is_multiple_of(2) {
            r#"{"activityId":1,"exerciseSets":[{"setType":"ACTIVE","repetitionCount":8}]}"#
        } else {
            r#"{"exerciseSets":[{"repetitionCount":8,"setType":"ACTIVE"}],"activityId":1}"#
        };
        ResponseTemplate::new(200).set_body_string(body)
    }
}

async fn stub() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sso/embed"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html></html>"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/sso/signin"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                r#"<form><input type="hidden" name="_csrf" value="CSRF" /></form>"#,
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/sso/signin"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"<title>Success</title><script>response_url = "{}/sso/embed?ticket=ST-1-t-cas";</script>"#,
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
    Mock::given(method("GET"))
        .and(path("/activitylist-service/activities/search/activities"))
        .respond_with(ActivityList {
            walks: Arc::new(AtomicUsize::new(0)),
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/activity-service/activity/{BOUNDARY}/exerciseSets"
        )))
        .respond_with(SetList {
            served: AtomicUsize::new(0),
        })
        .mount(&server)
        .await;
    for id in [OLDER, LATER] {
        Mock::given(method("GET"))
            .and(path(format!(
                "/activity-service/activity/{id}/exerciseSets"
            )))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(format!(r#"{{"activityId":{id},"exerciseSets":[]}}"#)),
            )
            .mount(&server)
            .await;
    }
    server
}

/// Which activities' sets were asked for, in the order they were.
async fn asked_about(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|request| request.url.path().to_owned())
        .filter(|path| path.ends_with("/exerciseSets"))
        .collect()
}

fn sets_of(id: i64) -> String {
    format!("/activity-service/activity/{id}/exerciseSets")
}

/// **The list decides, and a second run lands nothing it already had.**
///
/// The first run asks about the two sessions whose summary names sets and not
/// the ride whose summary is empty. The second, after a new session: the new
/// one is asked about, the boundary again because the port's `since` is
/// inclusive — and its answer, served in another key order, lands nothing —
/// and the session older than the resumption point not at all.
#[test]
fn only_what_the_list_names_is_asked_for_and_only_once() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let server = stub().await;
        let directory = tempfile::tempdir().expect("a directory");
        let database = directory.path().join("test.db");
        let pool = connect(&database).await.expect("a store");

        let extraction = || {
            let base = server.uri();
            Extraction::new(ExtractionPorts {
                source: GarminExerciseSets::new(
                    base.clone(),
                    GarminAuth::new(
                        base.clone(),
                        token_base_for(&base),
                        GarminCredentials::new("rider@example.com", "not-a-real-password"),
                        None,
                    )
                    .without_pause(),
                ),
                landing: GarminExerciseSetLandingStore::new(pool.clone()).expect("a landing store"),
                resumption: SqliteResumptionPointStore::new(pool.clone()),
                runs: SqliteExtractionRunLog::new(pool.clone()),
                lock: FileRunLock::beside(&database),
                clock: FixedClock,
            })
        };

        let first = extraction().extract().await.expect("a first run");
        assert_eq!(first.records_landed.as_usize(), 2);
        assert_eq!(
            asked_about(&server).await,
            [sets_of(BOUNDARY), sets_of(OLDER)],
            "the ride's summary is empty, so its sets are not asked for"
        );

        let second = extraction().extract().await.expect("a second run");
        assert_eq!(
            second.records_landed.as_usize(),
            1,
            "only the new session's sets are new; the boundary's came back reordered"
        );
        assert_eq!(
            asked_about(&server).await,
            [
                sets_of(BOUNDARY),
                sets_of(OLDER),
                sets_of(LATER),
                sets_of(BOUNDARY)
            ],
            "the session older than the resumption point is not asked about again"
        );

        let landing = GarminExerciseSetLandingStore::new(pool.clone()).expect("a landing store");
        assert_eq!(landing.count().await.expect("a count").as_usize(), 3);
    });
}
