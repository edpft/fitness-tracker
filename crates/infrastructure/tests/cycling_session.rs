//! The cycling session, from Peloton's landed records to rows in the store.
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
    PelotonRideLandingStore, PelotonRideSampleLandingStore, PelotonSessionAccountReader,
    SqliteCyclingSessionStore, SqliteExtractionRunLog, SqliteNormalisationRunLog,
    SqliteRefusalStore, connect, peloton::PelotonSessionTranslator,
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

/// Peloton's own id for the *Cool Down Ride* class type.
const COOL_DOWN: &str = "a1fa617f3ba14c0a8c25468d5c88b3ea";
/// Peloton's own id for the *Low Impact Ride* class type.
const LOW_IMPACT: &str = "59a49f882ea9475faa3110d50a8fb3f3";
/// The series every Power Zone class the operator rides belongs to.
const POWER_ZONE_SERIES: &str = "0f63c48726fa4533a928cae5358d94d7";
/// Peloton's series for its FTP test rides.
const FTP_TEST_SERIES: &str = "7609c9f02ed644e58104af7a8337125c";
/// Peloton's series for the warm-up ridden before one.
const FTP_WARM_UP_SERIES: &str = "ad6c6bc2e8ce4304bb6839f690038271";

/// What a ride was, in the terms the source states it.
#[derive(Debug, Clone, Copy)]
enum Kind {
    Main,
    CoolDown,
    LowImpact,
    WarmUp,
    Test,
    /// Just Ride and Entertainment: no class at all.
    Freestyle,
}

/// One workout record, as Peloton's list serves it.
///
/// Only the fields the translator reads are varied; the rest of a real payload
/// is landed verbatim and never looked at, so leaving it out changes nothing
/// this suite asserts.
fn workout(id: &str, kind: Kind, start: i64, end: i64) -> String {
    ridden(id, kind, "cycling", "home_bike_plus", start, end)
}

fn ridden(id: &str, kind: Kind, discipline: &str, device: &str, start: i64, end: i64) -> String {
    let (workout_type, class) = match kind {
        Kind::Freestyle => ("freestyle", "null".to_owned()),
        Kind::Main => ("class", class_of(POWER_ZONE_SERIES, &[])),
        Kind::CoolDown => ("class", class_of(POWER_ZONE_SERIES, &[COOL_DOWN])),
        Kind::LowImpact => ("class", class_of(POWER_ZONE_SERIES, &[LOW_IMPACT])),
        Kind::WarmUp => ("class", class_of(FTP_WARM_UP_SERIES, &[])),
        Kind::Test => ("class", class_of(FTP_TEST_SERIES, &[])),
    };
    format!(
        r#"{{"id":"{id}","fitness_discipline":"{discipline}","device_type":"{device}",
            "is_outdoor":false,"start_time":{start},"end_time":{end},"distance":1.3213,
            "workout_type":"{workout_type}","ride":{class}}}"#
    )
}

fn class_of(series: &str, class_types: &[&str]) -> String {
    let types = class_types
        .iter()
        .map(|id| format!("\"{id}\""))
        .collect::<Vec<_>>()
        .join(",");
    format!(r#"{{"title":"a class","series_id":"{series}","class_type_ids":[{types}]}}"#)
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

    let rides = PelotonRideLandingStore::new(pool.clone())?;
    let samples = PelotonRideSampleLandingStore::new(pool.clone())?;
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
            raw: PelotonSessionAccountReader::new(pool.clone())?,
            translator: PelotonSessionTranslator,
            workouts: SqliteCyclingSessionStore::new(pool.clone())?,
            refusals: SqliteRefusalStore::new(pool.clone(), PelotonRideLandingStore::STREAM)?,
            runs: SqliteNormalisationRunLog::new(pool.clone()),
            clock: FixedClock,
        },
        OperatorZone::try_from("Europe/London")?,
    );
    Ok(normalisation.normalise().await?)
}

