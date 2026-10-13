//! The devices the operator uses (#427), through the use case over the real
//! store: Garmin's recordings as the baseline, his statements over it.
//!
//! The serial numbers and dates are his; the recordings are built here, in the
//! shape his landed ones hold.

use application::{
    ExtractionRunLog as _, LandingStore,
    devices::{DeviceRegister, StatementRefused},
};
use domain::{
    device::{
        AddedDevice, Contradiction, DeviceIdentifier, DeviceName, DeviceStatement, InUse, LastUsed,
        Measurand, SerialNumber, sent_by,
    },
    landing::{
        Endpoint, EventKind, EventProvenance, FetchedAt, LandingRecord, LandingStream, RawPayload,
        SourceRecordId,
    },
    normalised::OperatorZone,
};
use infrastructure::{
    GarminActivityFileLandingStore, GarminActivityLandingStore, GarminRecordedDevices,
    SqliteDeviceStatementStore, SqliteExtractionRunLog, WithingsMeasurementLandingStore,
    WithingsRecordedDevices, connect,
};
use jiff::{Timestamp, civil::date};
use serde_json::json;
use sqlx::SqlitePool;

type Failure = Box<dyn std::error::Error + Send + Sync>;

const FORERUNNER_165: u32 = 3_483_897_389;
const VIVOACTIVE_3: u32 = 3_960_966_298;
const TICKR: u32 = 3_339_528_293;
const EDGE_530: u32 = 3_442_801_430;

/// A sensor a recording lists: its index, kind, maker, serial, product and link.
type Listed = (u8, u8, u16, u32, u16, u8);

/// A FIT recording naming what wrote it and what it lists.
fn recording(recorder: (u16, u16, u32), listed: &[Listed]) -> Vec<u8> {
    let mut messages = vec![
        0x40, 0x00, 0x00, 0, 0, 3, 1, 2, 0x84, 2, 2, 0x84, 3, 4, 0x8C, 0x00,
    ];
    messages.extend_from_slice(&recorder.0.to_le_bytes());
    messages.extend_from_slice(&recorder.1.to_le_bytes());
    messages.extend_from_slice(&recorder.2.to_le_bytes());
    messages.extend_from_slice(&[
        0x41, 0x00, 0x00, 23, 0, 6, 0, 1, 0x02, 1, 1, 0x02, 2, 2, 0x84, 3, 4, 0x8C, 4, 2, 0x84, 25,
        1, 0x00,
    ]);
    for (index, kind, maker, serial, product, link) in listed {
        messages.extend_from_slice(&[0x01, *index, *kind]);
        messages.extend_from_slice(&maker.to_le_bytes());
        messages.extend_from_slice(&serial.to_le_bytes());
        messages.extend_from_slice(&product.to_le_bytes());
        messages.push(*link);
    }
    let mut file = vec![12, 0x10, 0x00, 0x00];
    file.extend_from_slice(
        &u32::try_from(messages.len())
            .unwrap_or_default()
            .to_le_bytes(),
    );
    file.extend_from_slice(b".FIT");
    file.extend_from_slice(&messages);
    file
}

fn archive(recording: &[u8]) -> Result<Vec<u8>, Failure> {
    use std::io::Write as _;

    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer.start_file("1_ACTIVITY.fit", options)?;
    writer.write_all(recording)?;
    Ok(writer.finish()?.into_inner())
}

/// Land one activity and its recording.
async fn land(pool: &SqlitePool, id: &str, started: &str, recording: &[u8]) -> Result<(), Failure> {
    let runs = SqliteExtractionRunLog::new(pool.clone());
    let activity = serde_json::to_vec(&json!({ "activityId": 1, "startTimeGMT": started }))?;
    for (store, stream, path, payload) in [
        (
            0,
            "garmin.activities",
            "/activitylist-service/activities/search/activities",
            activity,
        ),
        (
            1,
            "garmin.activity_files",
            "/download-service/files/activity",
            archive(recording)?,
        ),
    ] {
        let record = LandingRecord::land(
            LandingStream::try_from(stream)?,
            FetchedAt::try_from("2026-10-13T08:00:00Z")?,
            SourceRecordId::try_from(id)?,
            EventProvenance::new(
                Endpoint::try_from(path)?,
                EventKind::try_from("updated")?,
                None,
            )
            .into(),
            RawPayload::try_from(payload)?,
        );
        if store == 0 {
            let landing = GarminActivityLandingStore::new(pool.clone())?;
            let run = runs.begin(landing.stream(), FetchedAt::EPOCH).await?;
            landing.append(run, vec![record]).await?;
        } else {
            let landing = GarminActivityFileLandingStore::new(pool.clone())?;
            let run = runs.begin(landing.stream(), FetchedAt::EPOCH).await?;
            landing.append(run, vec![record]).await?;
        }
    }
    Ok(())
}

