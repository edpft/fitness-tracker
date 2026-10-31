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
    CanonicalCyclingSessionStore as _, ExtractionRunLog as _, NormalisationSummary,
    NormalisedCyclingSessionReader as _, PerformedCyclingSessions as _, RefusalReporter as _,
    RiddenSessionLog as _, RiddenVenues as _, WorkoutNormaliser as _,
    canonicalise::{self, Canonicalised},
    normalise::{Normalisation, NormalisationPorts, Refusals},
    ports::Translator as _,
};
use domain::normalised::StatedZones;
use domain::{
    landing::{
        Endpoint, EventKind, EventProvenance, FetchedAt, LandingRecord, LandingStream, RawPayload,
        RunId, SourceRecordId,
    },
    normalised::{OperatorZone, RefusalKind, RefusalReason},
};
use infrastructure::{
    PelotonRideLandingStore, PelotonRideSampleLandingStore, PelotonSessionAccountReader,
    SqliteCanonicalCyclingSessionStore, SqliteCyclingSessionStore, SqliteExtractionRunLog,
    SqliteNormalisationRunLog, SqliteNormalisedCyclingSessionReader, SqliteRefusalStore, connect,
    peloton::PelotonSessionTranslator,
};
use sqlx::SqlitePool;

use crate::support::{layers, versions};

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
    let at = class_id(id);
    let (workout_type, class) = match kind {
        Kind::Freestyle => ("freestyle", "null".to_owned()),
        Kind::Main => ("class", class_of(&at, POWER_ZONE_SERIES, &[])),
        Kind::CoolDown => ("class", class_of(&at, POWER_ZONE_SERIES, &[COOL_DOWN])),
        Kind::LowImpact => ("class", class_of(&at, POWER_ZONE_SERIES, &[LOW_IMPACT])),
        Kind::WarmUp => ("class", class_of(&at, FTP_WARM_UP_SERIES, &[])),
        Kind::Test => ("class", class_of(&at, FTP_TEST_SERIES, &[])),
    };
    format!(
        r#"{{"id":"{id}","fitness_discipline":"{discipline}","device_type":"{device}",
            "is_outdoor":false,"start_time":{start},"end_time":{end},"distance":1.3213,
            "workout_type":"{workout_type}","ride":{class}}}"#
    )
}

/// The class a workout was ridden to.
///
/// **One per workout**, so a suite asserting which class a ride names cannot
/// pass by accident: a single shared id would make every ride's venue equal and
/// the assertion vacuous.
fn class_id(workout: &str) -> String {
    format!("class-of-{workout}")
}

fn class_of(id: &str, series: &str, class_types: &[&str]) -> String {
    let types = class_types
        .iter()
        .map(|id| format!("\"{id}\""))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"id":"{id}","title":"a class","series_id":"{series}","class_type_ids":[{types}]}}"#
    )
}

/// The average power every fixture graph states.
///
/// **Deliberately not the mean of any fixture's samples.** Peloton states its
/// own average and it is not the mean of the series beside it — on the
/// operator's record the two disagree on 33 of 231 rides, once by 10 watts — so
/// a fixture whose stated average happened to equal its sample mean would pass
/// whether the translator read the source's figure or computed one.
const STATED_AVERAGE_POWER: u32 = 199;

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
             "average_summaries":[
               {{"slug":"avg_output","display_unit":"watts","value":{STATED_AVERAGE_POWER}}}
             ],
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

