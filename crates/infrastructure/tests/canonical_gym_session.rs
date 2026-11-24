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

use crate::support;
use domain::gym::edit::Edits;

use application::{CanonicalGymSessionStore as _, PerformedGymSessions as _};
use domain::{
    canonical::{Attributed, NormalisedSessionId, Occurred, SessionCount, TakenFrom},
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
) -> Result<Attributed<StartedAt, TakenFrom>, Failure> {
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
        sources: std::collections::BTreeSet::new(),
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
        sources: std::collections::BTreeSet::new(),
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
        let started = at("2020-10-09T07:12:00Z").expect("an instant");
        let ended: jiff::Timestamp = "2020-10-09T08:04:00Z".parse().expect("an instant");
        let session = CanonicalGymSession::new(
            Attributed::new(started.clone(), watch),
            NonEmpty::of(CanonicalItem::exercise(deadlift), vec![]),
            Some(heart_rate(watch).expect("a heart rate")),
            Some(Attributed::new(ended, watch)),
            vec![
                domain::gym::Account {
                    stream: "spreadsheets.files".parse().expect("a stream"),
                    normalised: sheet,
                    occurred: on("2020-10-09").expect("a day"),
                    ended: None,
                },
                domain::gym::Account {
                    stream: "garmin.activities".parse().expect("a stream"),
                    normalised: watch,
                    occurred: started,
                    ended: Some(ended),
                },
            ],
        );

        let written = store
            .replace(vec![session.clone()], &Edits::default())
            .await
            .expect("a write");
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
                    sources: std::collections::BTreeSet::new(),
                },
                vec![],
            ),
        };

        let session = CanonicalGymSession::new(
            domain::canonical::Attributed::edited(on("2025-02-19").expect("a day")),
            NonEmpty::of(
                CanonicalItem::Superset(Box::new(AtLeastTwo::of(thrusters, burpees, vec![]))),
                vec![CanonicalItem::exercise(guessed)],
            ),
            None,
            None,
            Vec::new(),
        );

        store
            .replace(vec![session.clone()], &Edits::default())
            .await
            .expect("a write");
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
                domain::canonical::Attributed::edited(occurred),
                NonEmpty::of(
                    CanonicalItem::exercise(CanonicalExercise::ForReps {
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
                Vec::new(),
            )
        };
        let older = one(on("2019-03-14").expect("a day"));
        let newer = one(at("2025-02-19T12:30:30Z").expect("an instant"));

        store
            .replace(
                vec![
                    newer.clone(),
                    older.clone(),
                    one(on("2021-01-01").expect("a day")),
                ],
                &Edits::default(),
            )
            .await
            .expect("a write");
        assert_eq!(store.count().await.expect("a count"), SessionCount::from(3));

        let written = store
            .replace(vec![newer.clone(), older.clone()], &Edits::default())
            .await
            .expect("a second write");
        assert_eq!(written, SessionCount::from(2));
        assert_eq!(store.count().await.expect("a count"), SessionCount::from(2));

        // Written newest first and read oldest first, so the order is the
        // store's rather than the caller's.
        assert_eq!(store.all().await.expect("a read"), vec![older, newer]);
    });
}

