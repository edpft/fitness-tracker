//! Garmin's daily summaries into [`Stress`] (#517, #777).
//!
//! Every payload here is invented; the shapes — a day with an average
//! measured over the whole of it, a day measured for a morning, a day with −1
//! for an average, a day that states nothing, and a day served twice — are the
//! ones the operator's 3,232 landed days hold.

use application::{
    ExtractionRunLog, LandingStore, StressHistory, Translation, WorkoutNormaliser as _,
    normalise::{Normalisation, NormalisationPorts},
    ports::{SourceAccount, Translator},
};
use domain::{
    body::Stress,
    landing::{
        Endpoint, EventKind, EventProvenance, FetchedAt, LandedRecord, LandingRecord,
        LandingRecordId, LandingStream, RawPayload, SourceRecordId,
    },
    normalised::{OperatorZone, RefusalKind, RefusalReason, StatedZones},
};
use infrastructure::{
    GarminDailySummaryAccountReader, GarminDailySummaryLandingStore, GarminSleepLandingStore,
    SqliteExtractionRunLog, SqliteNormalisationRunLog, SqliteRefusalStore, SqliteStressHistory,
    SqliteStressStore, connect,
    garmin::{GarminStressTranslator, days},
};
use serde_json::{Value, json};

use crate::support::{corpus::FixedClock, versions};

type Failure = Box<dyn std::error::Error>;

fn landing(
    stream: &str,
    path: &str,
    date: &str,
    payload: &Value,
) -> Result<LandingRecord, Failure> {
    let provenance = EventProvenance::new(
        Endpoint::try_from(path)?,
        EventKind::try_from("updated")?,
        None,
    );
    Ok(LandingRecord::land(
        LandingStream::try_from(stream)?,
        FetchedAt::try_from("2026-11-11T08:00:00Z")?,
        SourceRecordId::try_from(date)?,
        provenance.into(),
        RawPayload::try_from(serde_json::to_vec(payload)?)?,
    ))
}

fn record(id: i64, date: &str, payload: &Value) -> Result<LandedRecord, Failure> {
    Ok(LandedRecord::new(
        LandingRecordId::try_from(id)?,
        landing(
            "garmin.daily_summary",
            "/usersummary-service/usersummary/daily",
            date,
            payload,
        )?,
    ))
}

/// A day as Garmin's summary serves one: the average, and the seconds it was
/// measured over split between Garmin's bands.
fn day(date: &str, average: Option<i64>, measured: Option<u64>) -> Value {
    json!({
        "calendarDate": date,
        "totalSteps": 9_412,
        "restingHeartRate": 50,
        "averageStressLevel": average,
        "maxStressLevel": average.map(|level| level + 60),
        "stressDuration": measured.map(|seconds| seconds / 4),
        "restStressDuration": measured.map(|seconds| seconds / 2),
        "totalStressDuration": measured,
        "stressQualifier": average.map(|_| "BALANCED"),
    })
}

fn translate(records: Vec<LandedRecord>) -> Result<Translation<Stress>, Failure> {
    let accounts = days(records);
    let [account] = accounts.as_slice() else {
        return Err(format!("{} days, not one", accounts.len()).into());
    };
    Ok(GarminStressTranslator.translate(
        account,
        &StatedZones::throughout(OperatorZone::try_from("Europe/London")?),
    )?)
}

fn entity(records: Vec<LandedRecord>) -> Result<Stress, Failure> {
    match translate(records)? {
        Translation::Entity { entity, refusals } if refusals.is_empty() => Ok(*entity),
        other => Err(format!("no day: {other:?}").into()),
    }
}

fn refused(payload: &Value) -> Result<RefusalReason, Failure> {
    match translate(vec![record(1, "2026-10-20", payload)?])? {
        Translation::Refused(refusals) => Ok(refusals.first().reason.clone()),
        other => Err(format!("not refused: {other:?}").into()),
    }
}

#[test]
fn a_day_is_its_average_and_how_long_it_was_measured_over() {
    let payload = day("2026-10-20", Some(28), Some(86_400));
    let stress = entity(vec![record(1, "2026-10-20", &payload).expect("a record")]).expect("a day");

    assert_eq!(stress.day(), jiff::civil::date(2026, 10, 20));
    assert_eq!(stress.average().as_u8(), 28);
    assert_eq!(stress.measured().as_seconds(), 86_400);
    assert_eq!(stress.recorded_by(), None);
}

#[test]
fn a_day_with_no_average_is_refused_as_no_stress_recorded() {
    for payload in [
        day("2026-10-20", None, None),
        day("2026-10-20", Some(-1), Some(3_600)),
        day("2026-10-20", Some(-2), None),
    ] {
        let reason = refused(&payload).expect("refused");
        assert_eq!(reason, RefusalReason::NoStressRecorded);
        assert_eq!(reason.kind(), RefusalKind::DeclaredLimitation);
    }
}