async fn refusals(pool: &SqlitePool) -> Built<Vec<domain::normalised::Refusal>> {
    let reporter = Refusals::new(
        SqliteRefusalStore::new(pool.clone(), PelotonRideLandingStore::STREAM)?,
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
            vec![("ride-1", workout("ride-1", Kind::Main, 100, 400))],
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
        // Two records for one session of one ride: the workout and its graph.
        assert_eq!(summary.records_read.as_usize(), 2);
        assert_eq!(summary.workouts_written.as_usize(), 1);
        assert_eq!(summary.records_composed.as_usize(), 2);
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
            vec![("ride-1", workout("ride-1", Kind::Main, 100, 400))],
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
                (
                    "yoga",
                    ridden("yoga", Kind::Main, "yoga", "home_bike_plus", 100, 400),
                ),
                // A cycling workout the operator's Garmin synced in.
                // A day later, so the two are separate sessions and each
                // refuses for its own reason.
                (
                    "garmin",
                    ridden("garmin", Kind::Main, "cycling", "garmin", 90_000, 90_300),
                ),
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
            vec![("ride-1", workout("ride-1", Kind::Main, 100, 400))],
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
                Some(RefusalReason::CompanionNotLanded { stream }) if stream == "peloton.ride_samples"
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
            vec![("ride-1", workout("ride-1", Kind::Main, 100, 400))],
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
            vec![("ride-1", workout("ride-1", Kind::Main, 100, 400))],
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
            vec![("ride-1", workout("ride-1", Kind::Main, 100, 400))],
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
            vec![("ride-1", workout("ride-1", Kind::Main, 100, 400))],
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
            vec![("ride-1", workout("ride-1", Kind::Main, 100, 400))],
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

/// Two servings of one ride are one ride told twice, not a session of two.
///
/// **This is the case that made `records_superseded` necessary.** § 10 says the
/// later supersedes and § 3.1 says the two do not compose, but until an entity
/// composed several records neither had to be acted on: one record made one
/// workout and both stood. Group them and one ride becomes a `main + main`
/// session, which is not a thing that happened.
///
/// The operator's store holds 51 such records — every one a workout landed
/// twice before the revision digest learned to ignore how many strangers were
/// riding the class at that moment.
#[test]
fn a_ride_served_twice_is_one_ride_and_the_earlier_serving_is_counted() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![
                ("ride-1", workout("ride-1", Kind::Main, 100, 400)),
                ("ride-1", workout("ride-1", Kind::Main, 100, 401)),
            ],
            vec![("ride-1", graph(&[1], &[93], None, "2.0", "km"))],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 1, "one session");
        assert_eq!(summary.records_superseded.as_usize(), 1, "the earlier one");
        assert!(
            summary.reconciles(),
            "the superseded record still has an outcome"
        );

        let row = sqlx::query!(
            r#"SELECT COUNT(*) AS "rides!: i64",
                      (SELECT COUNT(*) FROM cycling_session) AS "sessions!: i64",
                      (SELECT duration_seconds FROM bike_plus_ride) AS "duration!: i64"
               FROM bike_plus_ride"#
        )
        .fetch_one(&pool)
        .await
        .expect("a count");
        assert_eq!((row.sessions, row.rides), (1, 1));
        // The later serving, which is the one the source is still standing by.
        assert_eq!(row.duration, 301);
    });
}

/// A main ride and the cool-down ridden down from it: 114 of the operator's
/// sessions, and the commonest shape in his record.
#[test]
fn a_main_ride_and_its_cool_down_are_one_session() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![
                ("main", workout("main", Kind::Main, 100, 1900)),
                // 70 seconds later: choosing the next class, not a new session.
                ("cool", workout("cool", Kind::CoolDown, 1970, 2270)),
            ],
            vec![
                ("main", graph(&[1], &[93], None, "10.0", "km")),
                ("cool", graph(&[1], &[40], None, "1.0", "km")),
            ],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 1, "one session");
        assert!(summary.reconciles());

        let row = sqlx::query!(
            r#"SELECT kind AS "kind!: String",
                      (SELECT GROUP_CONCAT(role, ",") FROM (
                         SELECT role FROM bike_plus_ride ORDER BY started_at_utc
                       )) AS "roles!: String"
               FROM cycling_session"#
        )
        .fetch_one(&pool)
        .await
        .expect("a session");
        assert_eq!(row.kind, "ride");
        assert_eq!(row.roles, "main,cool-down");
    });
}

/// The FTP test: a warm-up class, the effort, and a cool-down. Three of
/// Peloton's workouts, one thing nobody would plan separately.
///
/// The warm-up and the effort are recognised by `series_id` — Peloton's own
/// statement that two classes are the same kind of thing. The operator:
/// *"Peloton publishes lots of different FTP warm up and FTP test rides"*, so
/// there is no list of class ids that would work, and his six tests used five
/// distinct test classes over 31 months.
#[test]
fn a_warm_up_an_effort_and_a_cool_down_are_a_test_session() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![
                ("warm", workout("warm", Kind::WarmUp, 100, 700)),
                ("test", workout("test", Kind::Test, 821, 2021)),
                // 310 seconds later — the real gap in his 2026-07-22 test, and
                // the one case a five-minute rule would have split.
                ("cool", workout("cool", Kind::CoolDown, 2331, 2631)),
            ],
            vec![
                ("warm", graph(&[1], &[80], None, "3.0", "km")),
                ("test", graph(&[1], &[250], None, "12.0", "km")),
                ("cool", graph(&[1], &[40], None, "1.0", "km")),
            ],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 1);
        assert!(summary.reconciles());

        let row = sqlx::query!(
            r#"SELECT kind AS "kind!: String",
                      (SELECT GROUP_CONCAT(role, ",") FROM (
                         SELECT role FROM bike_plus_ride ORDER BY started_at_utc
                       )) AS "roles!: String"
               FROM cycling_session"#
        )
        .fetch_one(&pool)
        .await
        .expect("a session");
        assert_eq!(row.kind, "test");
        assert_eq!(row.roles, "warm-up,effort,cool-down");
    });
}