/// **A read spanning a rebuild sees the layer it began on, whole** (#467).
///
/// On 2026-10-16 a page read during `next` showed front squat with no test.
/// The layer is replaced in one transaction under fresh ids, and a session is
/// read in several queries — its row, then its items and sets — so a read that
/// straddled the commit could take its rows from before and its sets from
/// after. A reading is one snapshot: the layer replaced between its two reads,
/// it answers both times with the layer as it stood, and the next read answers
/// with the new one.
#[test]
fn a_reading_spanning_a_replacement_sees_the_layer_it_began_on() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = support::store::derived_and_authored()
            .await
            .expect("a derived store");
        let ids = normalised(&pool).await.expect("normalised sessions");
        let normalised_session = ids[0];
        let store = SqliteCanonicalGymSessionStore::new(pool.clone());

        let squatted = |day: &str, grams: u64| {
            CanonicalGymSession::new(
                domain::canonical::Attributed::edited(on(day).expect("a day")),
                NonEmpty::of(
                    CanonicalItem::exercise(CanonicalExercise::ForReps {
                        identified: Attributed::new(
                            Identified::Recorded(RepsExercise::FrontSquatBarbell),
                            normalised_session,
                        ),
                        sets: NonEmpty::of(
                            logged_set(1, grams, normalised_session).expect("a set"),
                            vec![],
                        ),
                    }),
                    vec![],
                ),
                None,
                None,
                Vec::new(),
            )
        };
        let before = vec![squatted("2026-09-25", 92_500)];
        let after = vec![
            squatted("2026-09-25", 92_500),
            squatted("2026-10-02", 95_000),
        ];
        let (from, to) = (
            "2026-09-01".parse::<Date>().expect("a day"),
            "2026-10-31".parse::<Date>().expect("a day"),
        );
        store
            .replace(before.clone(), &Edits::default())
            .await
            .expect("a write");

        let mut reading = store.reading().await.expect("a reading");
        assert_eq!(
            reading.between(from, to).await.expect("a read"),
            before,
            "the first read is the layer as it stands"
        );

        store
            .replace(after.clone(), &Edits::default())
            .await
            .expect("a rebuild while the reading is open");

        assert_eq!(
            reading.between(from, to).await.expect("a second read"),
            before,
            "the second read is the layer the reading began on, whole"
        );
        assert_eq!(reading.all().await.expect("a third read"), before);
        drop(reading);

        assert_eq!(
            store.between(from, to).await.expect("a fresh read"),
            after,
            "a read begun after the rebuild sees it"
        );
    });
}

/// **A session as History lists it** (#817): its length, its counts and its
/// stated average, read without its sets; a session only a sheet dated has no
/// length; and a day's list holds that day's sessions only.
#[test]
fn a_session_is_listed_by_its_counts_and_averages() {
    use application::{Activities, history::Activity};

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
        let squat = CanonicalExercise::ForReps {
            identified: Attributed::new(
                Identified::Recorded(RepsExercise::BackSquatBarbell),
                sheet,
            ),
            sets: NonEmpty::of(merged_set(5, sheet, 80_000, watch).expect("a set"), vec![]),
        };
        let clocked = at("2020-10-09T07:12:00Z").expect("an instant");
        let dated = on("2020-10-10").expect("a day");
        store
            .replace(
                vec![
                    CanonicalGymSession::new(
                        Attributed::new(clocked.clone(), watch),
                        NonEmpty::of(
                            CanonicalItem::exercise(deadlift),
                            vec![CanonicalItem::exercise(squat.clone())],
                        ),
                        Some(heart_rate(watch).expect("a heart rate")),
                        Some(Attributed::new(
                            "2020-10-09T08:04:00Z".parse().expect("an instant"),
                            watch,
                        )),
                        Vec::new(),
                    ),
                    CanonicalGymSession::new(
                        domain::canonical::Attributed::edited(dated.clone()),
                        NonEmpty::of(CanonicalItem::exercise(squat), vec![]),
                        None,
                        None,
                        Vec::new(),
                    ),
                ],
                &Edits::default(),
            )
            .await
            .expect("a write");

        let friday = "2020-10-09".parse::<Date>().expect("a day");
        let saturday = "2020-10-10".parse::<Date>().expect("a day");
        assert_eq!(
            Activities::between(&store, friday, saturday)
                .await
                .expect("a read"),
            vec![
                (
                    clocked,
                    Activity::Gym {
                        length: Some(PositiveDuration::from_seconds(3_120).expect("a duration")),
                        exercises: 2,
                        sets: 3,
                        heart_rate: Some("77".parse().expect("a rate")),
                        tested: Vec::new(),
                    },
                ),
                (
                    dated.clone(),
                    Activity::Gym {
                        length: None,
                        exercises: 1,
                        sets: 1,
                        heart_rate: None,
                        tested: Vec::new(),
                    },
                ),
            ]
        );
        assert_eq!(
            Activities::between(&store, saturday, saturday)
                .await
                .expect("a read")
                .len(),
            1
        );
        assert_eq!(
            Activities::first(&store).await.expect("first"),
            Some(friday)
        );
    });
}

