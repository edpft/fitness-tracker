//! The canonical layer for gym sessions (#284): one row per visit, naming the
//! normalised sessions its parts come from.
//!
//! What is pinned here is the table and the round trip, not the matching —
//! nothing builds these yet, and #247 is the algorithm that will. So the
//! sessions are assembled by hand, in the shapes the operator's record holds:
//! a visit two sources described, a visit only a sheet dated, and a visit
//! whose heart rate and exercises came from one watch.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

mod support;

use application::CanonicalGymSessionStore as _;
use domain::{
    canonical::{NormalisedSessionId, Occurred, SessionCount},
    gym::{CanonicalGymSession, Part},
    normalised::{OperatorZone, StartedAt},
    sequence::NonEmpty,
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

/// The normalised sessions the corpus derived, oldest first.
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

/// Written, read back, and identical — including the order the parts were
/// matched in, which the store keeps by position rather than by insertion.
#[test]
fn a_canonical_session_round_trips() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = support::store::derived_and_authored()
            .await
            .expect("a derived store");
        let ids = normalised(&pool).await.expect("normalised sessions");
        let (first, second) = (ids[0], ids[1]);

        let store = SqliteCanonicalGymSessionStore::new(pool.clone());

        // A visit two sources described: one holds the exercises, the other
        // the heart rate.
        let joined = CanonicalGymSession::new(
            at("2025-02-19T12:30:30Z").expect("an instant"),
            NonEmpty::of(Part::Exercises(first), vec![Part::HeartRate(second)]),
        );
        // A visit only a sheet dated, which is every spreadsheet session.
        let dated = CanonicalGymSession::new(
            on("2019-03-14").expect("a day"),
            NonEmpty::of(Part::Exercises(ids[2]), vec![]),
        );

        let written = store
            .replace(vec![dated.clone(), joined.clone()])
            .await
            .expect("a write");
        assert_eq!(written, SessionCount::from(2));
        assert_eq!(store.count().await.expect("a count"), SessionCount::from(2));

        // Oldest first, so the 2019 sheet leads although it was written first
        // by accident rather than by order.
        let read = store.all().await.expect("a read");
        assert_eq!(read, vec![dated, joined]);
    });
}

/// One watch supplies both parts of a session, and says so twice.
///
/// 2019-03-14 is the case: Garmin holds the only heart rate and, before Hevy
/// existed, an account of the exercises too.
#[test]
fn one_normalised_session_may_supply_both_parts() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = support::store::derived_and_authored()
            .await
            .expect("a derived store");
        let ids = normalised(&pool).await.expect("normalised sessions");
        let watch = ids[0];

        let store = SqliteCanonicalGymSessionStore::new(pool.clone());
        let session = CanonicalGymSession::new(
            at("2019-03-14T07:43:22Z").expect("an instant"),
            NonEmpty::of(Part::Exercises(watch), vec![Part::HeartRate(watch)]),
        );

        store.replace(vec![session.clone()]).await.expect("a write");

        let read = store.all().await.expect("a read");
        assert_eq!(read, vec![session]);
        assert_eq!(read[0].stands_on(), vec![watch]);
    });
}

/// A derivation is replaced, never added to (§ II).
#[test]
fn a_second_join_replaces_the_first() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = support::store::derived_and_authored()
            .await
            .expect("a derived store");
        let ids = normalised(&pool).await.expect("normalised sessions");

        let store = SqliteCanonicalGymSessionStore::new(pool.clone());
        let first = CanonicalGymSession::new(
            on("2019-03-14").expect("a day"),
            NonEmpty::of(Part::Exercises(ids[0]), vec![]),
        );
        store.replace(vec![first]).await.expect("a first write");

        let second = CanonicalGymSession::new(
            on("2020-10-12").expect("a day"),
            NonEmpty::of(Part::Exercises(ids[1]), vec![]),
        );
        store
            .replace(vec![second.clone()])
            .await
            .expect("a second write");

        assert_eq!(store.count().await.expect("a count"), SessionCount::from(1));
        assert_eq!(store.all().await.expect("a read"), vec![second]);
    });
}

/// A part naming a normalised session the store does not hold is refused by
/// the foreign key, and nothing of the write survives.
#[test]
fn a_part_cannot_name_a_session_that_is_not_there() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = support::store::derived_and_authored()
            .await
            .expect("a derived store");
        let ids = normalised(&pool).await.expect("normalised sessions");

        let store = SqliteCanonicalGymSessionStore::new(pool.clone());
        let kept = CanonicalGymSession::new(
            on("2019-03-14").expect("a day"),
            NonEmpty::of(Part::Exercises(ids[0]), vec![]),
        );
        store.replace(vec![kept.clone()]).await.expect("a write");

        let absent = NormalisedSessionId::try_from(i64::MAX).expect("an id");
        let bad = CanonicalGymSession::new(
            on("2020-10-12").expect("a day"),
            NonEmpty::of(Part::Exercises(absent), vec![]),
        );
        let outcome = store.replace(vec![bad]).await;
        assert!(outcome.is_err(), "a part named a session that is not there");

        // The transaction rolled back, so the earlier join is untouched.
        assert_eq!(store.all().await.expect("a read"), vec![kept]);
    });
}

/// One source's account belongs to one event (§ 10): the same normalised
/// session cannot supply the same part to two canonical sessions.
#[test]
fn a_normalised_session_supplies_one_canonical_session() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (_directory, pool) = support::store::derived_and_authored()
            .await
            .expect("a derived store");
        let ids = normalised(&pool).await.expect("normalised sessions");

        let store = SqliteCanonicalGymSessionStore::new(pool.clone());
        let outcome = store
            .replace(vec![
                CanonicalGymSession::new(
                    on("2019-03-14").expect("a day"),
                    NonEmpty::of(Part::Exercises(ids[0]), vec![]),
                ),
                CanonicalGymSession::new(
                    on("2019-03-17").expect("a day"),
                    NonEmpty::of(Part::Exercises(ids[0]), vec![]),
                ),
            ])
            .await;

        assert!(outcome.is_err(), "one account, claimed by two events");
        assert_eq!(store.count().await.expect("a count"), SessionCount::from(0));
    });
}
