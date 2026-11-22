//! A lift's performances as a chart reads them (#817): its sets as the one
//! thing each is, and none that does not say what it is.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use crate::support;
use domain::gym::edit::Edits;

use application::{CanonicalGymSessionStore as _, Performances as _};
use domain::{
    canonical::{Attributed, NormalisedSessionId, Occurred},
    gym::{
        CanonicalExercise, CanonicalGymSession, CanonicalItem, CanonicalSet, Identified, Load,
        Performed, PerformedSet, Rir, SetKind, exercise::RepsExercise,
    },
    measure::{Kg, RepCount},
    sequence::NonEmpty,
};
use infrastructure::{SqliteCanonicalGymSessionStore, SqlitePerformances};
use jiff::civil::{Date, date};
use sqlx::{Row as _, SqlitePool};

type Failure = Box<dyn std::error::Error>;

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

async fn first_normalised(pool: &SqlitePool) -> Result<NormalisedSessionId, Failure> {
    let row = sqlx::query("SELECT id FROM gym_session ORDER BY id LIMIT 1")
        .fetch_one(pool)
        .await?;
    Ok(NormalisedSessionId::try_from(row.get::<i64, _>("id"))?)
}

/// A set at `kg`, completed for `reps` or failed, with the kind and effort the
/// record states, if any.
fn set(
    kg: u64,
    reps: Option<u32>,
    kind: Option<SetKind>,
    rir: Option<Rir>,
    by: NormalisedSessionId,
) -> Result<CanonicalSet<RepCount>, Failure> {
    Ok(CanonicalSet {
        outcome: Attributed::new(
            match reps {
                Some(reps) => Performed::Completed(Some(RepCount::new(reps)?)),
                None => Performed::Failed,
            },
            by,
        ),
        load: Some(Attributed::new(
            Load::absolute(Kg::from_grams(kg * 1_000)),
            by,
        )),
        began: None,
        intensity: rir.map(|rir| Attributed::new(rir, by)),
        kind: kind.map(|kind| Attributed::new(kind, by)),
        rest_after: None,
        sources: std::collections::BTreeSet::new(),
    })
}

fn session(
    on: Date,
    lift: RepsExercise,
    sets: Vec<CanonicalSet<RepCount>>,
    by: NormalisedSessionId,
) -> Result<CanonicalGymSession, Failure> {
    Ok(CanonicalGymSession::new(
        Occurred::On(on),
        NonEmpty::of(
            CanonicalItem::exercise(CanonicalExercise::ForReps {
                identified: Attributed::new(Identified::Recorded(lift), by),
                sets: NonEmpty::new(sets)?,
            }),
            vec![],
        ),
        None,
        None,
    ))
}

const fn kg(kg: u64) -> Load {
    Load::absolute(Kg::from_grams(kg * 1_000))
}

fn reps(count: u32) -> Result<RepCount, Failure> {
    Ok(RepCount::new(count)?)
}

/// 2026-09-25's shape, a set that names no kind, and a lift and a day nobody
/// asked about.
#[test]
fn each_set_is_the_one_thing_it_is_and_one_without_a_kind_is_not_read() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = support::store::derived_and_authored()
            .await
            .expect("a derived store");
        let by = first_normalised(&pool).await.expect("a normalised session");
        let front_squat = RepsExercise::FrontSquatBarbell;
        let sessions = vec![
            session(
                date(2026, 9, 25),
                front_squat,
                vec![
                    set(35, Some(4), Some(SetKind::Warmup), None, by).expect("a set"),
                    set(80, Some(3), Some(SetKind::Working), Some(Rir::Two), by).expect("a set"),
                    set(90, Some(1), Some(SetKind::Working), None, by).expect("a set"),
                    set(95, None, Some(SetKind::Working), None, by).expect("a set"),
                    // 2018's spreadsheets name no kind.
                    set(60, Some(5), None, None, by).expect("a set"),
                ],
                by,
            )
            .expect("a session"),
            session(
                date(2026, 9, 25),
                RepsExercise::BenchPressBarbell,
                vec![set(60, Some(5), Some(SetKind::Working), Some(Rir::Two), by).expect("a set")],
                by,
            )
            .expect("a session"),
            session(
                date(2026, 9, 1),
                front_squat,
                vec![
                    set(70, Some(5), Some(SetKind::Working), Some(Rir::Three), by).expect("a set"),
                ],
                by,
            )
            .expect("a session"),
        ];
        SqliteCanonicalGymSessionStore::new(pool.clone())
            .replace(sessions, &Edits::default())
            .await
            .expect("a write");

        let read = SqlitePerformances::new(pool)
            .of(&[front_squat], date(2026, 9, 20), date(2026, 9, 30))
            .await
            .expect("a read");

        assert_eq!(read.len(), 1, "one lift, one day: {read:?}");
        let sets: Vec<PerformedSet> = read[0].sets.iter().copied().collect();
        assert_eq!(
            sets,
            [
                PerformedSet::Warmup {
                    load: kg(35),
                    reps: reps(4).expect("a count")
                },
                PerformedSet::Working {
                    load: kg(80),
                    reps: reps(3).expect("a count"),
                    rir: Rir::Two
                },
                PerformedSet::Unrated {
                    load: kg(90),
                    reps: reps(1).expect("a count")
                },
                PerformedSet::Failed { load: kg(95) },
            ]
        );
    });
}