/// A test the operator finished with a low-impact ride instead of a cool-down.
///
/// His 2023-12-27 test, and his reading of it: *"looks like I picked the wrong
/// type of ride for a Cool Down, not something we should try to model
/// explicitly, though I guess we'll need to allow the cool down to be a low
/// impact ride too so we don't lose this FTP test"*. So the class says what it
/// is and the sequence says what it was for.
#[test]
fn a_session_ending_in_a_low_impact_ride_ends_in_a_cool_down() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![
                ("warm", workout("warm", Kind::WarmUp, 100, 700)),
                ("test", workout("test", Kind::Test, 821, 2021)),
                ("extra", workout("extra", Kind::LowImpact, 2109, 2709)),
            ],
            vec![
                ("warm", graph(&[1], &[80], None, "3.0", "km")),
                ("test", graph(&[1], &[250], None, "12.0", "km")),
                ("extra", graph(&[1], &[60], None, "2.0", "km")),
            ],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 1, "the test is kept");

        let row = sqlx::query!(r#"SELECT kind AS "kind!: String" FROM cycling_session"#)
            .fetch_one(&pool)
            .await
            .expect("a session");
        assert_eq!(row.kind, "test");
    });
}

/// A low-impact ride on its own is an ordinary ride, not a cool-down with
/// nothing to cool down from. The same class, read by where it sat.
#[test]
fn a_low_impact_ride_alone_is_a_main_ride() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![("solo", workout("solo", Kind::LowImpact, 100, 1300))],
            vec![("solo", graph(&[1], &[60], None, "6.0", "km"))],
        )
        .await
        .expect("a landed corpus");

        derive(&pool).await.expect("a derivation");
        let row = sqlx::query!(
            r#"SELECT kind AS "kind!: String",
                      (SELECT role FROM bike_plus_ride) AS "role!: String"
               FROM cycling_session"#
        )
        .fetch_one(&pool)
        .await
        .expect("a session");
        assert_eq!((row.kind.as_str(), row.role.as_str()), ("ride", "main"));
    });
}

/// A freestyle ride belongs to no session, even when it sits inside one's span.
///
/// The operator has two: a two-minute Just Ride he started by mistake after an
/// endurance ride, and a mobility video watched on the bike after a cool-down —
/// *"conceptually the same as a main ride + cool down + stretch or yoga"*. Both
/// carry no class at all, so "not part of a session" is something Peloton
/// states rather than something we infer.
#[test]
fn a_freestyle_ride_is_refused_and_does_not_join_the_session_beside_it() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![
                ("main", workout("main", Kind::Main, 100, 1900)),
                ("cool", workout("cool", Kind::CoolDown, 1970, 2270)),
                ("extra", workout("extra", Kind::Freestyle, 2300, 2420)),
            ],
            vec![
                ("main", graph(&[1], &[93], None, "10.0", "km")),
                ("cool", graph(&[1], &[40], None, "1.0", "km")),
                ("extra", graph(&[1], &[20], None, "0.5", "km")),
            ],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 1, "the session stands");
        assert_eq!(summary.records_refused.as_usize(), 2, "the freestyle pair");
        assert!(summary.reconciles());

        let refusals = refusals(&pool).await.expect("refusals");
        assert!(
            refusals
                .iter()
                .all(|refusal| refusal.kind() == RefusalKind::Unmodelled),
            "{refusals:?}"
        );
    });
}

/// A shape neither variant holds. The operator's 2023-12-24 — an Intro ride
/// followed by a Beginner ride, from the Discover programme — which he called
/// *"conceptually, a single session but an anomaly"*. Refused, not forced.
#[test]
fn a_session_of_two_main_rides_is_refused_as_unmodelled() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![
                ("intro", workout("intro", Kind::Main, 100, 1000)),
                ("beginner", workout("beginner", Kind::Main, 1205, 2405)),
            ],
            vec![
                ("intro", graph(&[1], &[93], None, "5.0", "km")),
                ("beginner", graph(&[1], &[93], None, "7.0", "km")),
            ],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 0);
        assert_eq!(summary.records_refused.as_usize(), 4);
        assert!(summary.reconciles());

        let refusals = refusals(&pool).await.expect("refusals");
        assert_eq!(
            refusals.first().and_then(|refusal| refusal.reason.detail()),
            Some("a session of main then main".to_owned())
        );
    });
}

/// Two hours apart is two sessions, however alike they look.
#[test]
fn rides_a_long_way_apart_are_separate_sessions() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![
                ("morning", workout("morning", Kind::Main, 100, 1900)),
                ("evening", workout("evening", Kind::Main, 40_000, 41_800)),
            ],
            vec![
                ("morning", graph(&[1], &[93], None, "10.0", "km")),
                ("evening", graph(&[1], &[93], None, "10.0", "km")),
            ],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 2);
        assert!(summary.reconciles());
    });
}
