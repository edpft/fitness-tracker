//! The Peloton Bike+ ride, from two landed responses to rows in the store.
//!
//! At this ring because the suite needs the Peloton translator and the real
//! tables, and `application` may not depend on the ring above it. What it
//! drives is the *use case*, generic over its ports, with real ones supplied.
//!
//! The payloads are written here rather than taken from a corpus file. They are
//! small, every one of them is a case the operator's own account contains, and
//! a fixture that is not a `.rs` file needs naming in the flake's fileset — an
//! omission that passes on this machine and finds an empty file in the sandbox.

use application::{
    ExtractionRunLog as _, NormalisationSummary, RefusalReporter as _, WorkoutNormaliser as _,
    normalise::{Normalisation, NormalisationPorts, Refusals},
};
use domain::{
    landing::{
        Endpoint, EventKind, EventProvenance, FetchedAt, LandingRecord, LandingStream, RawPayload,
        RunId, SourceRecordId,
    },
    normalised::{OperatorZone, RefusalKind, RefusalReason},
};
use infrastructure::{
    PelotonRideAccountReader, PelotonWorkoutLandingStore, PelotonWorkoutSampleLandingStore,
    SqliteBikePlusRideStore, SqliteExtractionRunLog, SqliteNormalisationRunLog, SqliteRefusalStore,
    connect, peloton::PelotonRideTranslator,
};
use sqlx::SqlitePool;

type Built<T> = Result<T, Box<dyn std::error::Error>>;

/// A clock that never moves, so a run log is comparable between derivations.
#[derive(Debug, Clone, Copy)]
struct FixedClock;

impl application::Clock for FixedClock {
    fn now(&self) -> FetchedAt {
        FetchedAt::EPOCH
    }
}

/// One workout record, as Peloton's list serves it.
///
/// Only the fields the translator reads are varied; the rest of a real payload
/// is landed verbatim and never looked at, so leaving it out changes nothing
/// this suite asserts.
fn workout(id: &str, discipline: &str, device: &str, start: i64, end: i64) -> String {
    format!(
        r#"{{"id":"{id}","fitness_discipline":"{discipline}","device_type":"{device}",
            "is_outdoor":false,"start_time":{start},"end_time":{end},"distance":1.3213}}"#
    )
}

/// One performance graph, with four bike series and optionally a heart rate.
///
/// `offsets` is given explicitly because the source's index is sparse and that
/// is the property most worth pinning.
fn graph(
    offsets: &[u32],
    power: &[u32],
    heart_rate: Option<(&[u32], &str)>,
    distance: &str,
    unit: &str,
) -> String {
    let list = |values: &[u32]| {
        values
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };
    let cadence = list(&vec![80; offsets.len()]);
    let resistance = list(&vec![35; offsets.len()]);
    let speed = vec!["25.4"; offsets.len()].join(",");

    let heart = heart_rate.map_or_else(String::new, |(values, missing)| {
        format!(
            r#",{{"slug":"heart_rate","display_unit":"bpm","values":[{}],
                  "missing_data_duration":{missing}}}"#,
            list(values)
        )
    });

    format!(
        r#"{{"seconds_since_pedaling_start":[{}],
             "summaries":[{{"slug":"distance","display_unit":"{unit}","value":{distance}}}],
             "metrics":[
               {{"slug":"output","display_unit":"watts","values":[{}]}},
               {{"slug":"cadence","display_unit":"rpm","values":[{cadence}]}},
               {{"slug":"resistance","display_unit":"%","values":[{resistance}]}},
               {{"slug":"speed","display_unit":"kph","values":[{speed}]}}
               {heart}
             ]}}"#,
        list(offsets),
        list(power),
    )
}

fn record(stream: &LandingStream, endpoint: &str, id: &str, payload: &str) -> Built<LandingRecord> {
    Ok(LandingRecord::land(
        stream.clone(),
        FetchedAt::EPOCH,
        SourceRecordId::try_from(id)?,
        EventProvenance::new(Endpoint::try_from(endpoint)?, EventKind::Updated, None).into(),
        RawPayload::try_from(payload.as_bytes())?,
    ))
}

