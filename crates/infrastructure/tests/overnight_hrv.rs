//! Garmin's nights into [`OvernightHrv`], and the operator's ruling that a night
//! without a baseline is not a night with HRV (2026-09-18, #161).
//!
//! Every payload here is invented; the shapes — including the two-second window
//! Garmin serves for a night it measured nothing on, and the one night with a
//! five-minute high and no average — are the ones his 634 landed nights hold.

use application::{Translation, ports::SourceAccount, ports::Translator};
use domain::{
    body::{HrvStatus, OvernightHrv},
    landing::{
        Endpoint, EventKind, EventProvenance, FetchedAt, LandedRecord, LandingRecord,
        LandingRecordId, LandingStream, RawPayload, SourceRecordId,
    },
    normalised::{OperatorZone, RefusalReason},
};
use infrastructure::garmin::{GarminHrvTranslator, nights};
use serde_json::{Value, json};

type Failure = Box<dyn std::error::Error>;

fn record(id: i64, date: &str, payload: &Value) -> Result<LandedRecord, Failure> {
    let provenance = EventProvenance::new(
        Endpoint::try_from("/hrv-service/hrv")?,
        EventKind::try_from("updated")?,
        None,
    );
    let landed = LandingRecord::land(
        LandingStream::try_from("garmin.hrv")?,
        FetchedAt::try_from("2026-09-18T08:00:00Z")?,
        SourceRecordId::try_from(date)?,
        provenance.into(),
        RawPayload::try_from(serde_json::to_vec(payload)?)?,
    );
    Ok(LandedRecord::new(LandingRecordId::try_from(id)?, landed))
}

fn baseline() -> Value {
    json!({
        "lowUpper": 45, "balancedLow": 49, "balancedUpper": 71, "markerValue": 0.363_632_2,
    })
}

/// A night as Garmin serves one, with the readings it serves for recent nights.
fn night(date: &str, start: &str, end: &str, readings: usize) -> Value {
    let readings: Vec<Value> = (0..readings)
        .map(|step| {
            let minute = step.saturating_mul(5);
            json!({
                "hrvValue": 50 + i64::try_from(step).unwrap_or(0) % 7,
                "readingTimeGMT": format!("2026-09-16T22:{:02}:06.0", minute % 60),
                "readingTimeLocal": format!("2026-09-16T23:{:02}:06.0", minute % 60),
            })
        })
        .collect();
    json!({
        "userProfilePk": 8_298_639_i64,
        "hrvSummary": {
            "calendarDate": date,
            "weeklyAvg": 54,
            "lastNightAvg": 52,
            "lastNight5MinHigh": 79,
            "baseline": baseline(),
            "status": "UNBALANCED",
            "feedbackPhrase": "HRV_UNBALANCED_12",
            "createTimeStamp": format!("{date}T05:32:37.942"),
        },
        "hrvReadings": readings,
        "startTimestampGMT": start,
        "endTimestampGMT": end,
        "startTimestampLocal": start,
        "endTimestampLocal": end,
        "sleepStartTimestampGMT": null,
        "sleepEndTimestampGMT": null,
        "sleepStartTimestampLocal": null,
        "sleepEndTimestampLocal": null,
    })
}

/// A summary with fields overridden, which is how the awkward nights are built.
fn night_with(date: &str, start: &str, end: &str, overrides: &[(&str, Value)]) -> Value {
    let mut payload = night(date, start, end, 0);
    if let Some(summary) = payload.get_mut("hrvSummary").and_then(Value::as_object_mut) {
        for (field, value) in overrides {
            summary.insert((*field).to_owned(), value.clone());
        }
    }
    payload
}

fn translate(date: &str, payload: &Value) -> Result<Translation<OvernightHrv>, Failure> {
    let accounts = nights(vec![record(1, date, payload)?]);
    let [account] = accounts.as_slice() else {
        return Err(format!("{} nights, not one", accounts.len()).into());
    };
    Ok(GarminHrvTranslator.translate(account, &OperatorZone::try_from("Europe/London")?)?)
}

fn entity_for(date: &str, payload: &Value) -> Result<OvernightHrv, Failure> {
    match translate(date, payload)? {
        Translation::Entity { entity, refusals } if refusals.is_empty() => Ok(*entity),
        other => Err(format!("no night: {other:?}").into()),
    }
}

fn refused_for(date: &str, payload: &Value) -> Result<RefusalReason, Failure> {
    match translate(date, payload)? {
        Translation::Refused(refusals) => Ok(refusals.first().reason.clone()),
        other => Err(format!("not refused: {other:?}").into()),
    }
}

