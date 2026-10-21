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
use jiff::civil::date;
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

/// A night, as served for whichever date was asked.
struct Night;

impl Respond for Night {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let asked = request
            .url
            .query_pairs()
            .find(|(key, _)| key == "date")
            .map(|(_, value)| value.into_owned())
            .unwrap_or_default();
        ResponseTemplate::new(200).set_body_string(format!(
            r#"{{"dailySleepDTO":{{"calendarDate":"{asked}","sleepTimeSeconds":27000}}}}"#
        ))
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
        .respond_with(Night)
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

/// **Every day from the first wrist sensor to today, under the account's name,
/// and a second run lands nothing it already had.**
///
/// The first run asks for each of the three days and lands each. The second
/// asks again only for the last, because a day's answer grows until it is
/// over, and the same answer lands nothing.
#[test]
fn every_day_is_asked_for_once_and_today_again() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let server = stub().await;
        let directory = tempfile::tempdir().expect("a directory");
        let database = directory.path().join("test.db");
        let pool = connect(&database).await.expect("a store");

        let extraction = || {
            let base = server.uri();
            Extraction::new(ExtractionPorts {
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
                    date(2018, 1, 8),
                ),
                landing: GarminSleepLandingStore::new(pool.clone()).expect("a landing store"),
                resumption: SqliteResumptionPointStore::new(pool.clone()),
                runs: SqliteExtractionRunLog::new(pool.clone()),
                lock: FileRunLock::beside(&database),
                clock: FixedClock,
            })
        };

        let first = extraction().extract().await.expect("a first run");
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

        let second = extraction().extract().await.expect("a second run");
        assert_eq!(
            second.records_landed.as_usize(),
            0,
            "nothing new was served"
        );
        assert_eq!(
            asked(&server).await.0,
            ["2018-01-06", "2018-01-07", "2018-01-08", "2018-01-08"],
            "only today again"
        );

        let landing = GarminSleepLandingStore::new(pool.clone()).expect("a landing store");
        assert_eq!(landing.count().await.expect("a count").as_usize(), 3);
    });
}
