//! Which device sent a Bike+ ride's heart rate (#427), through the use case
//! over the real store: Garmin's recordings and Peloton's account.
//!
//! The serial numbers and dates are the operator's; the recordings are built
//! here, in the shape his landed ones hold.

use application::{ExtractionRunLog as _, LandingStore, devices::DeviceRegister};
use domain::{
    device::{Device, DeviceIdentifier, InUse, Measurand, Register, SerialNumber},
    landing::{
        Endpoint, EventKind, EventProvenance, FetchedAt, LandingRecord, LandingStream, RawPayload,
        SourceRecordId,
    },
    normalised::OperatorZone,
};
use infrastructure::{
    GarminActivityFileLandingStore, GarminActivityLandingStore, GarminDailyHeartRateLandingStore,
    GarminRecordedDevices, GarminWear, PelotonAccountLandingStore, PelotonRecordedDevices,
    SqliteExtractionRunLog, connect,
};
use jiff::civil::date;
use serde_json::json;
use sqlx::SqlitePool;

type Failure = Box<dyn std::error::Error + Send + Sync>;

const FORERUNNER_165: u32 = 3_483_897_389;
const VIVOACTIVE_3: u32 = 3_960_966_298;
const TICKR: u32 = 3_339_528_293;
/// The TICKR as Garmin records it over Bluetooth: the serial it reports,
/// 07211011341 in Wahoo's app, less its first two digits.
const TICKR_BLUETOOTH: u32 = 211_011_341;
const EDGE_530: u32 = 3_442_801_430;
const FORERUNNER_310XT: u32 = 3_820_129_853;

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

/// Peloton's account as it answered on 2026-10-13: the Bike+ paired with
/// monitor 3629.
async fn paired(pool: &SqlitePool) -> Result<(), Failure> {
    let runs = SqliteExtractionRunLog::new(pool.clone());
    let account = serde_json::to_vec(&json!({
        "id": "f051be7977044fd39797c05a854a6f59",
        "paired_devices": [
            { "name": "Device: 3629", "paired_device_type": "heart_rate_monitor", "serial_number": "3629" },
        ],
    }))?;
    let record = LandingRecord::land(
        LandingStream::try_from("peloton.account")?,
        FetchedAt::try_from("2026-10-13T19:00:00Z")?,
        SourceRecordId::try_from("f051be7977044fd39797c05a854a6f59")?,
        EventProvenance::new(Endpoint::try_from("/api/me")?, EventKind::Updated, None).into(),
        RawPayload::try_from(account)?,
    );
    let landing = PelotonAccountLandingStore::new(pool.clone())?;
    let run = runs.begin(landing.stream(), FetchedAt::EPOCH).await?;
    landing.append(run, vec![record]).await?;
    Ok(())
}

/// A store holding the operator's recordings in the shapes that matter: the
/// Vivoactive 3 with the TICKR paired, the Forerunner 165 on its own wrist and
/// broadcasting to the Edge 530, a Peloton class pushed in as a "Forerunner
/// 945", the 310XT once with a date and once with its placeholder, and
/// Peloton's account.
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
    land(
        &pool,
        "5",
        "2007-04-01 00:00:51",
        &recording((1, 1018, FORERUNNER_310XT), &[]),
    )
    .await?;
    land(
        &pool,
        "6",
        "2014-12-04 07:42:19",
        &recording((1, 1018, FORERUNNER_310XT), &[]),
    )
    .await?;
    paired(&pool).await?;
    Ok((pool, directory))
}

async fn devices(pool: &SqlitePool) -> Result<Register, Failure> {
    let zone = OperatorZone::try_from("Europe/London")?;
    Ok(DeviceRegister::new(
        (
            GarminRecordedDevices::new(pool.clone(), zone.clone()),
            PelotonRecordedDevices::new(pool.clone(), zone),
        ),
        GarminWear::new(pool.clone()),
    )
    .register()
    .await?)
}

/// One day of Garmin's daily heart rate: worn, with a resting heart rate, or
/// not, with nothing.
async fn day(pool: &SqlitePool, date: &str, worn: bool) -> Result<(), Failure> {
    let runs = SqliteExtractionRunLog::new(pool.clone());
    let payload = serde_json::to_vec(&if worn {
        json!({ "calendarDate": date, "restingHeartRate": 55, "heartRateValues": [[1, 60]] })
    } else {
        json!({ "calendarDate": date, "restingHeartRate": null, "heartRateValues": null })
    })?;
    let record = LandingRecord::land(
        LandingStream::try_from("garmin.daily_heart_rate")?,
        FetchedAt::try_from("2026-10-13T08:00:00Z")?,
        SourceRecordId::try_from(date)?,
        EventProvenance::new(
            Endpoint::try_from("/wellness-service/wellness/dailyHeartRate")?,
            EventKind::Updated,
            None,
        )
        .into(),
        RawPayload::try_from(payload)?,
    );
    let landing = GarminDailyHeartRateLandingStore::new(pool.clone())?;
    let run = runs.begin(landing.stream(), FetchedAt::EPOCH).await?;
    landing.append(run, vec![record]).await?;
    Ok(())
}