#[test]
fn a_night_is_its_summary_and_the_readings_behind_it() {
    let payload = night(
        "2026-09-17",
        "2026-09-16T22:32:32.0",
        "2026-09-17T05:55:54.0",
        3,
    );
    let night = entity_for("2026-09-17", &payload).expect("a night");

    assert_eq!(night.last_night().average.as_milliseconds(), 52);
    assert_eq!(night.last_night().five_minute_high.as_milliseconds(), 79);
    assert_eq!(night.weekly().average.as_milliseconds(), 54);
    assert_eq!(night.weekly().status, HrvStatus::Unbalanced);
    assert_eq!(night.weekly().baseline.low_upper.as_milliseconds(), 45);
    assert_eq!(night.weekly().baseline.balanced_upper.as_milliseconds(), 71);

    let readings = night.last_night().readings.as_ref().expect("readings");
    assert_eq!(readings.iter().count(), 3);
    assert_eq!(
        readings
            .iter()
            .next()
            .expect("a first reading")
            .taken_at
            .zone()
            .id(),
        "Europe/London"
    );
}

#[test]
fn the_morning_is_the_windows_end_and_two_nights_can_start_on_one_date() {
    // The shape of 2025-01-28 and 2025-01-29 in the operator's record: one night
    // begins after midnight, the next begins before it, and both begin on the 28th.
    let after_midnight = night(
        "2025-01-28",
        "2025-01-28T00:24:39.0",
        "2025-01-28T06:27:48.0",
        0,
    );
    let before_midnight = night(
        "2025-01-29",
        "2025-01-28T23:17:05.0",
        "2025-01-29T06:27:48.0",
        0,
    );

    let first = entity_for("2025-01-28", &after_midnight).expect("a night");
    let second = entity_for("2025-01-29", &before_midnight).expect("a night");

    assert_eq!(first.morning_of().to_string(), "2025-01-28");
    assert_eq!(second.morning_of().to_string(), "2025-01-29");
    assert_eq!(
        first.measured().from().wall_clock().date(),
        second.measured().from().wall_clock().date(),
        "both windows start on the same date, which is why the morning is the label",
    );
}

#[test]
fn a_night_whose_readings_garmin_no_longer_serves_still_stands() {
    let payload = night(
        "2025-06-01",
        "2025-05-31T22:32:32.0",
        "2025-06-01T05:55:54.0",
        0,
    );
    let night = entity_for("2025-06-01", &payload).expect("a night");
    assert!(
        night.last_night().readings.is_none(),
        "504 of 634 nights are served with no readings at all",
    );
    assert_eq!(night.last_night().average.as_milliseconds(), 52);
}

#[test]
fn a_night_before_garmin_had_a_baseline_is_refused() {
    let onboarding = night_with(
        "2024-12-22",
        "2024-12-22T00:02:59.0",
        "2024-12-22T06:59:21.0",
        &[
            ("status", json!("NONE")),
            ("baseline", Value::Null),
            ("weeklyAvg", Value::Null),
            ("feedbackPhrase", json!("ONBOARDING_1")),
        ],
    );
    assert_eq!(
        refused_for("2024-12-22", &onboarding).expect("a refusal"),
        RefusalReason::WithoutBaseline
    );
}

#[test]
fn a_night_garmin_measured_nothing_on_is_refused() {
    let placeholder = night_with(
        "2025-03-31",
        "2025-03-31T06:27:46.0",
        "2025-03-31T06:27:48.0",
        &[
            ("lastNightAvg", Value::Null),
            ("lastNight5MinHigh", Value::Null),
        ],
    );
    assert_eq!(
        refused_for("2025-03-31", &placeholder).expect("a refusal"),
        RefusalReason::MissingFigure {
            figure: "overnight reading of any kind"
        }
    );
}

#[test]
fn a_night_measured_with_no_average_is_refused_for_its_own_reason() {
    let partial = night_with(
        "2026-03-12",
        "2026-03-10T22:32:32.0",
        "2026-03-11T05:55:54.0",
        &[
            ("lastNightAvg", Value::Null),
            ("lastNight5MinHigh", json!(94)),
        ],
    );
    assert_eq!(
        refused_for("2026-03-12", &partial).expect("a refusal"),
        RefusalReason::MissingFigure {
            figure: "overnight average"
        },
        "the one night with a high and no average stays distinguishable from the 23",
    );
}

