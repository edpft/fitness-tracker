//! Garmin's days into [`RestingHeartRate`] (#254).
//!
//! Every payload here is invented; the shapes — a day with a resting heart
//! rate and no readings, a day with both, and a day nothing was worn — are the
//! three the operator's 3,204 landed days hold.

use application::{Translation, ports::SourceAccount, ports::Translator};
use domain::{
    body::RestingHeartRate,
    landing::{
        Endpoint, EventKind, EventProvenance, FetchedAt, LandedRecord, LandingRecord,
        LandingRecordId, LandingStream, RawPayload, SourceRecordId,
    },
    normalised::{OperatorZone, RefusalKind, RefusalReason},
};
use infrastructure::garmin::{GarminRestingHeartRateTranslator, days};
use serde_json::{Value, json};

type Failure = Box<dyn std::error::Error>;

fn record(id: i64, date: &str, payload: &Value) -> Result<LandedRecord, Failure> {
    let provenance = EventProvenance::new(
        Endpoint::try_from("/wellness-service/wellness/dailyHeartRate")?,
        EventKind::try_from("updated")?,
        None,
    );
    let landed = LandingRecord::land(
        LandingStream::try_from("garmin.daily_heart_rate")?,
        FetchedAt::try_from("2026-10-14T08:00:00Z")?,
        SourceRecordId::try_from(date)?,
        provenance.into(),
        RawPayload::try_from(serde_json::to_vec(payload)?)?,
    );
    Ok(LandedRecord::new(LandingRecordId::try_from(id)?, landed))
}

/// A day as Garmin serves one: before about 2026-05-15 with no readings, after
/// it with them.
fn day(date: &str, resting: Option<u32>, readings: &[(i64, u32)]) -> Value {
    let values: Vec<Value> = readings
        .iter()
        .map(|(at, rate)| json!([at, rate]))
        .collect();
    json!({
        "userProfilePK": 8_298_639_i64,
        "calendarDate": date,
        "startTimestampGMT": format!("{date}T00:00:00.0"),
        "endTimestampGMT": format!("{date}T23:59:59.0"),
        "maxHeartRate": readings.iter().map(|(_, rate)| *rate).max(),
        "minHeartRate": readings.iter().map(|(_, rate)| *rate).min(),
        "restingHeartRate": resting,
        "lastSevenDaysAvgRestingHeartRate": resting.map(|rate| rate.saturating_sub(4)),
        "heartRateValueDescriptors": [
            {"index": 0, "key": "timestamp"},
            {"index": 1, "key": "heartrate"},
        ],
        "heartRateValues": if readings.is_empty() { Value::Null } else { json!(values) },
    })
}

fn translate(records: Vec<LandedRecord>) -> Result<Translation<RestingHeartRate>, Failure> {
    let accounts = days(records);
    let [account] = accounts.as_slice() else {
        return Err(format!("{} days, not one", accounts.len()).into());
    };
    Ok(GarminRestingHeartRateTranslator
        .translate(account, &OperatorZone::try_from("Europe/London")?)?)
}

fn entity(records: Vec<LandedRecord>) -> Result<RestingHeartRate, Failure> {
    match translate(records)? {
        Translation::Entity { entity, refusals } if refusals.is_empty() => Ok(*entity),
        other => Err(format!("no day: {other:?}").into()),
    }
}

#[test]
fn a_day_is_its_resting_heart_rate() {
    let payload = day("2019-03-02", Some(48), &[]);
    let rate = entity(vec![record(1, "2019-03-02", &payload).expect("a record")])
        .expect("a resting heart rate");

    assert_eq!(rate.day(), jiff::civil::date(2019, 3, 2));
    assert_eq!(rate.rate().as_u32(), 48);
}

#[test]
fn a_day_with_readings_is_the_same_figure() {
    let payload = day(
        "2026-10-10",
        Some(54),
        &[(1_791_586_800_000, 54), (1_791_586_920_000, 56)],
    );
    let rate = entity(vec![record(1, "2026-10-10", &payload).expect("a record")])
        .expect("a resting heart rate");

    assert_eq!(rate.rate().as_u32(), 54);
}

#[test]
fn a_day_nothing_was_worn_is_refused_as_that_and_not_as_wrong_data() {
    let payload = day("2024-01-15", None, &[]);
    let translated =
        translate(vec![record(1, "2024-01-15", &payload).expect("a record")]).expect("translated");

    let Translation::Refused(refusals) = translated else {
        panic!("not refused: {translated:?}");
    };
    assert_eq!(refusals.first().reason, RefusalReason::NothingWorn);
    assert_eq!(
        refusals.first().reason.kind(),
        RefusalKind::DeclaredLimitation
    );
}

#[test]
fn the_last_serving_of_a_day_stands() {
    let morning = day("2026-10-14", Some(57), &[(1_792_000_000_000, 60)]);
    let evening = day("2026-10-14", Some(52), &[(1_792_000_000_000, 60)]);
    let records = vec![
        record(1, "2026-10-14", &morning).expect("a record"),
        record(2, "2026-10-14", &evening).expect("a record"),
    ];

    let accounts = days(records.clone());
    let [account] = accounts.as_slice() else {
        panic!("{} days, not one", accounts.len());
    };
    assert_eq!(account.superseded(), 1);

    let rate = entity(records).expect("a resting heart rate");
    assert_eq!(rate.rate().as_u32(), 52);
}

#[test]
fn a_resting_heart_rate_of_zero_is_not_a_rate() {
    let payload = day("2020-06-01", Some(0), &[]);
    let translated =
        translate(vec![record(1, "2020-06-01", &payload).expect("a record")]).expect("translated");

    let Translation::Refused(refusals) = translated else {
        panic!("not refused: {translated:?}");
    };
    assert!(matches!(
        refusals.first().reason,
        RefusalReason::UnreadableValue { .. }
    ));
}
