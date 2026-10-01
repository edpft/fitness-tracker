//! The canonical layer for gym sessions (#284): one row per visit, merged from
//! whichever normalised sessions recorded it.
//!
//! What is pinned here is the table and the round trip, not the matching —
//! nothing builds these yet, and #247 is the algorithm that will. So the
//! sessions are assembled by hand, in the shapes the operator's record holds:
//! a set whose reps and load come from two different normalised sessions, a
//! session only a sheet dated, a superset, and a watch's heart rate with its
//! samples.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

mod support;

use application::CanonicalGymSessionStore as _;
use domain::{
    canonical::{Attributed, NormalisedSessionId, Occurred, SessionCount},
    gym::{
        CanonicalExercise, CanonicalGymSession, CanonicalItem, CanonicalSet, Guess,
        GuessedExercise, Identified, Load, MeasuredHeartRate, Performed, SetKind,
        exercise::RepsExercise,
    },
    measure::{HeartRateSample, HeartRateSeries, HeartRateSummary, Kg, PositiveDuration, RepCount},
    normalised::{OperatorZone, StartedAt},
    sequence::{AtLeastTwo, NonEmpty},
};
use infrastructure::SqliteCanonicalGymSessionStore;
use jiff::civil::Date;
use sqlx::{Row as _, SqlitePool};

type Failure = Box<dyn std::error::Error>;

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

/// The normalised sessions the corpus derived, oldest first. These are the
/// normalised sessions a canonical session's fields name.
async fn normalised(pool: &SqlitePool) -> Result<Vec<NormalisedSessionId>, Failure> {
    let rows = sqlx::query("SELECT id FROM gym_session ORDER BY id")
        .fetch_all(pool)
        .await?;
    rows.into_iter()
        .map(|row| Ok(NormalisedSessionId::try_from(row.get::<i64, _>("id"))?))
        .collect()
}

fn at(instant: &str) -> Result<Occurred, Failure> {
    Ok(Occurred::At(StartedAt::new(
        instant.parse()?,
        OperatorZone::try_from("Europe/London".to_owned())?,
    )))
}

fn on(day: &str) -> Result<Occurred, Failure> {
    Ok(Occurred::On(day.parse::<Date>()?))
}

fn began(
    instant: &str,
    normalised_session: NormalisedSessionId,
) -> Result<Attributed<StartedAt>, Failure> {
    Ok(Attributed::new(
        StartedAt::new(
            instant.parse()?,
            OperatorZone::try_from("Europe/London".to_owned())?,
        ),
        normalised_session,
    ))
}

/// 2020-10-09's shape: the reps are the sheet's, the load is the watch's, and
/// nothing states a kind or a rest.
fn merged_set(
    reps: u32,
    sheet: NormalisedSessionId,
    grams: u64,
    watch: NormalisedSessionId,
) -> Result<CanonicalSet<RepCount>, Failure> {
    Ok(CanonicalSet {
        outcome: Attributed::new(Performed::Completed(Some(RepCount::new(reps)?)), sheet),
        load: Some(Attributed::new(
            Load::absolute(Kg::from_grams(grams)),
            watch,
        )),
        began: None,
        intensity: None,
        kind: None,
        rest_after: None,
    })
}

/// A set every field of which is one normalised session's, which is what a
/// Hevy-only session looks like.
fn logged_set(
    reps: u32,
    grams: u64,
    normalised_session: NormalisedSessionId,
) -> Result<CanonicalSet<RepCount>, Failure> {
    Ok(CanonicalSet {
        outcome: Attributed::new(
            Performed::Completed(Some(RepCount::new(reps)?)),
            normalised_session,
        ),
        load: Some(Attributed::new(
            Load::absolute(Kg::from_grams(grams)),
            normalised_session,
        )),
        began: None,
        intensity: None,
        kind: Some(Attributed::new(SetKind::Working, normalised_session)),
        rest_after: None,
    })
}

fn heart_rate(
    normalised_session: NormalisedSessionId,
) -> Result<Attributed<MeasuredHeartRate>, Failure> {
    let samples = NonEmpty::of(
        HeartRateSample {
            at: domain::measure::Duration::from_seconds(0),
            beats_per_minute: "92".parse()?,
        },
        vec![
            HeartRateSample {
                at: domain::measure::Duration::from_seconds(7),
                beats_per_minute: "118".parse()?,
            },
            HeartRateSample {
                at: domain::measure::Duration::from_seconds(19),
                beats_per_minute: "133".parse()?,
            },
        ],
    );
    Ok(Attributed::new(
        MeasuredHeartRate::new(
            HeartRateSummary::new("77".parse()?, "133".parse()?),
            Some(HeartRateSeries::new(samples)),
        ),
        normalised_session,
    ))
}