fn find(register: &Register, serial: u32) -> Option<&Device> {
    let identifier = DeviceIdentifier::Serial(SerialNumber::try_from(serial.to_string()).ok()?);
    register
        .devices()
        .iter()
        .find(|device| device.is(&identifier))
}

fn runtime() -> Result<tokio::runtime::Runtime, Failure> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}

/// Each device the record identifies is one entry, named for what it is: the
/// Forerunner 165 once, though Garmin names it by serial and Peloton by ANT+
/// number. A sync is no device.
#[test]
fn every_device_is_one_entry() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = store().await.expect("a store");
        let devices = devices(&pool).await.expect("the register");

        let names: Vec<&str> = devices
            .devices()
            .iter()
            .map(|device| device.name().as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "Forerunner 310XT",
                // First used the same day, so ordered by identifier.
                "TICKR",
                "Vivoactive 3",
                "Edge 530",
                "Forerunner 165",
            ]
        );
        let watch = find(&devices, FORERUNNER_165).expect("the watch");
        assert_eq!(watch.first_used(), date(2025, 3, 2));
        assert_eq!(watch.last_used(), date(2026, 10, 13));
    });
}

/// A recording the watch dated with its placeholder says nothing about when
/// the watch was in use.
#[test]
fn a_placeholder_date_is_no_date() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = store().await.expect("a store");
        let devices = devices(&pool).await.expect("the register");

        let watch = find(&devices, FORERUNNER_310XT).expect("the 310XT");
        assert_eq!(watch.first_used(), date(2014, 12, 4));
    });
}

/// Both watches broadcast heart rate, which is the model's, not something a
/// recording shows: the Vivoactive 3 is a candidate beside the TICKR it was
/// paired with that day. The Edge 530 sends nothing.
#[test]
fn a_watch_that_broadcasts_is_a_candidate() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = store().await.expect("a store");
        let devices = devices(&pool).await.expect("the register");

        let InUse::Several(candidates) = devices.sent_by(Measurand::HeartRate, date(2024, 6, 30))
        else {
            panic!("the strap and the watch were both in use");
        };
        let mut named: Vec<&str> = candidates
            .iter()
            .map(|device| device.name().as_str())
            .collect();
        named.sort_unstable();
        assert_eq!(named, vec!["TICKR", "Vivoactive 3"]);
        let edge = find(&devices, EDGE_530).expect("the Edge");
        assert!(edge.sends().is_empty());
    });
}

/// Peloton's account carries the watch past its last Garmin activity, so the
/// rides after it are the watch's with nothing stated.
#[test]
fn the_paired_monitor_carries_the_watch_forward() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = store().await.expect("a store");
        let devices = devices(&pool).await.expect("the register");

        for ride in [date(2026, 9, 30), date(2026, 10, 4), date(2026, 10, 7)] {
            let InUse::One(device) = devices.sent_by(Measurand::HeartRate, ride) else {
                panic!("one device sent the heart rate of {ride}");
            };
            assert_eq!(device.name().as_str(), "Forerunner 165");
        }
    });
}

/// A day Garmin shows nothing worn takes the watch out, and the strap paired
/// to it that day is the one device left.
#[test]
fn a_day_the_watch_was_not_worn_leaves_the_strap() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = store().await.expect("a store");
        day(&pool, "2024-06-30", false).await.expect("a day");
        let unworn = devices(&pool).await.expect("the register");

        let InUse::One(device) = unworn.sent_by(Measurand::HeartRate, date(2024, 6, 30)) else {
            panic!("the strap alone was in use");
        };
        assert_eq!(device.name().as_str(), "TICKR");

        day(&pool, "2024-06-30", true)
            .await
            .expect("a later answer");
        let worn = devices(&pool).await.expect("the register");
        assert!(matches!(
            worn.sent_by(Measurand::HeartRate, date(2024, 6, 30)),
            InUse::Several(_)
        ));
    });
}

/// Garmin recorded the TICKR over ANT+ and over Bluetooth under two serials,
/// and they are one strap: the ANT+ serial's high half is the Bluetooth
/// serial's low half. So a ride on a day only the Bluetooth entry was recorded,
/// with the watch not worn, is the strap's alone.
#[test]
fn the_strap_over_bluetooth_is_the_strap_over_ant() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = store().await.expect("a store");
        // Bluetooth Low Energy is link 3, and its kind for heart rate is 1.
        land(
            &pool,
            "7",
            "2024-01-15 18:00:00",
            &recording(
                (1, 2700, VIVOACTIVE_3),
                &[
                    (0, 255, 1, VIVOACTIVE_3, 2700, 5),
                    (1, 10, 0xFFFF, 0, 0xFFFF, 5),
                    (2, 1, 32, TICKR_BLUETOOTH, 7, 3),
                ],
            ),
        )
        .await
        .expect("a recording");
        day(&pool, "2024-01-20", false).await.expect("a day");
        let devices = devices(&pool).await.expect("the register");

        let strap = find(&devices, TICKR).expect("the strap");
        assert_eq!(find(&devices, TICKR_BLUETOOTH), Some(strap));
        assert_eq!(strap.first_used(), date(2024, 1, 15));
        let InUse::One(device) = devices.sent_by(Measurand::HeartRate, date(2024, 1, 20)) else {
            panic!("the strap alone was in use");
        };
        assert_eq!(device, strap);
    });
}