/// A graph stating an average of the caller's choosing.
///
/// `graph` states [`STATED_AVERAGE_POWER`] for every ride; a test needs to
/// state the average whose arithmetic is under test.
fn graph_averaging(offsets: &[u32], power: &[u32], average: u32) -> String {
    graph(offsets, power, None, "2.0", "km").replace(
        &format!(r#""value":{STATED_AVERAGE_POWER}"#),
        &format!(r#""value":{average}"#),
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
    derive_as(pool, application::Rederivation::Whole).await
}

async fn derive_as(
    pool: &SqlitePool,
    rederivation: application::Rederivation,
) -> Built<NormalisationSummary> {
    let normalisation = Normalisation::new(
        NormalisationPorts {
            raw: PelotonSessionAccountReader::new(pool.clone())?,
            translator: PelotonSessionTranslator,
            workouts: SqliteCyclingSessionStore::new(pool.clone())?,
            refusals: SqliteRefusalStore::new(pool.clone(), PelotonRideLandingStore::STREAM)?,
            runs: SqliteNormalisationRunLog::new(pool.clone()),
            clock: FixedClock,
        },
        StatedZones::throughout(OperatorZone::try_from("Europe/London")?),
    );
    Ok(normalisation.normalise(rederivation).await?)
}

/// Rebuild the canonical cycling layer from what `derive` wrote.
async fn canonicalise(pool: &SqlitePool) -> Built<Canonicalised> {
    Ok(canonicalise::cycling_sessions(
        &SqliteNormalisedCyclingSessionReader::new(pool.clone()),
        &SqliteCanonicalCyclingSessionStore::new(pool.clone()),
    )
    .await?)
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

/// **A ride of no length is refused as nothing having happened** (#326), and
/// one that ends before it starts as a value this adapter cannot read. The
/// operator, 2026-09-30: *"A 0 duration activity isn't an activity, by
/// definition, nothing happened."*
#[test]
fn a_ride_of_no_length_is_refused_as_nothing_having_happened() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        for ((start, end), expected) in [
            ((400, 400), "nothing-happened"),
            ((400, 100), "unreadable-value"),
        ] {
            let (pool, _directory) = landed(
                vec![("ride-1", workout("ride-1", Kind::Main, start, end))],
                vec![("ride-1", graph(&[1], &[93], None, "2.0", "km"))],
            )
            .await
            .expect("a landed corpus");

            let summary = derive(&pool)
                .await
                .expect("a derivation, not a store failure");
            assert_eq!(summary.workouts_written.as_usize(), 0);

            let refusals = refusals(&pool).await.expect("refusals");
            let reasons: Vec<&str> = refusals
                .iter()
                .map(|refusal| refusal.reason.as_str())
                .collect();
            assert!(reasons.contains(&expected), "{start}..{end}: {reasons:?}");
        }
    });
}

/// The FTP table is derived with the sessions, and re-derived with them.
///
/// **Both halves matter** (issue #56). The value is what the test measured, and
/// running the derivation again leaves the same row rather than a second one —
/// which is what makes rebuilding the table safe and freezing a row
/// unnecessary.
#[test]
fn a_test_session_writes_the_ftp_it_measured_and_rewrites_it() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        // 209 W stated, which is the operator's 2024-09-15 test: it rounds to
        // 199 and truncates to 198, so it tells the two apart.
        let (pool, _directory) = landed(
            vec![
                ("warm-up", workout("warm-up", Kind::WarmUp, 100, 400)),
                ("effort", workout("effort", Kind::Test, 400, 1600)),
            ],
            vec![
                ("warm-up", graph(&[1, 2], &[93, 132], None, "2.0", "km")),
                ("effort", graph_averaging(&[1, 2], &[93, 132], 209)),
            ],
        )
        .await
        .expect("a landed corpus");

        for pass in 1..=2 {
            let summary = derive(&pool).await.expect("a derivation");
            assert_eq!(summary.workouts_written.as_usize(), 1, "pass {pass}");

            let rows = sqlx::query!(
                r#"SELECT effect_from AS "from!: String",
                          watts       AS "watts!: i64",
                          provenance  AS "provenance!: String"
                     FROM ftp"#
            )
            .fetch_all(&pool)
            .await
            .expect("the series");

            assert_eq!(rows.len(), 1, "one row, not one per pass (pass {pass})");
            assert_eq!(rows[0].watts, 199, "209 W rounds to 199");
            assert_eq!(rows[0].provenance, "estimated");
            // The day the test was ridden: 400 seconds after the epoch is the
            // warm-up's start, which is the session's.
            assert_eq!(rows[0].from, "1970-01-01");
        }
    });
}

/// An ordinary ride measures no FTP, so the series stays empty.
#[test]
fn a_ride_writes_no_ftp() {
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

        let row = sqlx::query!(r#"SELECT COUNT(*) AS "held!: i64" FROM ftp"#)
            .fetch_one(&pool)
            .await
            .expect("a count");
        assert_eq!(row.held, 0);
    });
}

