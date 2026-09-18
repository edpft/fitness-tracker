//! The walk of Garmin's activity files, pinned against a local stub.
//!
//! What is pinned is that every activity's file is asked for and that its bytes
//! land exactly as served — an archive is not text, and reading it as text
//! would corrupt it. What a real file holds is read off the operator's store
//! (#175).
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use application::{
    Clock, LandingStore as _, WorkoutExtractor as _,
    extract::{Extraction, ExtractionPorts},
};
use domain::landing::FetchedAt;
use infrastructure::{
    FileRunLock, GarminActivityFileLandingStore, GarminAuth, GarminCredentials,
    SqliteExtractionRunLog, SqliteResumptionPointStore, connect,
    garmin::{GarminActivityFiles, token_base_for},
};
use sqlx::Row as _;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
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

/// A zip's own signature followed by bytes that are not UTF-8.
const ARCHIVE: &[u8] = &[0x50, 0x4b, 0x03, 0x04, 0xff, 0xfe, 0x00, 0x80, 0xc3];

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
        .and(query_param("start", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "activityId": 1, "beginTimestamp": 2_000_000,
              "activityType": { "typeKey": "strength_training" } },
            { "activityId": 2, "beginTimestamp": 1_000_000,
              "activityType": { "typeKey": "cycling" } },
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/activitylist-service/activities/search/activities"))
        .and(query_param("start", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;
    // **The failure the first live run died on**: one 504, then the file.
    Mock::given(method("GET"))
        .and(path("/download-service/files/activity/2"))
        .respond_with(ResponseTemplate::new(504).set_body_string("error code: 504"))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    for id in [1, 2] {
        Mock::given(method("GET"))
            .and(path(format!("/download-service/files/activity/{id}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(ARCHIVE))
            .mount(&server)
            .await;
    }
    server
}

/// **Every activity, whatever its type, and the bytes as served** — including
/// the one whose first request the server failed.
#[test]
fn every_activity_s_file_lands_byte_for_byte() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let server = stub().await;
        let directory = tempfile::tempdir().expect("a directory");
        let database = directory.path().join("test.db");
        let pool = connect(&database).await.expect("a store");

        let base = server.uri();
        let summary = Extraction::new(ExtractionPorts {
            source: GarminActivityFiles::new(
                base.clone(),
                GarminAuth::new(
                    base.clone(),
                    token_base_for(&base),
                    GarminCredentials::new("rider@example.com", "not-a-real-password"),
                    None,
                )
                .without_pause(),
            ),
            landing: GarminActivityFileLandingStore::new(pool.clone()).expect("a landing store"),
            resumption: SqliteResumptionPointStore::new(pool.clone()),
            runs: SqliteExtractionRunLog::new(pool.clone()),
            lock: FileRunLock::beside(&database),
            clock: FixedClock,
        })
        .extract()
        .await
        .expect("a run");

        assert_eq!(
            summary.records_landed.as_usize(),
            2,
            "the ride's file is collected as the gym session's is"
        );

        let landed: Vec<Vec<u8>> =
            sqlx::query("SELECT payload FROM garmin_activity_file_landing ORDER BY id")
                .fetch_all(&pool)
                .await
                .expect("the landed files")
                .iter()
                .map(|row| row.get::<Vec<u8>, _>("payload"))
                .collect();
        assert_eq!(landed, [ARCHIVE.to_vec(), ARCHIVE.to_vec()]);

        let landing = GarminActivityFileLandingStore::new(pool).expect("a landing store");
        assert_eq!(landing.count().await.expect("a count").as_usize(), 2);
    });
}