/// One weigh-in on the Body Scan, as Withings serves its group.
async fn weigh(pool: &SqlitePool) -> Result<(), Failure> {
    let runs = SqliteExtractionRunLog::new(pool.clone());
    let group = serde_json::to_vec(&json!({
        "grpid": 7_269_264_664_i64,
        "attrib": 0,
        "date": 1_790_834_400_i64,
        "timezone": "Europe/London",
        "deviceid": "9883ca686c39eca2f2bad69a72e2e0e6af8f71fa",
        "hash_deviceid": "9883ca686c39eca2f2bad69a72e2e0e6af8f71fa",
        "model": "Body Scan",
        "modelid": 10,
        "measures": [
            { "value": 88_000, "type": 1, "unit": -3 },
            { "value": 61, "type": 11, "unit": 0 },
        ],
    }))?;
    let record = LandingRecord::land(
        LandingStream::try_from("withings.measurements")?,
        FetchedAt::try_from("2026-10-13T08:00:00Z")?,
        SourceRecordId::try_from("7269264664")?,
        EventProvenance::new(
            Endpoint::try_from("/measure")?,
            EventKind::try_from("updated")?,
            None,
        )
        .into(),
        RawPayload::try_from(group)?,
    );
    let landing = WithingsMeasurementLandingStore::new(pool.clone())?;
    let run = runs.begin(landing.stream(), FetchedAt::EPOCH).await?;
    landing.append(run, vec![record]).await?;
    Ok(())
}

/// A store holding the operator's recordings in the shapes that matter: the
/// Vivoactive 3 with the TICKR paired, the Forerunner 165 on its own wrist and
/// broadcasting to the Edge 530, a Peloton class peloton-to-garmin pushed in as
/// a "Forerunner 945", and a weigh-in on the Body Scan.
async fn store() -> Result<(SqlitePool, tempfile::TempDir), Failure> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;
    // The wrist sensor is onboard (link 5, kind 10); the strap is ANT+ (link 1,
    // kind 120).
    land(
        &pool,
        "1",
        "2024-06-30 15:52:20",
        &recording(
            (1, 2700, VIVOACTIVE_3),
            &[
                (0, 255, 1, VIVOACTIVE_3, 2700, 5),
                (1, 10, 0xFFFF, 0, 0xFFFF, 5),
                (2, 120, 32, TICKR, 7, 1),
            ],
        ),
    )
    .await?;
    land(
        &pool,
        "2",
        "2026-09-29 18:40:07",
        &recording(
            (1, 4432, FORERUNNER_165),
            &[
                (0, 255, 1, FORERUNNER_165, 4432, 5),
                (1, 10, 0xFFFF, 0, 0xFFFF, 5),
            ],
        ),
    )
    .await?;
    land(
        &pool,
        "3",
        "2026-02-11 20:35:22",
        &recording((1, 3113, 1), &[]),
    )
    .await?;
    land(
        &pool,
        "4",
        "2025-03-02 12:26:12",
        &recording(
            (1, 3121, EDGE_530),
            &[
                (0, 255, 1, EDGE_530, 3121, 5),
                (1, 120, 1, FORERUNNER_165, 255, 1),
            ],
        ),
    )
    .await?;
    weigh(&pool).await?;
    Ok((pool, directory))
}

type Recorded = (GarminRecordedDevices, WithingsRecordedDevices);

fn register(
    pool: &SqlitePool,
) -> Result<DeviceRegister<Recorded, SqliteDeviceStatementStore>, Failure> {
    let zone = OperatorZone::try_from("Europe/London")?;
    Ok(DeviceRegister::new(
        (
            GarminRecordedDevices::new(pool.clone(), zone.clone()),
            WithingsRecordedDevices::new(pool.clone(), zone)?,
        ),
        SqliteDeviceStatementStore::new(pool.clone()),
    ))
}

fn serial(number: u32) -> Result<DeviceIdentifier, Failure> {
    Ok(DeviceIdentifier::Serial(SerialNumber::try_from(
        number.to_string(),
    )?))
}

fn runtime() -> Result<tokio::runtime::Runtime, Failure> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}

fn at(text: &str) -> Timestamp {
    text.parse().unwrap_or(Timestamp::UNIX_EPOCH)
}