/// The average power is the source's, not one computed from the samples.
///
/// **The distinction decides FTP figures.** Every FTP value in the operator's
/// record is a twenty-minute test's average output times 0.95, and only
/// Peloton's stated average reproduces all six of them: a mean of the samples
/// gets two of the six wrong. Peloton computes over what it measured, and its
/// series is sparse — one ride states 134 watts where its 1,076 samples across
/// an index running to 1,800 average 124.
#[test]
fn the_average_power_is_the_one_the_source_stated() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        // A mean of 100, against a graph that states 199.
        let (pool, _directory) = landed(
            vec![("ride-1", workout("ride-1", Kind::Main, 100, 400))],
            vec![("ride-1", graph(&[1, 2], &[50, 150], None, "2.0", "km"))],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 1);

        let row =
            sqlx::query!(r#"SELECT average_power_watts AS "watts!: i64" FROM bike_plus_ride"#)
                .fetch_one(&pool)
                .await
                .expect("the ride");
        assert_eq!(
            row.watts,
            i64::from(STATED_AVERAGE_POWER),
            "the source's average, not the samples' mean of 100"
        );
    });
}

/// A graph that states no average refuses the ride rather than being given one.
///
/// Inventing the number is the error the stated figure exists to avoid, so
/// there is nothing to fall back to (§ 37).
#[test]
fn a_graph_stating_no_average_power_refuses_the_ride() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        // Written out rather than taken from `graph`, which always states one.
        let without = r#"{"seconds_since_pedaling_start":[1,2],
             "summaries":[{"slug":"distance","display_unit":"km","value":2.0}],
             "metrics":[
               {"slug":"output","display_unit":"watts","values":[93,132]},
               {"slug":"cadence","display_unit":"rpm","values":[80,80]},
               {"slug":"resistance","display_unit":"%","values":[35,35]},
               {"slug":"speed","display_unit":"kph","values":[25.4,25.4]}
             ]}"#
        .to_owned();

        let (pool, _directory) = landed(
            vec![("ride-1", workout("ride-1", Kind::Main, 100, 400))],
            vec![("ride-1", without)],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 0);

        // The store folds the field name into the detail on the way back, so
        // the name is read there rather than off `field`.
        let refusals = refusals(&pool).await.expect("refusals");
        assert!(
            matches!(
                refusals.first().map(|refusal| &refusal.reason),
                Some(RefusalReason::UnreadableValue { detail, .. })
                    if detail.starts_with("avg_output")
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

/// **The class a ride was ridden to reaches the store, and reads back.**
///
/// This is what answers *"the newest that hasn't already been taken"* (#180).
/// Until 2026-09-20 nothing above raw held a class id: the payload carried one
/// and `bike_plus_ride` had no column for it, so a holding microcycle could
/// have offered the operator a ride he did last Tuesday.
///
/// **Every ride of a session, not only the main one.** A cool-down is a class
/// he has taken too, and a selection that saw only main rides would offer one
/// back.
#[test]
fn every_ride_records_the_class_it_was_ridden_to() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![
                ("main", workout("main", Kind::Main, 100, 1900)),
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

        canonicalise(&pool).await.expect("a canonical layer");
        let ridden = SqliteCanonicalCyclingSessionStore::new(pool.clone())
            .ridden()
            .await
            .expect("the venues read back");

        let references: Vec<&str> = {
            let mut seen: Vec<&str> = ridden
                .iter()
                .map(domain::cycling::RideVenue::reference)
                .collect();
            seen.sort_unstable();
            seen
        };
        assert_eq!(references, vec!["class-of-cool", "class-of-main"]);
    });
}

/// **A class ridden twice is one entry, not two.** The operator rides the same
/// cool-down over and over — 126 of them in his record share a handful of
/// classes — and a selection asking "has this been ridden" wants a set.
#[test]
fn a_class_ridden_twice_is_named_once() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        // Two sessions far enough apart not to group, both ridden to the same
        // class: `class_id` keys on the workout id, so they are given the same
        // one by hand here.
        let same = r#"{"id":"%ID%","fitness_discipline":"cycling","device_type":"home_bike_plus",
            "is_outdoor":false,"start_time":%START%,"end_time":%END%,"distance":1.3213,
            "workout_type":"class",
            "ride":{"id":"one-class","title":"45 min Power Zone Ride",
                    "series_id":"0f63c48726fa4533a928cae5358d94d7","class_type_ids":[]}}"#;
        let build = |id: &str, start: i64, end: i64| {
            same.replace("%ID%", id)
                .replace("%START%", &start.to_string())
                .replace("%END%", &end.to_string())
        };

        let (pool, _directory) = landed(
            vec![
                ("first", build("first", 100, 1900)),
                ("second", build("second", 40_000, 41_800)),
            ],
            vec![
                ("first", graph(&[1], &[93], None, "10.0", "km")),
                ("second", graph(&[1], &[93], None, "10.0", "km")),
            ],
        )
        .await
        .expect("a landed corpus");

        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(summary.workouts_written.as_usize(), 2, "two sessions");

        canonicalise(&pool).await.expect("a canonical layer");
        let ridden = SqliteCanonicalCyclingSessionStore::new(pool.clone())
            .ridden()
            .await
            .expect("the venues read back");
        assert_eq!(ridden.len(), 1, "{ridden:?}");
    });
}