/// A store holding whatever workouts and graphs a test names, in a temp file.
async fn landed(
    workouts: Vec<(&str, String)>,
    graphs: Vec<(&str, String)>,
) -> Built<(SqlitePool, tempfile::TempDir)> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;

    let rides = PelotonWorkoutLandingStore::new(pool.clone())?;
    let samples = PelotonWorkoutSampleLandingStore::new(pool.clone())?;
    let runs = SqliteExtractionRunLog::new(pool.clone());

    append(&rides, &runs, "/api/user/u/workouts", workouts).await?;
    append(&samples, &runs, "/api/workout/w/performance_graph", graphs).await?;

    Ok((pool, directory))
}

async fn append<S: application::LandingStore + Sync>(
    store: &S,
    runs: &SqliteExtractionRunLog,
    endpoint: &str,
    payloads: Vec<(&str, String)>,
) -> Built<RunId> {
    let run = runs.begin(store.stream(), FetchedAt::EPOCH).await?;
    let mut records = Vec::new();
    for (id, payload) in payloads {
        records.push(record(store.stream(), endpoint, id, &payload)?);
    }
    store.append(run, records).await?;
    Ok(run)
}

async fn derive(pool: &SqlitePool) -> Built<NormalisationSummary> {
    let normalisation = Normalisation::new(
        NormalisationPorts {
            raw: PelotonRideAccountReader::new(pool.clone())?,
            translator: PelotonRideTranslator,
            workouts: SqliteBikePlusRideStore::new(pool.clone())?,
            refusals: SqliteRefusalStore::new(pool.clone(), PelotonWorkoutLandingStore::STREAM)?,
            runs: SqliteNormalisationRunLog::new(pool.clone()),
            clock: FixedClock,
        },
        OperatorZone::try_from("Europe/London")?,
    );
    Ok(normalisation.normalise().await?)
}

async fn refusals(pool: &SqlitePool) -> Built<Vec<domain::normalised::Refusal>> {
    let reporter = Refusals::new(
        SqliteRefusalStore::new(pool.clone(), PelotonWorkoutLandingStore::STREAM)?,
        SqliteNormalisationRunLog::new(pool.clone()),
    );
    Ok(reporter.refusals().await?.refusals)
}

/// A runtime built by hand, because `#[tokio::test]` generates an
/// `#[allow(clippy::unwrap_used)]` that `forbid` refuses to compile.
fn runtime() -> Built<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}