/// A set named by Hevy's workout and its place in it.
fn hevy(term: &str) -> Result<domain::gym::identity::SourceSet, Failure> {
    Ok(domain::gym::identity::SourceSet {
        stream: domain::landing::LandingStream::try_from("hevy.workouts")?,
        record: domain::landing::SourceRecordId::try_from("a-workout".to_owned())?,
        term: domain::normalised::SourceTerm::try_from(term.to_owned())?,
    })
}

/// Each canonical set's id and the exercise and session ids it sits under, by
/// its place in the layer.
async fn ids_by_place(pool: &SqlitePool) -> Result<Vec<(i64, i64, i64, i64)>, Failure> {
    let rows = sqlx::query(
        "SELECT s.position, s.id AS set_id, e.id AS exercise_id, s.session
         FROM canonical_gym_set AS s
         JOIN canonical_gym_exercise AS e
           ON e.session = s.session AND e.item_position = s.item_position
          AND e.position = s.exercise_position
         ORDER BY s.session, s.item_position, s.exercise_position, s.position",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<i64, _>("position"),
                row.get::<i64, _>("set_id"),
                row.get::<i64, _>("exercise_id"),
                row.get::<i64, _>("session"),
            )
        })
        .collect())
}

/// The one session the layer holds.
async fn only_session(
    pool: &SqlitePool,
) -> Result<domain::gym::identity::CanonicalSessionId, Failure> {
    let id: i64 = sqlx::query_scalar("SELECT id FROM canonical_gym_session")
        .fetch_one(pool)
        .await?;
    Ok(domain::gym::identity::CanonicalSessionId::try_from(id)?)
}

/// **A rebuilt set keeps its id where it was built from the same source sets**
/// (#893), whatever its position, and only a set nothing built before is new.
///
/// The operator, 2026-11-21: *"Once they've been built, they have been built,
/// they are a thing."* The layer is replaced whole each time here, which is
/// what a normalised writer's emptying of it comes to.
#[test]
fn a_rebuilt_set_keeps_its_id_by_its_source_sets() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = support::store::derived_and_authored()
            .await
            .expect("a derived store");
        let ids = normalised(&pool).await.expect("normalised sessions");
        let log = ids[0];
        let store = SqliteCanonicalGymSessionStore::new(pool.clone());

        let named = |reps: u32, term: &str| {
            let mut set = logged_set(reps, 100_000, log).expect("a set");
            set.sources.insert(hevy(term).expect("a source set"));
            set
        };
        let session = |sets: Vec<CanonicalSet<RepCount>>| {
            CanonicalGymSession::new(
                domain::canonical::Attributed::edited(on("2026-11-20").expect("a day")),
                NonEmpty::of(
                    CanonicalItem::exercise(CanonicalExercise::ForReps {
                        identified: Attributed::new(
                            Identified::Recorded(RepsExercise::BackSquatBarbell),
                            log,
                        ),
                        sets: NonEmpty::new(sets).expect("sets"),
                    }),
                    vec![],
                ),
                None,
                None,
                Vec::new(),
            )
        };

        store
            .replace(
                vec![session(vec![named(5, "0.0.0"), named(4, "0.0.1")])],
                &Edits::default(),
            )
            .await
            .expect("a first build");
        let first = ids_by_place(&pool).await.expect("ids");

        store
            .replace(
                vec![session(vec![named(5, "0.0.0"), named(4, "0.0.1")])],
                &Edits::default(),
            )
            .await
            .expect("the same build again");
        assert_eq!(ids_by_place(&pool).await.expect("ids"), first);

        // The two sets swap places and a third arrives: the two keep their
        // ids, and so do the exercise and session they are in.
        store
            .replace(
                vec![session(vec![
                    named(4, "0.0.1"),
                    named(5, "0.0.0"),
                    named(3, "0.0.2"),
                ])],
                &Edits::default(),
            )
            .await
            .expect("a rebuild");
        let rebuilt = ids_by_place(&pool).await.expect("ids");
        assert_eq!(rebuilt[0].1, first[1].1);
        assert_eq!(rebuilt[1].1, first[0].1);
        assert!(rebuilt[2].1 > first[1].1, "the new set is issued a new id");
        assert!(
            rebuilt
                .iter()
                .all(|row| row.2 == first[0].2 && row.3 == first[0].3)
        );
    });
}

