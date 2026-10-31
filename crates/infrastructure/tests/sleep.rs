//! Garmin's sleep records into [`Sleep`] (#255).
//!
//! Every payload here is invented; the shapes — a night with time asleep and a
//! confirmed window, a night that states nothing at all, and a night served
//! twice — are the ones the operator's 3,217 landed nights hold.

use application::{Translation, ports::SourceAccount, ports::Translator};
use domain::{
    body::Sleep,
    landing::{
        Endpoint, EventKind, EventProvenance, FetchedAt, LandedRecord, LandingRecord,
        LandingRecordId, LandingStream, RawPayload, SourceRecordId,
    },
    normalised::{OperatorZone, RefusalKind, RefusalReason, StatedZones},
};
use infrastructure::garmin::{GarminSleepTranslator, sleeps};
use serde_json::{Value, json};

use crate::support::versions;

type Failure = Box<dyn std::error::Error>;

fn record(id: i64, date: &str, payload: &Value) -> Result<LandedRecord, Failure> {
    let provenance = EventProvenance::new(
        Endpoint::try_from("/wellness-service/wellness/dailySleepData")?,
        EventKind::try_from("updated")?,
        None,
    );
    let landed = LandingRecord::land(
        LandingStream::try_from("garmin.sleep")?,
        FetchedAt::try_from("2026-10-27T08:00:00Z")?,
        SourceRecordId::try_from(date)?,
        provenance.into(),
        RawPayload::try_from(serde_json::to_vec(payload)?)?,
    );
    Ok(LandedRecord::new(LandingRecordId::try_from(id)?, landed))
}

/// A night as Garmin serves one: the window in GMT milliseconds, and the
/// watch's offset applied to the same instants beside them.
fn night(date: &str, asleep: Option<u64>, window: Option<(i64, i64)>, offset_hours: i64) -> Value {
    let local = |at: i64| at + offset_hours * 3_600_000;
    json!({
        "dailySleepDTO": {
            "calendarDate": date,
            "sleepTimeSeconds": asleep,
            "napTimeSeconds": asleep.map(|_| 0),
            "sleepStartTimestampGMT": window.map(|(from, _)| from),
            "sleepEndTimestampGMT": window.map(|(_, until)| until),
            "sleepStartTimestampLocal": window.map(|(from, _)| local(from)),
            "sleepEndTimestampLocal": window.map(|(_, until)| local(until)),
            "deepSleepSeconds": asleep.map(|_| 3_660),
            "awakeSleepSeconds": asleep.map(|_| 60),
            "sleepNeed": asleep.map(|_| json!({"deviceId": 3_442_839_173_u64})),
        },
        "restingHeartRate": 52,
    })
}

fn translate(records: Vec<LandedRecord>) -> Result<Translation<Sleep>, Failure> {
    let accounts = sleeps(records);
    let [account] = accounts.as_slice() else {
        return Err(format!("{} nights, not one", accounts.len()).into());
    };
    Ok(GarminSleepTranslator.translate(
        account,
        &StatedZones::throughout(OperatorZone::try_from("Europe/London")?),
    )?)
}

fn entity(records: Vec<LandedRecord>) -> Result<Sleep, Failure> {
    match translate(records)? {
        Translation::Entity { entity, refusals } if refusals.is_empty() => Ok(*entity),
        other => Err(format!("no night: {other:?}").into()),
    }
}

/// 2026-10-20: asleep 23:07 BST, woke 07:13, 8 h 04 min asleep.
const OCTOBER: (i64, i64) = (1_792_447_656_000, 1_792_476_804_000);

#[test]
fn a_night_is_its_time_asleep_and_its_window_in_the_stated_zone() {
    let payload = night("2026-10-20", Some(29_088), Some(OCTOBER), 1);
    let sleep =
        entity(vec![record(1, "2026-10-20", &payload).expect("a record")]).expect("a night");

    assert_eq!(sleep.morning_of(), jiff::civil::date(2026, 10, 20));
    assert_eq!(sleep.asleep().as_seconds(), 29_088);
    assert_eq!(
        sleep.window().fell_asleep().wall_clock().datetime(),
        jiff::civil::date(2026, 10, 19).at(23, 7, 36, 0)
    );
    assert_eq!(
        sleep.window().woke().wall_clock().datetime(),
        jiff::civil::date(2026, 10, 20).at(7, 13, 24, 0)
    );
    assert_eq!(
        sleep.recorded_by().map(ToString::to_string).as_deref(),
        Some("serial/3442839173")
    );
}

#[test]
fn a_night_with_no_sleep_recorded_is_refused_as_that() {
    let payload = night("2026-10-26", None, None, 0);
    let translated =
        translate(vec![record(1, "2026-10-26", &payload).expect("a record")]).expect("translated");

    let Translation::Refused(refusals) = translated else {
        panic!("not refused: {translated:?}");
    };
    assert_eq!(refusals.first().reason, RefusalReason::NoSleepRecorded);
    assert_eq!(
        refusals.first().reason.kind(),
        RefusalKind::DeclaredLimitation
    );
}

#[test]
fn more_asleep_than_the_window_holds_is_not_a_night() {
    let payload = night("2026-10-20", Some(40_000), Some(OCTOBER), 1);
    let translated =
        translate(vec![record(1, "2026-10-20", &payload).expect("a record")]).expect("translated");
    assert!(
        matches!(translated, Translation::Refused(_)),
        "{translated:?}"
    );
}

#[test]
fn the_last_serving_of_a_night_stands() {
    let early = night("2026-10-20", Some(20_000), Some(OCTOBER), 1);
    let later = night("2026-10-20", Some(29_088), Some(OCTOBER), 1);
    let records = vec![
        record(1, "2026-10-20", &early).expect("a record"),
        record(2, "2026-10-20", &later).expect("a record"),
    ];

    let accounts = sleeps(records.clone());
    let [account] = accounts.as_slice() else {
        panic!("{} nights, not one", accounts.len());
    };
    assert_eq!(account.superseded(), 1);
    assert_eq!(
        entity(records).expect("a night").asleep().as_seconds(),
        29_088
    );
}

/// **Its version names what it translates** (#606): each night above, once
/// more.
#[test]
fn its_version_names_what_it_translates() {
    let nights = [
        vec![night("2026-10-20", Some(29_088), Some(OCTOBER), 1)],
        vec![night("2026-10-26", None, None, 0)],
        vec![night("2026-10-20", Some(40_000), Some(OCTOBER), 1)],
        vec![
            night("2026-10-20", Some(20_000), Some(OCTOBER), 1),
            night("2026-10-20", Some(29_088), Some(OCTOBER), 1),
        ],
    ];
    let translations: Vec<_> = nights
        .iter()
        .map(|servings| {
            let records = servings
                .iter()
                .zip(1..)
                .map(|(payload, id)| {
                    let date = payload["dailySleepDTO"]["calendarDate"]
                        .as_str()
                        .expect("a date");
                    record(id, date, payload).expect("a record")
                })
                .collect();
            translate(records).expect("translated")
        })
        .collect();

    if let Err(unpinned) = versions::pinned::<GarminSleepTranslator>(&translations) {
        panic!("{unpinned}");
    }
}