/// A whole ride, end to end: the entity, its samples, and its heart rate.
///
/// The offsets are the operator's real shape rather than `1..n` — the index
/// starts at 4 and skips 7 — because that is what the source serves on 143 of
/// his 285 rides, and a store that took row order for the offset would look
/// right here and lose the gap.
#[test]
fn a_ride_composes_a_workout_record_and_its_graph() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let offsets = [4_u32, 5, 6, 8];
        let (pool, _directory) = landed(
            vec![(
                "ride-1",
                workout("ride-1", "cycling", "home_bike_plus", 100, 400),
            )],
            vec![(
                "ride-1",
                graph(
                    &offsets,
                    &[93, 132, 120, 110],
                    Some((&[132, 131, 0, 130], "7")),
                    "2.12642",
                    "km",
                ),
            )],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.records_read.as_usize(), 1);
        assert_eq!(summary.workouts_written.as_usize(), 1);
        assert!(summary.reconciles(), "every record has exactly one outcome");

        let ride = sqlx::query!(
            r#"SELECT started_at_utc AS "started!: String", zone AS "zone!: String",
                      duration_seconds AS "duration!: i64",
                      distance_millimetres AS "distance!: i64",
                      heart_rate_declared_missing_seconds AS "missing: i64"
               FROM bike_plus_ride"#
        )
        .fetch_one(&pool)
        .await
        .expect("a ride");

        assert_eq!(ride.started, "1970-01-01T00:01:40Z");
        // The operator's declared zone, not the one Peloton stamps on the
        // record: that is the bike reporting its offset, and § II.3 says an
        // offset is not a substitute for a zone.
        assert_eq!(ride.zone, "Europe/London");
        // The ride's own span, from the workout record. Not the class's length,
        // which the graph echoes back.
        assert_eq!(ride.duration, 300);
        // 2.12642 km, exactly, through no float.
        assert_eq!(ride.distance, 2_126_420);
        // What the *source* says it did not measure. Not the number of gaps
        // below, which is one.
        assert_eq!(ride.missing, Some(7));

        let samples = sqlx::query!(
            r#"SELECT at_seconds AS "at!: i64", power_watts AS "power!: i64",
                      speed_millimetres_per_hour AS "speed!: i64"
               FROM bike_plus_ride_sample ORDER BY at_seconds"#
        )
        .fetch_all(&pool)
        .await
        .expect("samples");
        assert_eq!(
            samples.iter().map(|row| row.at).collect::<Vec<_>>(),
            vec![4, 5, 6, 8],
            "the source's own offsets, gap included"
        );
        assert_eq!(samples[0].power, 93);
        // 25.4 km/h with nothing lost to a division by 3.6.
        assert_eq!(samples[0].speed, 25_400_000);

        let heart_rate = sqlx::query!(
            r#"SELECT at_seconds AS "at!: i64", beats_per_minute AS "bpm!: i64"
               FROM bike_plus_ride_heart_rate ORDER BY at_seconds"#
        )
        .fetch_all(&pool)
        .await
        .expect("heart rate");
        // The zero at second 6 is the watch not broadcasting, and contributes
        // no reading rather than a reading of zero (§ 37).
        assert_eq!(
            heart_rate
                .iter()
                .map(|row| (row.at, row.bpm))
                .collect::<Vec<_>>(),
            vec![(4, 132), (5, 131), (8, 130)]
        );
    });
}

/// § 7, at the file. A second derivation over the same raw produces the same
/// rows, so nothing depends on what the first one left behind.
#[test]
fn deriving_twice_produces_the_same_layer() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![(
                "ride-1",
                workout("ride-1", "cycling", "home_bike_plus", 100, 400),
            )],
            vec![(
                "ride-1",
                graph(&[1, 2], &[93, 132], Some((&[132, 131], "3")), "2.0", "km"),
            )],
        )
        .await
        .expect("a landed corpus");

        derive(&pool).await.expect("a first derivation");
        let first = content(&pool).await.expect("content");
        derive(&pool).await.expect("a second derivation");
        let second = content(&pool).await.expect("content");

        assert_eq!(first, second);
    });
}

/// Everything the layer holds, excluding which run wrote it.
async fn content(pool: &SqlitePool) -> Built<Vec<String>> {
    let mut rows = Vec::new();
    for row in sqlx::query!(
        r#"SELECT source_record_id AS "source!: String", started_at_utc AS "started!: String",
                  duration_seconds AS "duration!: i64", distance_millimetres AS "distance!: i64"
           FROM bike_plus_ride ORDER BY landing_record_id"#
    )
    .fetch_all(pool)
    .await?
    {
        rows.push(format!(
            "{} {} {} {}",
            row.source, row.started, row.duration, row.distance
        ));
    }
    for row in sqlx::query!(
        r#"SELECT ride AS "ride!: i64", at_seconds AS "at!: i64", power_watts AS "power!: i64"
           FROM bike_plus_ride_sample ORDER BY ride, at_seconds"#
    )
    .fetch_all(pool)
    .await?
    {
        rows.push(format!("{} {} {}", row.ride, row.at, row.power));
    }
    Ok(rows)
}