#[test]
fn a_status_outside_our_vocabulary_is_refused_rather_than_mapped() {
    let unknown = night_with(
        "2026-09-17",
        "2026-09-16T22:32:32.0",
        "2026-09-17T05:55:54.0",
        &[("status", json!("POOR"))],
    );
    assert!(matches!(
        refused_for("2026-09-17", &unknown).expect("a refusal"),
        RefusalReason::Unmodelled { .. }
    ));
}

#[test]
fn a_reading_of_zero_is_a_sensor_saying_nothing() {
    let mut payload = night(
        "2026-09-17",
        "2026-09-16T22:32:32.0",
        "2026-09-17T05:55:54.0",
        1,
    );
    if let Some(readings) = payload.get_mut("hrvReadings").and_then(Value::as_array_mut) {
        readings[0] = json!({
            "hrvValue": 0,
            "readingTimeGMT": "2026-09-16T22:35:06.0",
            "readingTimeLocal": "2026-09-16T23:35:06.0",
        });
    }
    assert!(matches!(
        refused_for("2026-09-17", &payload).expect("a refusal"),
        RefusalReason::UnreadableValue { .. }
    ));
}

#[test]
fn a_window_that_ends_before_it_starts_is_refused() {
    let backwards = night(
        "2026-09-17",
        "2026-09-17T05:55:54.0",
        "2026-09-16T22:32:32.0",
        0,
    );
    assert!(matches!(
        refused_for("2026-09-17", &backwards).expect("a refusal"),
        RefusalReason::UnreadableValue { .. }
    ));
}

#[test]
fn a_night_served_twice_is_the_later_serving_and_the_earlier_is_set_aside() {
    let first = night(
        "2026-09-17",
        "2026-09-16T22:32:32.0",
        "2026-09-17T05:55:54.0",
        0,
    );
    let revised = night_with(
        "2026-09-17",
        "2026-09-16T22:32:32.0",
        "2026-09-17T05:55:54.0",
        &[("status", json!("BALANCED"))],
    );
    let records = vec![
        record(1, "2026-09-17", &first).expect("a record"),
        record(2, "2026-09-17", &revised).expect("a record"),
    ];
    let accounts = nights(records);
    let [account] = accounts.as_slice() else {
        panic!("{} nights, not one", accounts.len());
    };
    assert_eq!(account.records(), 2);
    assert_eq!(account.superseded(), 1);

    let translated = GarminHrvTranslator
        .translate(
            account,
            &OperatorZone::try_from("Europe/London".to_owned()).expect("a zone"),
        )
        .expect("a translation");
    let Translation::Entity { entity, .. } = translated else {
        panic!("no night: {translated:?}");
    };
    assert_eq!(
        entity.weekly().status,
        HrvStatus::Balanced,
        "the later serving is what stands (§ 10)",
    );
}

/// The write path, against a real SQLite file.
///
/// The suite above asserts the translation; this asserts the half it cannot —
/// that a night and its readings survive a round trip through the store, and
/// that a second derivation reproduces the first exactly (§ 7).
mod stored {
    use super::{Failure, night, night_with};

    use application::{
        ExtractionRunLog as _, LandingStore as _, WorkoutNormaliser,
        normalise::{Normalisation, NormalisationPorts},
        ports::{Clock, RefusalStore as _},
    };
    use domain::{
        landing::{
            Endpoint, EventKind, EventProvenance, FetchedAt, LandingRecord, LandingStream,
            RawPayload, SourceRecordId,
        },
        normalised::{OperatorZone, RefusalReason},
    };
    use infrastructure::{
        GarminHrvAccountReader, GarminHrvLandingStore, GarminHrvTranslator, SqliteExtractionRunLog,
        SqliteNormalisationRunLog, SqliteOvernightHrvStore, SqliteRefusalStore, connect,
    };
    use serde_json::Value;
    use sqlx::SqlitePool;

    struct EpochClock;

    impl Clock for EpochClock {
        fn now(&self) -> FetchedAt {
            FetchedAt::EPOCH
        }
    }

    fn landing_record(date: &str, payload: &Value) -> Result<LandingRecord, Failure> {
        let provenance = EventProvenance::new(
            Endpoint::try_from("/hrv-service/hrv")?,
            EventKind::try_from("updated")?,
            None,
        );
        Ok(LandingRecord::land(
            LandingStream::try_from("garmin.hrv")?,
            FetchedAt::try_from("2026-09-18T08:00:00Z")?,
            SourceRecordId::try_from(date)?,
            provenance.into(),
            RawPayload::try_from(serde_json::to_vec(payload)?)?,
        ))
    }

