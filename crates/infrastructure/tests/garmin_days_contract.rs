//! The walk of Garmin's daily metrics, pinned against a local stub (#269).
//!
//! What is pinned is which days are asked for, under which account, and what a
//! second run does, not what a night's sleep holds: the stub's answers are
//! shaped for the test.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use application::{
    Clock, LandingStore as _, WorkoutExtractor as _,
    extract::{Extraction, ExtractionPorts},
};
use domain::landing::FetchedAt;
use infrastructure::{
    FileRunLock, GarminAuth, GarminCredentials, GarminSleepLandingStore, SqliteExtractionRunLog,
    SqliteResumptionPointStore, connect,
    garmin::{
        days::{GarminDay, GarminDays},
        token_base_for,
    },
};
use std::sync::{Arc, Mutex};

use jiff::civil::{Date, date};
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

/// The account's name, which is not the email it signs in with.
const NAME: &str = "someone";

/// When the watch last synced, as Garmin would state it: changed by a test
/// between runs.
type Synced = Arc<Mutex<String>>;

fn synced_at(synced: &Synced) -> String {
    synced.lock().map(|at| at.clone()).unwrap_or_default()
}

/// A night, as served for whichever date was asked: what Garmin holds of it
/// grows with each sync.
struct Night(Synced);

impl Respond for Night {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let asked = request
            .url
            .query_pairs()
            .find(|(key, _)| key == "date")
            .map(|(_, value)| value.into_owned())
            .unwrap_or_default();
        ResponseTemplate::new(200).set_body_string(format!(
            r#"{{"dailySleepDTO":{{"calendarDate":"{asked}","sleepTimeSeconds":27000}},"synced":"{}"}}"#,
            synced_at(&self.0)
        ))
    }
}

/// The day's summary, which is where the sync is stated.
struct Summary(Synced);

impl Respond for Summary {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "lastSyncTimestampGMT": synced_at(&self.0),
        }))
    }
}

async fn stub(synced: &Synced) -> MockServer {
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
        .and(path("/userprofile-service/socialProfile"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "displayName": NAME })),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/wellness-service/wellness/dailySleepData/{NAME}"
        )))
        .respond_with(Night(Arc::clone(synced)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/usersummary-service/usersummary/daily/{NAME}"
        )))
        .respond_with(Summary(Arc::clone(synced)))
        .mount(&server)
        .await;
    server
}

/// The nights asked for, in order, and how often the account's name was.
async fn asked(server: &MockServer) -> (Vec<String>, usize) {
    let requests = server.received_requests().await.unwrap_or_default();
    let nights = requests
        .iter()
        .filter(|request| request.url.path().contains("/dailySleepData/"))
        .map(|request| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == "date")
                .map(|(_, value)| value.into_owned())
                .unwrap_or_default()
        })
        .collect();
    let profiles = requests
        .iter()
        .filter(|request| request.url.path() == "/userprofile-service/socialProfile")
        .count();
    (nights, profiles)
}

/// A run of the sleep walk against the stub, as of `today`.
fn extraction(
    server: &MockServer,
    pool: &sqlx::SqlitePool,
    database: &std::path::Path,
    today: Date,
) -> Result<impl application::WorkoutExtractor, Box<dyn std::error::Error>> {
    let base = server.uri();
    Ok(Extraction::new(ExtractionPorts {
        source: GarminDays::new(
            GarminDay::Sleep,
            base.clone(),
            GarminAuth::new(
                base.clone(),
                token_base_for(&base),
                GarminCredentials::new("sleeper@example.com", "not-a-real-password"),
                None,
            )
            .without_pause(),
            today,
        ),
        landing: GarminSleepLandingStore::new(pool.clone())?,
        resumption: SqliteResumptionPointStore::new(pool.clone()),
        runs: SqliteExtractionRunLog::new(pool.clone()),
        lock: FileRunLock::beside(database),
        clock: FixedClock,
    }))
}

/// **Every day from the first wrist sensor to today, under the account's name,
/// and a second run lands nothing it already had.**
///
/// The first run asks for each of the three days and lands each. The watch
/// synced on the morning of the last, so the second asks again only for that
/// one, and the same answer lands nothing.
#[test]
fn every_day_is_asked_for_once_and_the_synced_day_again() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let synced: Synced = Arc::new(Mutex::new("2018-01-08T08:00:00.0".to_owned()));
        let server = stub(&synced).await;
        let directory = tempfile::tempdir().expect("a directory");
        let database = directory.path().join("test.db");
        let pool = connect(&database).await.expect("a store");
        let today = date(2018, 1, 8);

        let first = extraction(&server, &pool, &database, today)
            .expect("a run")
            .extract()
            .await
            .expect("a first run");
        assert_eq!(first.records_landed.as_usize(), 3);
        assert_eq!(
            asked(&server).await,
            (
                vec![
                    "2018-01-06".to_owned(),
                    "2018-01-07".to_owned(),
                    "2018-01-08".to_owned()
                ],
                1
            ),
            "each day once, and the account's name once for the walk"
        );

        let second = extraction(&server, &pool, &database, today)
            .expect("a run")
            .extract()
            .await
            .expect("a second run");
        assert_eq!(
            second.records_landed.as_usize(),
            0,
            "nothing new was served"
        );
        assert_eq!(
            asked(&server).await.0,
            ["2018-01-06", "2018-01-07", "2018-01-08", "2018-01-08"],
            "only the day of the sync again"
        );

        let landing = GarminSleepLandingStore::new(pool.clone()).expect("a landing store");
        assert_eq!(landing.count().await.expect("a count").as_usize(), 3);
    });
}

/// **A day landed before the watch synced it is asked for again, and lands
/// whole** (#528).
///
/// The first run is on the 8th, but the watch last synced on the morning of
/// the 7th, so the 7th and the 8th land part-done. By the second run, on the
/// 9th, it has synced again: the walk starts from the 7th rather than the 8th,
/// and both land as Garmin now holds them.
#[test]
fn a_day_landed_before_the_sync_is_landed_again_whole() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let synced: Synced = Arc::new(Mutex::new("2018-01-07T10:00:00.0".to_owned()));
        let server = stub(&synced).await;
        let directory = tempfile::tempdir().expect("a directory");
        let database = directory.path().join("test.db");
        let pool = connect(&database).await.expect("a store");

        let first = extraction(&server, &pool, &database, date(2018, 1, 8))
            .expect("a run")
            .extract()
            .await
            .expect("a first run");
        assert_eq!(first.records_landed.as_usize(), 3);

        *synced.lock().expect("the sync") = "2018-01-09T07:00:00.0".to_owned();
        let second = extraction(&server, &pool, &database, date(2018, 1, 9))
            .expect("a run")
            .extract()
            .await
            .expect("a second run");
        assert_eq!(
            asked(&server).await.0,
            [
                "2018-01-06",
                "2018-01-07",
                "2018-01-08",
                "2018-01-07",
                "2018-01-08",
                "2018-01-09"
            ],
            "from the day of the last sync the first run saw"
        );
        assert_eq!(
            second.records_landed.as_usize(),
            3,
            "the 7th and 8th whole, and the 9th"
        );

        let third = extraction(&server, &pool, &database, date(2018, 1, 9))
            .expect("a run")
            .extract()
            .await
            .expect("a third run");
        assert_eq!(third.records_landed.as_usize(), 0);
        assert_eq!(
            asked(&server).await.0.last().map(String::as_str),
            Some("2018-01-09"),
            "the days before the sync are not asked for again"
        );
    });
}