/// **An FTP test is one canonical session, and every field names the
/// normalised session it came from** (#408). The warm-up and the effort are two
/// workouts to Peloton and one session here, as they are one normalised
/// session; `next` reads them back as one ridden session naming both classes.
#[test]
fn a_test_is_one_canonical_session_whose_fields_name_where_they_came_from() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![
                ("warm-up", workout("warm-up", Kind::WarmUp, 100, 400)),
                ("effort", workout("effort", Kind::Test, 400, 1600)),
            ],
            vec![
                ("warm-up", graph(&[1, 2], &[93, 132], None, "2.0", "km")),
                (
                    "effort",
                    graph(
                        &[1, 2, 3],
                        &[180, 200, 210],
                        Some((&[120, 140, 150], "0")),
                        "8.0",
                        "km",
                    ),
                ),
            ],
        )
        .await
        .expect("a landed corpus");
        derive(&pool).await.expect("a derivation");

        let done = canonicalise(&pool).await.expect("a canonical layer");
        assert_eq!(done.read.as_usize(), 1);
        assert_eq!(done.written.as_usize(), 1);

        let rides = sqlx::query!(
            r#"SELECT r.role AS "role!: String",
                      r.started_at_normalised_session AS "started!: i64",
                      r.venue_normalised_session AS "venue!: i64",
                      r.duration_normalised_session AS "duration!: i64",
                      r.distance_normalised_session AS "distance!: i64",
                      r.average_power_normalised_session AS "power!: i64",
                      r.samples_normalised_session AS "samples!: i64",
                      r.heart_rate_normalised_session AS "heart_rate: i64",
                      (SELECT count(*) FROM canonical_bike_plus_ride_sample AS s
                        WHERE s.session = r.session AND s.ride_position = r.position)
                        AS "sample_count!: i64",
                      (SELECT count(*) FROM canonical_bike_plus_ride_heart_rate AS h
                        WHERE h.session = r.session AND h.ride_position = r.position)
                        AS "beat_count!: i64",
                      (SELECT kind FROM canonical_cycling_session AS c WHERE c.id = r.session)
                        AS "kind!: String"
                 FROM canonical_bike_plus_ride AS r
                ORDER BY r.position"#
        )
        .fetch_all(&pool)
        .await
        .expect("the canonical rides");
        let normalised =
            sqlx::query_scalar!(r#"SELECT landing_record_id AS "id!: i64" FROM cycling_session"#)
                .fetch_one(&pool)
                .await
                .expect("the normalised session");

        let roles: Vec<&str> = rides.iter().map(|ride| ride.role.as_str()).collect();
        assert_eq!(roles, vec!["warm-up", "effort"]);
        for ride in &rides {
            assert_eq!(ride.kind, "test");
            for named in [
                ride.started,
                ride.venue,
                ride.duration,
                ride.distance,
                ride.power,
                ride.samples,
            ] {
                assert_eq!(named, normalised, "{} names its session", ride.role);
            }
        }
        assert_eq!(rides[0].sample_count, 2);
        assert_eq!(rides[0].heart_rate, None, "the warm-up had no heart rate");
        assert_eq!(rides[1].sample_count, 3);
        assert_eq!(rides[1].heart_rate, Some(normalised));
        assert_eq!(rides[1].beat_count, 3);

        let day = jiff::civil::date(1970, 1, 1);
        let ridden = SqliteCanonicalCyclingSessionStore::new(pool.clone())
            .ridden_between(day, day)
            .await
            .expect("the sessions ridden");
        assert_eq!(ridden.len(), 1, "one session, not two rides");
        let classes: Vec<&str> = ridden[0]
            .at
            .iter()
            .map(domain::cycling::RideVenue::reference)
            .collect();
        assert_eq!(classes.len(), 2, "both classes: {classes:?}");
    });
}

