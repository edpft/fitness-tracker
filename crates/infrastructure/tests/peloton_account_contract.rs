//! The contract with Peloton's account, pinned against a local stub (#427),
//! and with each workout's detail (#271).
//!
//! **What is kept is the account as served**, and what the register reads from
//! it is the heart rate monitors the Bike+ is paired with, each by its ANT+
//! number. The shape is the one `/api/me` answered for the operator on
//! 2026-10-13; every value here is invented except the monitor's number, which
//! is the low sixteen bits of his Forerunner 165's serial.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use application::WorkoutEventSource as _;
use domain::{
    device::{DeviceIdentifier, Measurand},
    landing::{
        Endpoint, EventKind, EventProvenance, FetchedAt, LandedRecord, LandingRecord,
        LandingRecordId, LandingStream, RawPayload, SourceRecordId, Watermark,
    },
    normalised::OperatorZone,
};
use infrastructure::peloton::{
    PelotonWorkoutDetails,
    auth::{PelotonAuth, PelotonCredentials},
    paired::{PelotonAccount, sightings},
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

/// A token endpoint that answers, so the search can be reached.
async fn authenticated(server: &MockServer) {
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
            "token_type": "Bearer",
            "expires_in": 172_800,
        })))
        .mount(server)
        .await;
}

const USER: &str = "f051be7977044fd39797c05a854a6f59";

fn account() -> serde_json::Value {
    serde_json::json!({
        "id": USER,
        "username": "rider",
        "total_workouts": 456,
        "paired_devices": [
            { "name": "Device: 3629", "paired_device_type": "heart_rate_monitor", "serial_number": "3629" },
            { "name": "[TV] A speaker", "paired_device_type": "audio", "serial_number": "aa:bb:cc:dd:ee" },
        ],
    })
}

/// One request, one record: the account under the user's id, as served.
#[test]
fn the_account_is_one_record_under_the_users_id() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;
        Mock::given(method("GET"))
            .and(path("/api/me"))
            .respond_with(ResponseTemplate::new(200).set_body_json(account()))
            .expect(1)
            .mount(&server)
            .await;

        let source = PelotonAccount::new(
            server.uri(),
            PelotonAuth::new(
                server.uri(),
                PelotonCredentials::new("rider@example.com", "not-a-real-password"),
                None,
            ),
        );
        let batch = source.fetch(None, None).await.expect("the account");

        assert!(batch.resume.is_none());
        let [event] = batch.events.as_slice() else {
            panic!("one record");
        };
        assert_eq!(event.source_record_id.as_str(), USER);
        let answered: serde_json::Value =
            serde_json::from_slice(event.payload.as_bytes()).expect("JSON");
        assert_eq!(answered, account());
    });
}

/// The monitor is read as the device that broadcasts by that number, sending
/// heart rate, in use when the account was fetched; a speaker is no monitor.
#[test]
fn a_paired_monitor_is_the_device_with_that_number() {
    let landed = LandedRecord::new(
        LandingRecordId::try_from(1).expect("an id"),
        LandingRecord::land(
            LandingStream::try_from("peloton.account").expect("a stream"),
            FetchedAt::try_from("2026-10-13T19:00:00Z").expect("an instant"),
            SourceRecordId::try_from(USER).expect("an id"),
            EventProvenance::new(
                Endpoint::try_from("/api/me").expect("an endpoint"),
                EventKind::Updated,
                None,
            )
            .into(),
            RawPayload::try_from(serde_json::to_vec(&account()).expect("bytes"))
                .expect("a payload"),
        ),
    );

    let read = sightings(
        &landed,
        &OperatorZone::try_from("Europe/London").expect("a zone"),
    );

    let [monitor] = read.as_slice() else {
        panic!("one monitor");
    };
    assert_eq!(monitor.device.to_string(), "ant/3629");
    assert!(matches!(monitor.device, DeviceIdentifier::Ant(_)));
    assert!(monitor.sent.contains(&Measurand::HeartRate));
    assert_eq!(
        monitor.at.wall_clock().date(),
        jiff::civil::date(2026, 10, 13)
    );
}

fn authenticator(server: &MockServer) -> PelotonAuth {
    PelotonAuth::new(
        server.uri(),
        PelotonCredentials::new("rider@example.com", "not-a-real-password"),
        None,
    )
}

/// **A detail from before the resumption point is not asked for**, as with a
/// graph; the boundary is, because the port's `since` is inclusive. Each
/// detail is filed under its workout's id with the workout's time.
#[test]
fn a_detail_is_fetched_only_for_a_workout_the_run_reaches() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let server = MockServer::start().await;
        authenticated(&server).await;
        Mock::given(method("GET"))
            .and(path("/api/me"))
            .respond_with(ResponseTemplate::new(200).set_body_json(account()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/api/user/{USER}/workouts")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "id": "new", "created_at": 2_000 },
                    { "id": "boundary", "created_at": 1_000 },
                    { "id": "old", "created_at": 999 },
                ],
                "show_next": true,
            })))
            .mount(&server)
            .await;
        for (id, ftp) in [("new", 177), ("boundary", 173)] {
            Mock::given(method("GET"))
                .and(path(format!("/api/workout/{id}")))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({ "id": id, "ftp_info": { "ftp": ftp } })),
                )
                .expect(1)
                .mount(&server)
                .await;
        }
        Mock::given(method("GET"))
            .and(path("/api/workout/old"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;

        let source = PelotonWorkoutDetails::new(server.uri(), authenticator(&server));
        let since = jiff::Timestamp::from_second(1_000)
            .ok()
            .map(Watermark::from);
        let batch = source.fetch(since, None).await.expect("the details");

        assert!(
            batch.resume.is_none(),
            "the walk stops at the resumption point"
        );
        let landed: Vec<(&str, Option<i64>)> = batch
            .events
            .iter()
            .map(|event| {
                (
                    event.source_record_id.as_str(),
                    event
                        .provenance
                        .occurred_at()
                        .map(|at| at.as_timestamp().as_second()),
                )
            })
            .collect();
        assert_eq!(landed, [("new", Some(2_000)), ("boundary", Some(1_000))]);
        let [new, _] = batch.events.as_slice() else {
            panic!("two details");
        };
        assert_eq!(
            new.provenance
                .as_event()
                .map(|event| event.endpoint().as_str()),
            Some("/api/workout/new"),
            "the detail's own path, not the list's"
        );
    });
}