/// The 141 records in the operator's account that are not Bike+ rides.
///
/// `Unmodelled` rather than wrong data, and neither dropped nor forced into the
/// entity: they are evidence for a feature nobody has asked for yet.
#[test]
fn what_is_not_a_bike_plus_ride_refuses_as_unmodelled() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![
                // A yoga class taken on the bike's screen. `device_type` says
                // `home_bike_plus` and the bike is not the instrument.
                ("yoga", workout("yoga", "yoga", "home_bike_plus", 100, 400)),
                // A cycling workout the operator's Garmin synced in.
                ("garmin", workout("garmin", "cycling", "garmin", 100, 400)),
            ],
            vec![],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 0);
        assert_eq!(summary.records_refused.as_usize(), 2);
        assert!(summary.reconciles());

        let refusals = refusals(&pool).await.expect("refusals");
        assert!(
            refusals
                .iter()
                .all(|refusal| refusal.kind() == RefusalKind::Unmodelled),
            "{refusals:?}"
        );
        let details: Vec<_> = refusals
            .iter()
            .filter_map(|refusal| refusal.reason.detail())
            .collect();
        assert_eq!(
            details,
            vec![
                "a yoga workout".to_owned(),
                "a cycling workout recorded by garmin".to_owned()
            ]
        );
    });
}

/// The two streams are collected independently, so a ride can be landed before
/// its graph is. That is a reason to run the other walk, not an error and not a
/// ride with the samples left out.
#[test]
fn a_ride_with_no_graph_landed_refuses_rather_than_deriving_half_of_one() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![("ride-1", workout("ride-1", "cycling", "home_bike_plus", 100, 400))],
            vec![],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 0);
        assert_eq!(summary.records_refused.as_usize(), 1);

        let refusals = refusals(&pool).await.expect("refusals");
        assert!(
            matches!(
                refusals.first().map(|refusal| &refusal.reason),
                Some(RefusalReason::CompanionNotLanded { stream }) if stream == "peloton.workout_samples"
            ),
            "{refusals:?}"
        );
    });
}

/// **The trap the entity exists to avoid.** The workout record states a
/// distance with no unit and it is miles; the graph states one with its unit
/// beside it. The unit is read, never assumed, so flipping the account
/// preference changes nothing about what is stored.
#[test]
fn the_distance_is_converted_by_the_unit_the_graph_declares() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![(
                "ride-1",
                workout("ride-1", "cycling", "home_bike_plus", 100, 400),
            )],
            // The same ride as the kilometre case, stated in miles.
            vec![("ride-1", graph(&[1], &[93], None, "1.3213", "mi"))],
        )
        .await
        .expect("a landed corpus");

        derive(&pool).await.expect("a derivation");

        let row =
            sqlx::query!(r#"SELECT distance_millimetres AS "distance!: i64" FROM bike_plus_ride"#)
                .fetch_one(&pool)
                .await
                .expect("a ride");
        // 1.3213 miles is 2126.426 metres, exactly. The graph reports the same
        // ride as 2.12642 km, which is 2126.420 — Peloton rounding its own
        // conversion to five decimal places, six millimetres away. Whichever
        // unit the account is set to is read exactly as stated; the two do not
        // round-trip through each other, and pretending they did would mean
        // choosing which of the source's numbers to disbelieve.
        assert_eq!(row.distance, 2_126_426);
    });
}

/// A unit neither we nor the account preference produce. Refused rather than
/// guessed: a distance read in the wrong unit is wrong by a factor nobody sees.
#[test]
fn an_unknown_distance_unit_refuses_the_ride() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![(
                "ride-1",
                workout("ride-1", "cycling", "home_bike_plus", 100, 400),
            )],
            vec![("ride-1", graph(&[1], &[93], None, "2.0", "furlongs"))],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 0);

        let refusals = refusals(&pool).await.expect("refusals");
        assert!(
            matches!(
                refusals.first().map(|refusal| &refusal.reason),
                Some(RefusalReason::UnreadableValue { .. })
            ),
            "{refusals:?}"
        );
    });
}