/// **The canonical layer reads back as it was written**: what the analytical
/// layer reads (#152) is the session canonicalising built, every field naming
/// where it came from and every series in full.
#[test]
fn the_canonical_layer_reads_back_the_sessions_it_holds() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![
                ("warm-up", workout("warm-up", Kind::WarmUp, 100, 400)),
                ("effort", workout("effort", Kind::Test, 400, 1600)),
                ("ride-1", workout("ride-1", Kind::Main, 90_000, 92_700)),
            ],
            vec![
                ("warm-up", graph(&[1, 2], &[93, 132], None, "2.0", "km")),
                (
                    "effort",
                    graph(
                        &[1, 2, 3],
                        &[180, 200, 210],
                        Some((&[120, 140, 150], "0")),
                        "8.0",
                        "km",
                    ),
                ),
                (
                    "ride-1",
                    graph(&[1, 2], &[140, 150], Some((&[110, 115], "3")), "20.0", "km"),
                ),
            ],
        )
        .await
        .expect("a landed corpus");
        derive(&pool).await.expect("a derivation");
        canonicalise(&pool).await.expect("a canonical layer");

        let built = domain::cycling::canonical_sessions(
            SqliteNormalisedCyclingSessionReader::new(pool.clone())
                .all()
                .await
                .expect("the normalised sessions"),
        );
        let read = SqliteCanonicalCyclingSessionStore::new(pool.clone())
            .all()
            .await
            .expect("the canonical sessions");
        assert_eq!(read.len(), 2, "a test and a ride");
        assert_eq!(read, built);
    });
}

/// **Re-deriving the rides empties the canonical layer**, because every field
/// of it names a normalised session the derivation deletes; the next
/// canonicalising builds it again.
#[test]
fn re_deriving_the_rides_empties_the_canonical_layer_until_it_is_rebuilt() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = landed(
            vec![("ride-1", workout("ride-1", Kind::Main, 100, 400))],
            vec![("ride-1", graph(&[1], &[93], None, "2.0", "km"))],
        )
        .await
        .expect("a landed corpus");
        let canonical = SqliteCanonicalCyclingSessionStore::new(pool.clone());

        derive(&pool).await.expect("a derivation");
        canonicalise(&pool).await.expect("a canonical layer");
        assert_eq!(canonical.count().await.expect("a count").as_usize(), 1);

        derive(&pool).await.expect("a second derivation");
        assert_eq!(canonical.count().await.expect("a count").as_usize(), 0);

        canonicalise(&pool).await.expect("the layer rebuilt");
        assert_eq!(canonical.count().await.expect("a count").as_usize(), 1);
    });
}

// --- Deriving only what landed since (#571) ---------------------------------

/// Peloton's account as asked at `at`, paired with one monitor.
fn answer(at: &str) -> Built<LandingRecord> {
    Ok(LandingRecord::land(
        LandingStream::try_from("peloton.account")?,
        FetchedAt::try_from(at)?,
        SourceRecordId::try_from("me")?,
        EventProvenance::new(Endpoint::try_from("/api/me")?, EventKind::Updated, None).into(),
        RawPayload::try_from(
            r#"{"id":"me","paired_devices":[{"name":"Device: 3629",
                "paired_device_type":"heart_rate_monitor","serial_number":"3629"}]}"#
                .as_bytes(),
        )?,
    ))
}