/// **Rebuilding only the days that changed keeps ids too** (#893, #610): the
/// rebuilt day's set keeps its id, and the day left alone keeps its rows.
#[test]
fn rebuilding_one_day_keeps_its_ids_and_leaves_the_others() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = support::store::derived_and_authored()
            .await
            .expect("a derived store");
        let ids = normalised(&pool).await.expect("normalised sessions");
        let log = ids[0];
        let store = SqliteCanonicalGymSessionStore::new(pool.clone());

        let session = |day: &str, term: &str| {
            let mut set = logged_set(5, 100_000, log).expect("a set");
            set.sources.insert(hevy(term).expect("a source set"));
            CanonicalGymSession::new(
                domain::canonical::Attributed::edited(on(day).expect("a day")),
                NonEmpty::of(
                    CanonicalItem::exercise(CanonicalExercise::ForReps {
                        identified: Attributed::new(
                            Identified::Recorded(RepsExercise::BackSquatBarbell),
                            log,
                        ),
                        sets: NonEmpty::of(set, vec![]),
                    }),
                    vec![],
                ),
                None,
                None,
                Vec::new(),
            )
        };

        store
            .replace(
                vec![
                    session("2026-11-01", "1.0.0"),
                    session("2026-11-20", "20.0.0"),
                ],
                &Edits::default(),
            )
            .await
            .expect("a whole build");
        let first = ids_by_place(&pool).await.expect("ids");

        let day: Date = "2026-11-20".parse().expect("a day");
        let reach = domain::canonical::Reach::of(
            std::collections::BTreeSet::from([day]),
            domain::gym::REACHES,
        );
        store
            .replace_within(
                &reach,
                vec![session("2026-11-20", "20.0.0")],
                &Edits::default(),
            )
            .await
            .expect("one day rebuilt");

        assert_eq!(ids_by_place(&pool).await.expect("ids"), first);
    });
}

/// 2026-11-20's back squat: two sets Hevy logged at 100 kg.
fn two_squat_sets(log: NormalisedSessionId) -> Result<CanonicalGymSession, Failure> {
    let mut sets: Vec<CanonicalSet<RepCount>> = Vec::new();
    for term in ["0.0.0", "0.0.1"] {
        let mut set = logged_set(5, 100_000, log)?;
        set.sources.insert(hevy(term)?);
        sets.push(set);
    }
    let started = at("2026-11-20T18:00:00Z")?;
    let ended: jiff::Timestamp = "2026-11-20T19:00:00Z".parse()?;
    Ok(CanonicalGymSession::new(
        Attributed::new(started.clone(), log),
        NonEmpty::of(
            CanonicalItem::exercise(CanonicalExercise::ForReps {
                identified: Attributed::new(
                    Identified::Recorded(RepsExercise::BackSquatBarbell),
                    log,
                ),
                sets: NonEmpty::new(sets)?,
            }),
            vec![],
        ),
        None,
        Some(Attributed::new(ended, log)),
        vec![domain::gym::Account {
            stream: "hevy.workouts".parse()?,
            normalised: log,
            occurred: started,
            ended: Some(ended),
        }],
    ))
}