/// Each device the recordings name is one entry, named for what it is, and
/// measuring what it supplied: the strap the heart rate of the ride it was
/// paired for, the watch beside it nothing. A sync is no device. The scale is
/// named by the id Withings gave it.
#[test]
fn the_recordings_are_the_baseline() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = store().await.expect("a store");
        let devices = register(&pool)
            .expect("a register")
            .devices()
            .await
            .expect("the register");

        let names: Vec<(&str, Vec<Measurand>)> = devices
            .iter()
            .map(|device| {
                (
                    device.name().value.as_str(),
                    device.measures().iter().copied().collect(),
                )
            })
            .collect();
        assert_eq!(
            names,
            vec![
                // First used the same day, so ordered by identifier.
                ("Wahoo heart rate sensor", vec![Measurand::HeartRate]),
                ("Vivoactive 3", vec![]),
                ("Edge 530", vec![]),
                ("Forerunner 165", vec![Measurand::HeartRate]),
                ("Body Scan", vec![Measurand::HeartRate, Measurand::BodyMass]),
            ]
        );
        let watch = devices
            .iter()
            .find(|device| device.is(&serial(FORERUNNER_165).expect("a serial")))
            .expect("the watch");
        assert_eq!(watch.first_used().value, date(2025, 3, 2));
        assert!(watch.sends().value.contains(&Measurand::HeartRate));
        let scale = devices
            .iter()
            .find(|device| device.name().value.as_str() == "Body Scan")
            .expect("the scale");
        assert_eq!(
            scale.identifier().map(ToString::to_string).as_deref(),
            Some("issued/Withings/9883ca686c39eca2f2bad69a72e2e0e6af8f71fa")
        );
        assert!(scale.sends().value.is_empty());
    });
}

/// What the operator says is kept, survives being read back by another
/// register, and attributes the rides the record alone could not.
#[test]
fn a_statement_is_kept_and_applied() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = store().await.expect("a store");
        let watch = serial(FORERUNNER_165).expect("a serial");

        register(&pool)
            .expect("a register")
            .state(
                at("2026-10-13T20:00:00Z"),
                DeviceStatement::LastUsed {
                    device: watch.clone(),
                    last: LastUsed::StillInUse,
                },
            )
            .await
            .expect("the statement is kept");
        register(&pool)
            .expect("a register")
            .state(
                at("2026-10-13T20:01:00Z"),
                DeviceStatement::Named {
                    device: serial(TICKR).expect("a serial"),
                    name: DeviceName::try_from("Wahoo TICKR").expect("a name"),
                },
            )
            .await
            .expect("the statement is kept");

        let devices = register(&pool)
            .expect("a register")
            .devices()
            .await
            .expect("the register");
        let InUse::One(device) = sent_by(&devices, Measurand::HeartRate, date(2026, 10, 7)) else {
            panic!("the watch measured the ride of 2026-10-07");
        };
        assert!(device.is(&watch));
        // The morning of a weigh-in: the scale measured a pulse, and sent it
        // nowhere.
        let InUse::One(device) = sent_by(&devices, Measurand::HeartRate, date(2026, 10, 1)) else {
            panic!("the watch alone could have sent a ride's heart rate on 2026-10-01");
        };
        assert!(device.is(&watch));
        assert!(
            devices
                .iter()
                .any(|device| device.name().value.as_str() == "Wahoo TICKR")
        );
    });
}

/// A device no recording names is added, and numbered so later statements
/// can be addressed to it.
#[test]
fn a_device_is_added_and_can_be_spoken_of() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = store().await.expect("a store");
        let use_case = register(&pool).expect("a register");

        let devices = use_case
            .add(
                at("2026-10-13T20:00:00Z"),
                AddedDevice::new(
                    DeviceName::try_from("Bike+").expect("a name"),
                    [Measurand::Power, Measurand::Cadence, Measurand::Speed]
                        .into_iter()
                        .collect(),
                    date(2023, 12, 21),
                    LastUsed::StillInUse,
                )
                .expect("an ordered period"),
            )
            .await
            .expect("the device is added");
        let bike = devices
            .iter()
            .find(|device| device.name().value.as_str() == "Bike+")
            .and_then(|device| device.identifier().cloned())
            .expect("the bike is in the register");
        assert_eq!(bike.to_string(), "stated/1");

        let devices = use_case
            .state(
                at("2026-10-13T20:05:00Z"),
                DeviceStatement::LastUsed {
                    device: bike.clone(),
                    last: LastUsed::On(date(2026, 10, 13)),
                },
            )
            .await
            .expect("the statement is kept");
        let held = devices
            .iter()
            .find(|device| device.is(&bike))
            .expect("the bike");
        assert_eq!(held.last_used().value, LastUsed::On(date(2026, 10, 13)));
    });
}

/// A statement the register could not apply is refused and not kept.
#[test]
fn a_contradiction_is_refused_and_not_kept() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = store().await.expect("a store");
        let use_case = register(&pool).expect("a register");

        let refused = use_case
            .state(
                at("2026-10-13T20:00:00Z"),
                DeviceStatement::LastUsed {
                    device: serial(FORERUNNER_165).expect("a serial"),
                    last: LastUsed::On(date(2024, 1, 1)),
                },
            )
            .await;

        assert!(matches!(
            refused,
            Err(StatementRefused::Contradiction(
                Contradiction::LastBeforeFirst { .. }
            ))
        ));
        let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM device_statement")
            .fetch_one(&pool)
            .await
            .expect("a count");
        assert_eq!(kept, 0);
    });
}