/// Workouts, their graphs and the account's answers, as one batch lands them.
type Batch<'a> = (
    Vec<(&'a str, String)>,
    Vec<(&'a str, String)>,
    Vec<LandingRecord>,
);

/// Two batches of rides and graphs landed one after the other, each with the
/// account answers given, deriving between them as `between` says.
async fn in_two_batches(
    first: Batch<'_>,
    second: Batch<'_>,
    between: bool,
) -> Built<(SqlitePool, tempfile::TempDir, Option<NormalisationSummary>)> {
    use application::LandingStore as _;

    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;
    let rides = PelotonRideLandingStore::new(pool.clone())?;
    let samples = PelotonRideSampleLandingStore::new(pool.clone())?;
    let account = infrastructure::PelotonAccountLandingStore::new(pool.clone())?;
    let runs = SqliteExtractionRunLog::new(pool.clone());

    let (workouts, graphs, answers) = first;
    append(&rides, &runs, "/api/user/u/workouts", workouts).await?;
    append(&samples, &runs, "/api/workout/w/performance_graph", graphs).await?;
    let run = runs.begin(account.stream(), FetchedAt::EPOCH).await?;
    account.append(run, answers).await?;
    if between {
        derive_as(&pool, application::Rederivation::Incremental).await?;
    }

    let (workouts, graphs, answers) = second;
    append(&rides, &runs, "/api/user/u/workouts", workouts).await?;
    append(&samples, &runs, "/api/workout/w/performance_graph", graphs).await?;
    let run = runs.begin(account.stream(), FetchedAt::EPOCH).await?;
    account.append(run, answers).await?;
    let summary = derive_as(&pool, application::Rederivation::Incremental).await?;
    Ok((pool, directory, between.then_some(summary)))
}

fn plain_graph() -> String {
    graph(
        &[1, 2, 3],
        &[150, 160, 170],
        Some((&[120, 121, 122], "0")),
        "2.0",
        "km",
    )
}

/// The same raw, derived whole and derived across two runs, holds the same
/// rows — and the second run translates only the sessions the second batch
/// changed: a ride whose cool-down landed later, a ride whose graph did, and a
/// ride of its own.
#[test]
fn an_incremental_derivation_writes_what_a_whole_one_would() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let batches = || {
            (
                (
                    vec![
                        ("ride-1", workout("ride-1", Kind::Main, 100, 400)),
                        ("ride-2", workout("ride-2", Kind::CoolDown, 450, 600)),
                        ("ride-3", workout("ride-3", Kind::WarmUp, 10_000, 10_600)),
                        ("ride-4", workout("ride-4", Kind::Test, 10_700, 11_900)),
                        ("ride-5", workout("ride-5", Kind::Main, 50_000, 50_300)),
                        ("ride-8", workout("ride-8", Kind::Main, 70_000, 70_300)),
                    ],
                    vec![
                        ("ride-1", plain_graph()),
                        ("ride-2", plain_graph()),
                        ("ride-3", plain_graph()),
                        ("ride-4", graph_averaging(&[1, 2, 3], &[200, 210, 220], 209)),
                        ("ride-5", plain_graph()),
                    ],
                    Vec::new(),
                ),
                (
                    vec![
                        ("ride-6", workout("ride-6", Kind::CoolDown, 50_350, 50_500)),
                        ("ride-7", workout("ride-7", Kind::Main, 90_000, 90_300)),
                    ],
                    vec![
                        ("ride-6", plain_graph()),
                        ("ride-7", plain_graph()),
                        ("ride-8", plain_graph()),
                    ],
                    Vec::new(),
                ),
            )
        };
        let (first, second) = batches();
        let (whole, _whole_directory, _) = in_two_batches(first, second, false)
            .await
            .expect("a whole derivation");
        let (first, second) = batches();
        let (incremental, _incremental_directory, summary) = in_two_batches(first, second, true)
            .await
            .expect("an incremental derivation");

        assert_eq!(
            layers::dump(&incremental).await.expect("a dump"),
            layers::dump(&whole).await.expect("a dump"),
        );
        let summary = summary.expect("the second run's summary");
        // Ride 5 with its new cool-down, ride 7, and ride 8 now its graph has
        // landed: not the warm-up and test, nor ride 1 and its cool-down.
        assert_eq!(summary.workouts_written.as_usize(), 3);
        assert!(summary.reconciles(), "every record read has one outcome");
        assert_eq!(summary.records_covered.as_usize(), 16);
    });
}