/// **When a session started and ended round-trips as an edit, and is
/// applied on a rebuild** (#894): a start the operator states and an end
/// taken from Hevy, and retracting it restores what Hevy said.
#[test]
fn a_session_s_start_and_end_are_edited() {
    use domain::gym::edit::{EditReason, SparseGymSession, Taken};
    use std::collections::BTreeMap;

    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = support::store::derived_and_authored()
            .await
            .expect("a derived store");
        let ids = normalised(&pool).await.expect("normalised sessions");
        let store = SqliteCanonicalGymSessionStore::new(pool.clone());
        let rebuild = async || {
            let edits = Edits::of(&store.edits().await.expect("the edits"));
            store
                .replace(vec![two_squat_sets(ids[0]).expect("a session")], &edits)
                .await
                .expect("a whole build");
        };
        rebuild().await;
        let session = only_session(&pool).await.expect("a session");

        let sparse = SparseGymSession::bounds(
            Some(Taken::Stated(
                at("2026-11-20T18:10:00Z").expect("an instant"),
            )),
            Some(Taken::From("hevy.workouts".parse().expect("a stream"))),
        );
        let reason = EditReason::try_from("pressed the watch late".to_owned()).expect("a reason");
        let id = store
            .assert(
                "2026-11-24T09:00:00Z".parse().expect("an instant"),
                &reason,
                &BTreeMap::from([(session, sparse.clone())]),
                &BTreeMap::new(),
            )
            .await
            .expect("an edit");
        assert_eq!(
            store.edits().await.expect("the edits")[0].sessions,
            BTreeMap::from([(session, sparse)])
        );

        rebuild().await;
        let read = store.all().await.expect("a read");
        assert_eq!(
            read[0].occurred(),
            &at("2026-11-20T18:10:00Z").expect("an instant")
        );
        assert!(read[0].started().is_edited());
        assert_eq!(
            read[0].ended().and_then(Attributed::normalised),
            Some(ids[0])
        );
        assert_eq!(
            read[0]
                .length()
                .map(domain::measure::PositiveDuration::as_seconds),
            Some(50 * 60)
        );

        assert!(store.retract(id).await.expect("a retraction"));
        rebuild().await;
        let read = store.all().await.expect("a read");
        assert_eq!(
            read[0].occurred(),
            &at("2026-11-20T18:00:00Z").expect("an instant")
        );
    });
}

/// Two edits of one squat: its second set rated at two in reserve, then the
/// exercise named a front squat. The first's id, to retract it by.
async fn rate_then_rename(
    store: &SqliteCanonicalGymSessionStore,
    session: domain::gym::identity::CanonicalSessionId,
    exercise: domain::gym::identity::CanonicalExerciseId,
    sets: &std::collections::BTreeSet<domain::gym::identity::CanonicalSetId>,
    second: domain::gym::identity::CanonicalSetId,
) -> Result<domain::gym::edit::EditId, Failure> {
    use domain::gym::{
        Rir,
        edit::{EditReason, Found, SparseExercise, SparseGymSession, SparseSet},
        exercise::Exercise,
    };
    use std::collections::BTreeMap;

    let reason = EditReason::try_from("forgot to rate it".to_owned())?;
    let rated = store
        .assert(
            "2026-11-22T09:00:00Z".parse()?,
            &reason,
            &BTreeMap::from([(
                session,
                SparseGymSession::exercise(
                    exercise,
                    SparseExercise::set(
                        second,
                        SparseSet::new(None, None, Some(Rir::Two)).ok_or("an effort")?,
                    ),
                ),
            )]),
            &BTreeMap::new(),
        )
        .await?;
    store
        .assert(
            "2026-11-22T09:01:00Z".parse()?,
            &reason,
            &BTreeMap::from([(
                session,
                SparseGymSession::exercise(
                    exercise,
                    SparseExercise::was(Exercise::Reps(RepsExercise::FrontSquatBarbell)),
                ),
            )]),
            &BTreeMap::from([(
                exercise,
                Found {
                    read: Exercise::Reps(RepsExercise::BackSquatBarbell),
                    sets: sets.clone(),
                },
            )]),
        )
        .await?;
    Ok(rated)
}