/// Written, read back, and identical — the merge inside a set included.
///
/// 2020-10-09 is the case: `the_beginner_prescription.xlsx` holds the movement
/// and the reps and no load at all, and the watch holds the load, the clock and
/// the heart rate. What the store has to keep is which of those came from
/// which, per field.
#[test]
fn a_merged_session_round_trips_field_by_field() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = support::store::derived_and_authored()
            .await
            .expect("a derived store");
        let ids = normalised(&pool).await.expect("normalised sessions");
        let (sheet, watch) = (ids[0], ids[1]);

        let store = SqliteCanonicalGymSessionStore::new(pool.clone());

        let deadlift = CanonicalExercise::ForReps {
            identified: Attributed::new(Identified::Recorded(RepsExercise::DeadliftBarbell), sheet),
            sets: NonEmpty::of(
                merged_set(4, sheet, 63_000, watch).expect("a set"),
                vec![merged_set(4, sheet, 65_000, watch).expect("a set")],
            ),
        };
        let session = CanonicalGymSession::new(
            at("2020-10-09T07:12:00Z").expect("an instant"),
            NonEmpty::of(CanonicalItem::Exercise(deadlift), vec![]),
            Some(heart_rate(watch).expect("a heart rate")),
            Some(Attributed::new(
                PositiveDuration::from_seconds(3_120).expect("a duration"),
                watch,
            )),
        );

        let written = store.replace(vec![session.clone()]).await.expect("a write");
        assert_eq!(written, SessionCount::from(1));

        let read = store.all().await.expect("a read");
        assert_eq!(read, vec![session.clone()]);

        // And the attribution survives as attribution, not just as bytes.
        let set = read[0]
            .exercises()
            .next()
            .expect("an exercise")
            .normalised_sessions();
        assert_eq!(set, vec![sheet, watch]);
        assert_eq!(read[0].stands_on(), vec![watch, sheet]);
    });
}

/// A superset round-trips as a superset, and a watch's guess round-trips as a
/// guess rather than as something a source recorded.
#[test]
fn a_superset_and_a_guess_both_round_trip() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = support::store::derived_and_authored()
            .await
            .expect("a derived store");
        let ids = normalised(&pool).await.expect("normalised sessions");
        let (log, watch) = (ids[0], ids[1]);

        let store = SqliteCanonicalGymSessionStore::new(pool.clone());

        let thrusters = CanonicalExercise::ForReps {
            identified: Attributed::new(Identified::Recorded(RepsExercise::ThrusterDumbbell), log),
            sets: NonEmpty::of(logged_set(10, 20_000, log).expect("a set"), vec![]),
        };
        let burpees = CanonicalExercise::ForReps {
            identified: Attributed::new(Identified::Recorded(RepsExercise::Burpee), log),
            sets: NonEmpty::of(logged_set(10, 0, log).expect("a set"), vec![]),
        };
        let guessed = CanonicalExercise::ForReps {
            identified: Attributed::new(
                Identified::Guessed(GuessedExercise::Proposed(Guess::Exercise(
                    RepsExercise::BenchPressBarbell,
                ))),
                watch,
            ),
            sets: NonEmpty::of(
                CanonicalSet {
                    outcome: Attributed::new(
                        Performed::Completed(Some(RepCount::new(8).expect("a count"))),
                        watch,
                    ),
                    load: Some(Attributed::new(
                        Load::absolute(Kg::from_grams(40_000)),
                        watch,
                    )),
                    began: Some(began("2025-02-19T12:34:00Z", watch).expect("a clock")),
                    intensity: None,
                    kind: None,
                    rest_after: None,
                },
                vec![],
            ),
        };

        let session = CanonicalGymSession::new(
            on("2025-02-19").expect("a day"),
            NonEmpty::of(
                CanonicalItem::Superset(Box::new(AtLeastTwo::of(thrusters, burpees, vec![]))),
                vec![CanonicalItem::Exercise(guessed)],
            ),
            None,
            None,
        );

        store.replace(vec![session.clone()]).await.expect("a write");
        let read = store.all().await.expect("a read");

        assert_eq!(read, vec![session]);
        assert_eq!(read[0].set_count(), 3);
    });
}

/// The layer is replaced rather than added to, and read back oldest first
/// whether a session is dated or instant.
#[test]
fn a_replacement_replaces_and_reads_oldest_first() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = support::store::derived_and_authored()
            .await
            .expect("a derived store");
        let ids = normalised(&pool).await.expect("normalised sessions");
        let normalised_session = ids[0];

        let store = SqliteCanonicalGymSessionStore::new(pool.clone());

        let one = |occurred: Occurred| {
            CanonicalGymSession::new(
                occurred,
                NonEmpty::of(
                    CanonicalItem::Exercise(CanonicalExercise::ForReps {
                        identified: Attributed::new(
                            Identified::Recorded(RepsExercise::BackSquatBarbell),
                            normalised_session,
                        ),
                        sets: NonEmpty::of(
                            logged_set(5, 100_000, normalised_session).expect("a set"),
                            vec![],
                        ),
                    }),
                    vec![],
                ),
                None,
                None,
            )
        };
        let older = one(on("2019-03-14").expect("a day"));
        let newer = one(at("2025-02-19T12:30:30Z").expect("an instant"));

        store
            .replace(vec![
                newer.clone(),
                older.clone(),
                one(on("2021-01-01").expect("a day")),
            ])
            .await
            .expect("a write");
        assert_eq!(store.count().await.expect("a count"), SessionCount::from(3));

        let written = store
            .replace(vec![newer.clone(), older.clone()])
            .await
            .expect("a second write");
        assert_eq!(written, SessionCount::from(2));
        assert_eq!(store.count().await.expect("a count"), SessionCount::from(2));

        // Written newest first and read oldest first, so the order is the
        // store's rather than the caller's.
        assert_eq!(store.all().await.expect("a read"), vec![older, newer]);
    });
}