    /// Three nights landed: one with readings, one without, one Garmin measured
    /// nothing on.
    async fn landed() -> Result<(SqlitePool, tempfile::TempDir), Failure> {
        let directory = tempfile::tempdir()?;
        let pool = connect(&directory.path().join("test.db")).await?;

        let landing = GarminHrvLandingStore::new(pool.clone())?;
        let runs = SqliteExtractionRunLog::new(pool.clone());
        let run = runs.begin(landing.stream(), FetchedAt::EPOCH).await?;

        let records = vec![
            landing_record(
                "2026-09-17",
                &night(
                    "2026-09-17",
                    "2026-09-16T22:32:32.0",
                    "2026-09-17T05:55:54.0",
                    4,
                ),
            )?,
            landing_record(
                "2025-06-01",
                &night(
                    "2025-06-01",
                    "2025-05-31T22:32:32.0",
                    "2025-06-01T05:55:54.0",
                    0,
                ),
            )?,
            landing_record(
                "2025-03-31",
                &night_with(
                    "2025-03-31",
                    "2025-03-31T06:27:46.0",
                    "2025-03-31T06:27:48.0",
                    &[
                        ("lastNightAvg", Value::Null),
                        ("lastNight5MinHigh", Value::Null),
                    ],
                ),
            )?,
        ];
        landing.append(run, records).await?;
        Ok((pool, directory))
    }

    async fn derive(pool: &SqlitePool) -> Result<(usize, usize), Failure> {
        let normalisation = Normalisation::new(
            NormalisationPorts {
                raw: GarminHrvAccountReader::new(pool.clone())?,
                translator: GarminHrvTranslator,
                workouts: SqliteOvernightHrvStore::new(pool.clone())?,
                refusals: SqliteRefusalStore::new(pool.clone(), GarminHrvLandingStore::STREAM)?,
                runs: SqliteNormalisationRunLog::new(pool.clone()),
                clock: EpochClock,
            },
            OperatorZone::try_from("Europe/London")?,
        );
        let summary = normalisation.normalise().await?;
        Ok((
            summary.workouts_written.as_usize(),
            summary.refusals_recorded.as_usize(),
        ))
    }

    fn block_on<T>(body: impl std::future::Future<Output = T>) -> Result<T, Failure> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        Ok(runtime.block_on(body))
    }

    #[test]
    fn a_night_and_its_readings_survive_the_store_and_a_re_derivation() {
        let outcome: Result<(), Failure> = block_on(async {
            let (pool, _directory) = landed().await?;

            let first = derive(&pool).await?;
            assert_eq!(first, (2, 1), "two nights stand, one is refused");

            let readings =
                sqlx::query!(r#"SELECT count(*) AS "n!: i64" FROM overnight_hrv_reading"#)
                    .fetch_one(&pool)
                    .await?
                    .n;
            assert_eq!(readings, 4);

            let mornings = sqlx::query!(
                r#"SELECT morning_of AS "morning!: String" FROM overnight_hrv ORDER BY morning_of"#
            )
            .fetch_all(&pool)
            .await?
            .into_iter()
            .map(|row| row.morning)
            .collect::<Vec<_>>();
            assert_eq!(mornings, ["2025-06-01", "2026-09-17"]);

            let status = sqlx::query!(
                r#"
                SELECT status AS "status!: String", last_night_average_ms AS "average!: i64"
                FROM overnight_hrv WHERE morning_of = '2026-09-17'
                "#
            )
            .fetch_one(&pool)
            .await?;
            assert_eq!((status.status.as_str(), status.average), ("unbalanced", 52));

            // **Read the refusal back**, which is the half a count cannot assert:
            // a reason is a key and a string in the store and a `&'static str` on
            // the way out, so a name this version cannot resolve makes `status`
            // and `refusals` fail on a derivation that succeeded. That is how the
            // first live run broke.
            let refusals = SqliteRefusalStore::new(pool.clone(), GarminHrvLandingStore::STREAM)?
                .all()
                .await?;
            let [refusal] = refusals.as_slice() else {
                panic!("{} refusals, not one", refusals.len());
            };
            assert_eq!(
                refusal.reason,
                RefusalReason::MissingFigure {
                    figure: "overnight reading of any kind"
                }
            );

            // § 7: the layer is replaced, not appended to.
            assert_eq!(derive(&pool).await?, first);
            let after = sqlx::query!(r#"SELECT count(*) AS "n!: i64" FROM overnight_hrv"#)
                .fetch_one(&pool)
                .await?
                .n;
            assert_eq!(after, 2);
            Ok(())
        })
        .expect("a runtime");
        outcome.expect("the store round trip");
    }
}