/// **An edit is applied on top of the layer, on the ids it keeps** (#893):
/// an effort given a set and an exercise renamed both stand after the layer
/// is rebuilt whole, on the same ids, and both say they came from the edit.
/// Retracting one rebuilds the set as its sources said it.
#[test]
fn an_edit_stands_on_the_same_ids_across_a_rebuild() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = support::store::derived_and_authored()
            .await
            .expect("a derived store");
        let ids = normalised(&pool).await.expect("normalised sessions");
        let log = ids[0];
        let store = SqliteCanonicalGymSessionStore::new(pool.clone());
        let session = || two_squat_sets(log).expect("a session");
        let rebuild = async || {
            let edits = Edits::of(&store.edits().await.expect("the edits"));
            store
                .replace(vec![session()], &edits)
                .await
                .expect("a whole build");
        };

        rebuild().await;
        let first = ids_by_place(&pool).await.expect("ids");
        let held = store.held().await.expect("the layer's ids");
        let (exercise, sets) = held.iter().next().expect("one exercise");
        let second = *sets.iter().nth(1).expect("a second set");
        let session = only_session(&pool).await.expect("a session");

        let rated = rate_then_rename(&store, session, *exercise, sets, second)
            .await
            .expect("two edits");

        rebuild().await;
        rebuild().await;
        assert_eq!(ids_by_place(&pool).await.expect("ids"), first);
        let read = store.all().await.expect("a read");
        let edited = read[0].exercises().next().expect("an exercise");
        assert_eq!(
            edited.exercise_key(),
            Some(RepsExercise::FrontSquatBarbell.as_str())
        );
        assert_eq!(edited.identified_by(), None, "the name is the edit's");
        let CanonicalExercise::ForReps {
            sets: read_sets, ..
        } = edited
        else {
            panic!("a reps exercise");
        };
        let efforts: Vec<_> = read_sets
            .iter()
            .map(|set| set.intensity.map(|rir| (rir.copied(), rir.is_edited())))
            .collect();
        assert_eq!(efforts, vec![None, Some((domain::gym::Rir::Two, true))]);
        assert!(
            store.held().await.expect("ids").len() == 1
                && Edits::of(&store.edits().await.expect("edits"))
                    .conflicts(
                        &store.held().await.expect("ids"),
                        &store.sessions_held().await.expect("sessions"),
                    )
                    .is_empty()
        );
        let first_edit = &store.edits().await.expect("edits")[0];
        assert_eq!(
            (first_edit.sets().count(), first_edit.exercises().count()),
            (1, 0),
            "the first holds one set and no exercise's name"
        );

        assert!(store.retract(rated).await.expect("a retraction"));
        rebuild().await;
        let read = store.all().await.expect("a read");
        let CanonicalExercise::ForReps {
            sets: read_sets, ..
        } = read[0].exercises().next().expect("an exercise")
        else {
            panic!("a reps exercise");
        };
        assert!(read_sets.iter().all(|set| set.intensity.is_none()));
    });
}

/// **A set edit's measure is read back as it was stated** (#901): a length,
/// a distance and a count each round-trip, with the load beside them.
#[test]
fn a_set_edit_s_measure_round_trips() {
    use domain::gym::{
        Load,
        edit::{EditReason, Measure, SparseExercise, SparseGymSession, SparseSet},
    };
    use domain::measure::{Distance, Duration, Kg, Metres, RepCount};
    use std::collections::BTreeMap;

    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = support::store::derived_and_authored()
            .await
            .expect("a derived store");
        let ids = normalised(&pool).await.expect("normalised sessions");
        let store = SqliteCanonicalGymSessionStore::new(pool.clone());
        store
            .replace(
                vec![two_squat_sets(ids[0]).expect("a session")],
                &Edits::default(),
            )
            .await
            .expect("a whole build");
        let held = store.held().await.expect("the layer's ids");
        let (exercise, set) = held
            .iter()
            .next()
            .and_then(|(exercise, sets)| Some((*exercise, *sets.iter().next()?)))
            .expect("a set");
        let session = only_session(&pool).await.expect("a session");
        let reason = EditReason::try_from("timed it wrong".to_owned()).expect("a reason");
        let stated = [
            Measure::Duration(Duration::from_seconds(45)),
            Measure::Distance(Distance {
                metres: Metres::from_millimetres(20_500),
            }),
            Measure::Reps(RepCount::new(6).expect("a count")),
        ];
        for (minute, measure) in stated.iter().enumerate() {
            let fields = SparseSet::new(
                Some(*measure),
                Some(Load::absolute(Kg::from_grams(10_000))),
                None,
            )
            .expect("an edit");
            store
                .assert(
                    format!("2026-11-23T09:0{minute}:00Z")
                        .parse()
                        .expect("an instant"),
                    &reason,
                    &BTreeMap::from([(
                        session,
                        SparseGymSession::exercise(exercise, SparseExercise::set(set, fields)),
                    )]),
                    &BTreeMap::new(),
                )
                .await
                .expect("an edit");
        }

        let read: Vec<Option<Measure>> = store
            .edits()
            .await
            .expect("the edits")
            .iter()
            .map(|edit| edit.sets().next().and_then(|(_, fields)| fields.measure()))
            .collect();
        assert_eq!(read, stated.map(Some));
    });
}