/// A new account answer gives the rides since the last one their monitor, and
/// an incremental derivation finds them without being told.
#[test]
fn a_new_account_answer_reaches_the_rides_before_it() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let batches = || -> Built<_> {
            Ok((
                (
                    vec![
                        ("ride-1", workout("ride-1", Kind::Main, 100, 400)),
                        ("ride-2", workout("ride-2", Kind::Main, 90_000, 90_300)),
                    ],
                    vec![("ride-1", plain_graph()), ("ride-2", plain_graph())],
                    vec![answer("1970-01-01T00:00:50Z")?],
                ),
                (
                    Vec::new(),
                    Vec::new(),
                    vec![answer("1970-01-02T02:00:00Z")?],
                ),
            ))
        };
        let (first, second) = batches().expect("fixtures");
        let (whole, _whole_directory, _) = in_two_batches(first, second, false)
            .await
            .expect("a whole derivation");
        let (first, second) = batches().expect("fixtures");
        let (incremental, _incremental_directory, summary) = in_two_batches(first, second, true)
            .await
            .expect("an incremental derivation");

        assert_eq!(
            layers::dump(&incremental).await.expect("a dump"),
            layers::dump(&whole).await.expect("a dump"),
        );
        let named = sqlx::query_scalar!(
            r#"SELECT count(*) AS "named!: i64" FROM bike_plus_ride
                WHERE heart_rate_device IS NOT NULL"#
        )
        .fetch_one(&incremental)
        .await
        .expect("a count");
        assert_eq!(named, 2, "both rides now have an answer either side");
        assert_eq!(
            summary
                .expect("the second run's summary")
                .workouts_written
                .as_usize(),
            2
        );
    });
}

/// **Its version names what it translates** (#606): a ride of each kind the
/// suite lands, with its graph or without one, in miles and in a unit nobody
/// rides in, and a session ridden on something other than the bike.
#[test]
fn its_version_names_what_it_translates() {
    let runtime = runtime().expect("a runtime");
    let translations = runtime.block_on(async {
        let hour = 3_600;
        let (pool, _directory) = landed(
            vec![
                ("main", workout("main", Kind::Main, 100, 400)),
                ("cool", workout("cool", Kind::CoolDown, 400, 700)),
                ("low", workout("low", Kind::LowImpact, hour, hour + 300)),
                (
                    "warm",
                    workout("warm", Kind::WarmUp, 2 * hour, 2 * hour + 300),
                ),
                (
                    "test",
                    workout("test", Kind::Test, 2 * hour + 300, 2 * hour + 1_500),
                ),
                (
                    "free",
                    workout("free", Kind::Freestyle, 3 * hour, 3 * hour + 300),
                ),
                (
                    "bare",
                    workout("bare", Kind::Main, 4 * hour, 4 * hour + 300),
                ),
                (
                    "miles",
                    workout("miles", Kind::Main, 5 * hour, 5 * hour + 300),
                ),
                ("odd", workout("odd", Kind::Main, 6 * hour, 6 * hour + 300)),
                (
                    "yoga",
                    ridden(
                        "yoga",
                        Kind::Main,
                        "yoga",
                        "home_bike_plus",
                        7 * hour,
                        7 * hour + 300,
                    ),
                ),
                (
                    "garmin",
                    ridden(
                        "garmin",
                        Kind::Main,
                        "cycling",
                        "garmin",
                        8 * hour,
                        8 * hour + 300,
                    ),
                ),
            ],
            vec![
                (
                    "main",
                    graph(
                        &[4, 5, 6, 8],
                        &[93, 132, 120, 110],
                        Some((&[132, 131, 0, 130], "7")),
                        "2.12642",
                        "km",
                    ),
                ),
                (
                    "cool",
                    graph(&[1, 2], &[93, 132], Some((&[132, 131], "3")), "2.0", "km"),
                ),
                ("low", graph(&[1, 2], &[93, 132], None, "2.0", "km")),
                ("warm", graph(&[1, 2], &[93, 132], None, "2.0", "km")),
                ("test", graph(&[1, 2], &[93, 132], None, "2.0", "km")),
                ("free", graph(&[1, 2], &[93, 132], None, "2.0", "km")),
                ("miles", graph(&[1], &[93], None, "1.3213", "mi")),
                ("odd", graph(&[1], &[93], None, "2.0", "furlongs")),
            ],
        )
        .await
        .expect("a landed corpus");
        let reader = PelotonSessionAccountReader::new(pool.clone()).expect("a reader");
        let zones = StatedZones::throughout(OperatorZone::try_from("Europe/London").expect("zone"));
        versions::accounts(&reader)
            .await
            .expect("the accounts")
            .iter()
            .map(|account| {
                PelotonSessionTranslator
                    .translate(account, &zones)
                    .expect("translated")
            })
            .collect::<Vec<_>>()
    });

    if let Err(unpinned) = versions::pinned::<PelotonSessionTranslator>(&translations) {
        panic!("{unpinned}");
    }
}
