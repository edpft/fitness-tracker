//! The next session, found across a plan rather than inside one mesocycle
//! (#122).
//!
//! The store holds the autumn's shape: a one-week test whose Monday is
//! skipped, then three four-week mesocycles back to back, running Mondays and
//! Fridays.
//!
//! ```text
//! 2026-09-14 to 2026-09-20   test week, Monday skipped: Friday 18 only
//! 2026-09-21 to 2026-10-18   four weeks
//! 2026-10-19 to 2026-11-15   four weeks
//! 2026-11-16 to 2026-12-13   four weeks, last session Friday 11 December
//! ```
//!
//! At the adapter's ring because the search crosses mesocycles through the
//! store's `following`, and the store is what is under test as much as the use
//! case.

mod support;

use application::{
    PlanStore as _,
    prescribe::{NextSession, next_session},
};
use domain::{
    measure::RepCount,
    prescription::{Mesocycle, Skip, authored::Shape},
    schedule::{Relative, SessionRole},
};
use infrastructure::{SqliteGymMesocycleStore, SqlitePlanStore, connect};
use jiff::civil::Date;
use support::{corpus, programme};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// A linear mesocycle over the fixture's Mondays and Fridays.
///
/// The template does not matter here: every mesocycle's calendar is what the
/// search asks, and linear is the one whose span is simply its weeks.
fn mesocycle(start: Date, weeks: u32) -> Fallible<Mesocycle> {
    let answers = programme::authored(
        start,
        Shape::Linear {
            gating: SessionRole::new(Relative::Higher, Relative::Lower),
            weeks,
        },
    )?;
    Ok(programme::authoring(answers, &[])??)
}

/// A one-week test over the same days, skipping some of them.
///
/// A test rather than a one-week linear block, which is refused — and a test
/// week is what opens the autumn.
fn test_week(start: Date, skipping: &[Skip]) -> Fallible<Mesocycle> {
    let answers = programme::authored(
        start,
        Shape::Test {
            reps: RepCount::new(1)?,
            asserted: None,
            provided: None,
        },
    )?;
    Ok(programme::authoring(answers, skipping)??)
}

/// A store with nothing authored, and the directory holding it.
async fn empty() -> Fallible<(SqliteGymMesocycleStore, tempfile::TempDir)> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;
    // A block's calendar is rebuilt from the operator's week on every read
    // (issue #63), so a store with no week in it cannot hold a plan.
    programme::record_the_week(&pool).await?;
    Ok((
        SqliteGymMesocycleStore::new(pool, corpus::zone()?),
        directory,
    ))
}

/// A store holding the autumn's shape, and the directory holding it.
async fn autumn() -> Fallible<(SqliteGymMesocycleStore, tempfile::TempDir)> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;
    // A block's calendar is rebuilt from the operator's week on every read
    // (issue #63), so a store with no week in it cannot hold a plan.
    programme::record_the_week(&pool).await?;

    let opening = Date::constant(2026, 9, 14);
    let plan = programme::named_plan(
        "autumn",
        vec![
            test_week(opening, &[Skip::day(opening)])?,
            mesocycle(Date::constant(2026, 9, 21), 4)?,
            mesocycle(Date::constant(2026, 10, 19), 4)?,
            mesocycle(Date::constant(2026, 11, 16), 4)?,
        ],
    )?;
    SqlitePlanStore::new(pool.clone(), corpus::zone()?)
        .author(&plan)
        .await?;

    Ok((
        SqliteGymMesocycleStore::new(pool, corpus::zone()?),
        directory,
    ))
}

async fn next_in(
    store: impl Future<Output = Fallible<(SqliteGymMesocycleStore, tempfile::TempDir)>>,
    from: Date,
) -> Fallible<NextSession> {
    // The directory is held until the search is done: dropping it removes the
    // database underneath the pool.
    let (store, _directory) = store.await?;
    Ok(next_session(&store, from).await?)
}

macro_rules! next_from {
    ($store:expr, $date:expr) => {
        match corpus::block_on(next_in($store, $date)) {
            Ok(Ok(next)) => next,
            Ok(Err(error)) => panic!("the store authors and answers: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

/// The case that made this blocking: the test week's one session is on the
/// Friday, and asked on the Saturday the answer is the Monday the next
/// mesocycle opens — not "the programme has no session left".
#[test]
fn after_the_test_weeks_session_it_is_the_next_mesocycles_first() {
    assert_eq!(
        next_from!(autumn(), Date::constant(2026, 9, 19)),
        NextSession::On(Date::constant(2026, 9, 21)),
    );
}

/// A skipped day is stepped over, not refused.
#[test]
fn a_skipped_day_answers_with_the_session_after_it() {
    assert_eq!(
        next_from!(autumn(), Date::constant(2026, 9, 14)),
        NextSession::On(Date::constant(2026, 9, 18)),
    );
}

/// No mesocycle need be in force on the date: before the plan opens, the
/// answer is its first session.
#[test]
fn before_the_plan_it_is_the_first_session() {
    assert_eq!(
        next_from!(autumn(), Date::constant(2026, 9, 1)),
        NextSession::On(Date::constant(2026, 9, 18)),
    );
}

/// Every boundary crosses, not only the test week's.
#[test]
fn the_weekend_before_a_mesocycle_is_its_monday() {
    assert_eq!(
        next_from!(autumn(), Date::constant(2026, 10, 17)),
        NextSession::On(Date::constant(2026, 10, 19)),
    );
}

/// Today counts: asked on a training day, the answer is that day.
#[test]
fn a_training_day_answers_with_itself() {
    assert_eq!(
        next_from!(autumn(), Date::constant(2026, 9, 18)),
        NextSession::On(Date::constant(2026, 9, 18)),
    );
}

/// Past the end is an answer, not an error — and it says when the plan ended.
#[test]
fn past_the_end_nothing_is_planned() {
    assert_eq!(
        next_from!(autumn(), Date::constant(2026, 12, 14)),
        NextSession::NothingPlanned {
            last: Some((
                programme::name("autumn").expect("a usable plan name"),
                Date::constant(2026, 12, 13),
            )),
        },
    );
}

/// After the last session but inside the last mesocycle, the plan still ends
/// on the mesocycle's last day rather than on the one before it.
#[test]
fn after_the_last_session_the_plan_ends_on_its_last_day() {
    assert_eq!(
        next_from!(autumn(), Date::constant(2026, 12, 12)),
        NextSession::NothingPlanned {
            last: Some((
                programme::name("autumn").expect("a usable plan name"),
                Date::constant(2026, 12, 13),
            )),
        },
    );
}

/// A store with nothing authored has nothing planned, and no plan to name.
#[test]
fn an_empty_store_has_nothing_planned() {
    assert_eq!(
        next_from!(empty(), Date::constant(2026, 9, 15)),
        NextSession::NothingPlanned { last: None },
    );
}