/// A ride with no heart-rate series at all is a ride, not a refusal. 52 of the
/// operator's 285 are exactly this: nothing was worn.
#[test]
fn a_ride_with_no_heart_rate_series_is_still_a_ride() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![(
                "ride-1",
                workout("ride-1", "cycling", "home_bike_plus", 100, 400),
            )],
            vec![("ride-1", graph(&[1, 2], &[93, 132], None, "2.0", "km"))],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 1);
        assert_eq!(summary.refusals_recorded.as_usize(), 0);

        let row = sqlx::query!(r#"SELECT COUNT(*) AS "held!: i64" FROM bike_plus_ride_heart_rate"#)
            .fetch_one(&pool)
            .await
            .expect("a count");
        assert_eq!(row.held, 0);
    });
}

/// A series the watch served and never got a reading into. The ride stands and
/// the series does not, which is the difference a separate table makes: with a
/// nullable column this would be indistinguishable from wearing nothing.
#[test]
fn a_heart_rate_series_of_nothing_but_dropouts_refuses_without_costing_the_ride() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![(
                "ride-1",
                workout("ride-1", "cycling", "home_bike_plus", 100, 400),
            )],
            vec![(
                "ride-1",
                graph(&[1, 2], &[93, 132], Some((&[0, 0], "2")), "2.0", "km"),
            )],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 1, "the ride stands");
        assert_eq!(summary.refusals_recorded.as_usize(), 1);

        let refusals = refusals(&pool).await.expect("refusals");
        assert!(
            matches!(
                refusals.first().map(|refusal| &refusal.reason),
                Some(RefusalReason::NoReadingsInSeries { series }) if *series == "heart rate"
            ),
            "{refusals:?}"
        );
    });
}

/// One ride in the operator's account declares `-1` seconds missing. A duration
/// is not negative, so the declaration is refused and the series it belongs to
/// is kept: a refused field does not cost what it was a field of.
#[test]
fn an_unreadable_missing_data_declaration_does_not_cost_the_series() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![(
                "ride-1",
                workout("ride-1", "cycling", "home_bike_plus", 100, 400),
            )],
            vec![(
                "ride-1",
                graph(&[1, 2], &[93, 132], Some((&[132, 131], "-1")), "2.0", "km"),
            )],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 1);
        assert_eq!(summary.refusals_recorded.as_usize(), 1);

        let row = sqlx::query!(
            r#"SELECT heart_rate_declared_missing_seconds AS "missing: i64",
                      (SELECT COUNT(*) FROM bike_plus_ride_heart_rate) AS "held!: i64"
               FROM bike_plus_ride"#
        )
        .fetch_one(&pool)
        .await
        .expect("a ride");
        assert_eq!(
            row.missing, None,
            "nothing is claimed on the source's behalf"
        );
        assert_eq!(row.held, 2, "the readings stand");
    });
}

/// Two servings of one workout are two entities, exactly as they are on the gym
/// side: § 10 puts "the same source contradicting itself" at the canonical
/// layer, and collapsing the pair here is the one thing this layer must not do.
/// The operator's account holds 24 such pairs.
#[test]
fn a_workout_served_twice_derives_twice() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![
                (
                    "ride-1",
                    workout("ride-1", "cycling", "home_bike_plus", 100, 400),
                ),
                (
                    "ride-1",
                    workout("ride-1", "cycling", "home_bike_plus", 100, 401),
                ),
            ],
            vec![("ride-1", graph(&[1], &[93], None, "2.0", "km"))],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 2);

        let row = sqlx::query!(
            r#"SELECT COUNT(DISTINCT source_record_id) AS "sources!: i64",
                      COUNT(*) AS "rides!: i64" FROM bike_plus_ride"#
        )
        .fetch_one(&pool)
        .await
        .expect("a count");
        assert_eq!((row.sources, row.rides), (1, 2));
    });
}