#[test]
fn an_average_with_nothing_saying_how_long_it_was_measured_over_is_unreadable() {
    let reason = refused(&day("2026-10-20", Some(28), None)).expect("refused");
    assert!(
        matches!(reason, RefusalReason::UnreadablePayload { .. }),
        "{reason:?}"
    );
}

#[test]
fn the_last_serving_of_a_day_stands() {
    let morning = day("2026-10-20", Some(19), Some(28_800));
    let whole = day("2026-10-20", Some(28), Some(86_400));
    let records = vec![
        record(1, "2026-10-20", &morning).expect("a record"),
        record(2, "2026-10-20", &whole).expect("a record"),
    ];

    let accounts = days(records.clone());
    let [account] = accounts.as_slice() else {
        panic!("{} days, not one", accounts.len());
    };
    assert_eq!(account.superseded(), 1);
    assert_eq!(entity(records).expect("a day").average().as_u8(), 28);
}

/// Each day reaches the store with the watch the sleep record names, and is
/// read back with how long it was measured over.
#[test]
fn a_day_reaches_the_store_and_names_the_watch_that_slept() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        let directory = tempfile::tempdir().expect("a directory");
        let pool = connect(&directory.path().join("test.db"))
            .await
            .expect("a store");
        let runs = SqliteExtractionRunLog::new(pool.clone());

        let summaries = GarminDailySummaryLandingStore::new(pool.clone()).expect("a store");
        let run = runs
            .begin(summaries.stream(), FetchedAt::EPOCH)
            .await
            .expect("a run");
        let landed = [
            day("2026-10-19", Some(41), Some(21_600)),
            day("2026-10-20", Some(28), Some(86_400)),
            day("2026-10-21", None, None),
        ]
        .iter()
        .zip(["2026-10-19", "2026-10-20", "2026-10-21"])
        .map(|(payload, date)| {
            landing(
                "garmin.daily_summary",
                "/usersummary-service/usersummary/daily",
                date,
                payload,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .expect("records");
        summaries.append(run, landed).await.expect("landed");

        let sleeps = GarminSleepLandingStore::new(pool.clone()).expect("a store");
        let run = runs
            .begin(sleeps.stream(), FetchedAt::EPOCH)
            .await
            .expect("a run");
        let slept = landing(
            "garmin.sleep",
            "/wellness-service/wellness/dailySleepData",
            "2026-10-20",
            &json!({
                "dailySleepDTO": {
                    "calendarDate": "2026-10-20",
                    "sleepNeed": { "deviceId": 3_442_839_173_u64 },
                },
            }),
        )
        .expect("a record");
        sleeps.append(run, vec![slept]).await.expect("landed");

        let summary = Normalisation::new(
            NormalisationPorts {
                raw: GarminDailySummaryAccountReader::new(pool.clone()).expect("a reader"),
                translator: GarminStressTranslator,
                workouts: SqliteStressStore::new(pool.clone()).expect("a store"),
                refusals: SqliteRefusalStore::new(pool.clone(), "garmin.daily_summary")
                    .expect("refusals"),
                runs: SqliteNormalisationRunLog::new(pool.clone()),
                clock: FixedClock,
            },
            StatedZones::throughout(OperatorZone::try_from("Europe/London").expect("a zone")),
        )
        .normalise(application::Rederivation::Whole)
        .await
        .expect("normalised");
        assert_eq!(summary.workouts_written.as_usize(), 2);

        let days = SqliteStressHistory::new(pool).days().await.expect("days");
        let read: Vec<_> = days
            .iter()
            .map(|day| {
                (
                    day.reading.on,
                    day.reading.value.average.as_u8(),
                    day.reading.value.over.as_seconds(),
                    day.by.as_ref().map(ToString::to_string),
                )
            })
            .collect();
        assert_eq!(
            read,
            vec![
                (jiff::civil::date(2026, 10, 19), 41, 21_600, None),
                (
                    jiff::civil::date(2026, 10, 20),
                    28,
                    86_400,
                    Some("serial/3442839173".to_owned())
                ),
            ]
        );
    });
}

/// **Its version names what it translates** (#606): each day above, once
/// more.
#[test]
fn its_version_names_what_it_translates() {
    let served = [
        vec![day("2026-10-20", Some(28), Some(86_400))],
        vec![day("2026-10-20", None, None)],
        vec![day("2026-10-20", Some(-1), Some(3_600))],
        vec![day("2026-10-20", Some(28), None)],
        vec![
            day("2026-10-20", Some(19), Some(28_800)),
            day("2026-10-20", Some(28), Some(86_400)),
        ],
    ];
    let translations: Vec<_> = served
        .iter()
        .map(|servings| {
            let records = servings
                .iter()
                .zip(1..)
                .map(|(payload, id)| record(id, "2026-10-20", payload).expect("a record"))
                .collect();
            translate(records).expect("translated")
        })
        .collect();

    if let Err(unpinned) = versions::pinned::<GarminStressTranslator>(&translations) {
        panic!("{unpinned}");
    }
}
