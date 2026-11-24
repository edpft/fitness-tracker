//! What in the gym record may need correcting, over this build's store, and
//! the operator saying one is right as recorded (#896).

use application::{
    Input, Node, NotRaw, Reads, StoreError,
    history::{History, Number},
};
use domain::{
    canonical::Occurred,
    gym::{
        check::{self, Doubt, Kind},
        exercise::RepsExercise,
        identity::CanonicalExerciseId,
    },
    normalised::OperatorZone,
};
use infrastructure::{
    SqliteCanonicalCyclingSessionStore, SqliteCanonicalGymSessionStore, SqlitePadelSessions,
};
use jiff::{Timestamp, civil::Date};

use crate::{
    graph::{page, read},
    one_rep_max,
    store::{Spanned, Store},
};

/// One exercise that may need correcting, and where it is changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToCheck {
    pub doubt: Doubt,
    pub lift: RepsExercise,
    /// What the operator says is right.
    pub exercise: CanonicalExerciseId,
    /// The session, as History opens it.
    pub on: Date,
    pub number: Number,
    /// Which of its exercises, from one: what its pen is addressed by.
    pub position: usize,
}

/// The canonical gym layer, what the operator said is right, and the list
/// that numbers the sessions.
pub struct Checks {
    store: Store,
    numbered: History<
        Spanned<SqliteCanonicalGymSessionStore>,
        Spanned<SqliteCanonicalCyclingSessionStore>,
        Spanned<SqlitePadelSessions>,
        one_rep_max::Tests,
    >,
}

/// **What he said is right is kept beside the edits**, so saying it moves
/// what the page is drawn from.
impl Input for Checks {
    fn reads() -> Reads {
        Reads::of([Node::Edits(application::Edited::Gym)])
            .and(<SqliteCanonicalGymSessionStore as Input>::reads())
            .and(<SqliteCanonicalCyclingSessionStore as Input>::reads())
            .and(<SqlitePadelSessions as Input>::reads())
    }
}

impl NotRaw for Checks {}

impl Checks {
    #[must_use]
    pub fn open(store: &Store, zone: &OperatorZone) -> Self {
        Self {
            store: store.clone(),
            numbered: History::new(
                store.adapter(SqliteCanonicalGymSessionStore::new),
                store.adapter(SqliteCanonicalCyclingSessionStore::new),
                store.adapter(SqlitePadelSessions::new),
                one_rep_max::tests(store, zone),
            ),
        }
    }

    /// Everything to check, newest first, leaving out what the operator said
    /// is right and what the layer is being rebuilt under.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn to_check(&self) -> Result<Vec<ToCheck>, StoreError> {
        let adapter = self.store.adapter(SqliteCanonicalGymSessionStore::new);
        let sessions = adapter
            .call(
                "SqliteCanonicalGymSessionStore::all",
                |adapter| async move { adapter.reading().await?.all().await },
            )
            .await?;
        let found = check::find(&sessions);
        let days = found
            .iter()
            .filter_map(|found| sessions.get(found.session))
            .map(|session| session.occurred().day());
        let (Some(from), Some(to)) = (days.clone().min(), days.max()) else {
            return Ok(Vec::new());
        };
        let numbered = self.numbered.gym_numbered(from, to).await?;
        let right = adapter
            .call("SqliteCanonicalGymSessionStore::said_right", |adapter| {
                adapter.said_right()
            })
            .await?;
        let mut checks = Vec::new();
        for found in found.into_iter().rev() {
            let Some(session) = sessions.get(found.session) else {
                continue;
            };
            let Some((on, number)) = number_of(&sessions, found.session, &numbered) else {
                continue;
            };
            let Some(exercise) = found
                .position
                .checked_sub(1)
                .and_then(|index| session.exercises().nth(index))
            else {
                continue;
            };
            let Some((_, ids)) = crate::edits::ids_of(&self.store, exercise).await? else {
                continue;
            };
            if right.contains(&(ids.exercise, found.doubt.kind())) {
                continue;
            }
            checks.push(ToCheck {
                doubt: found.doubt,
                lift: found.exercise,
                exercise: ids.exercise,
                on,
                number,
                position: found.position,
            });
        }
        Ok(checks)
    }
}

/// The day and number of `sessions[index]` in the list: the list's gym
/// session that occurred when it did, counting those that occurred at the same
/// moment in the order both read them.
fn number_of(
    sessions: &[domain::gym::CanonicalGymSession],
    index: usize,
    numbered: &[(Occurred, Date, Number)],
) -> Option<(Date, Number)> {
    let occurred = sessions.get(index)?.occurred();
    let nth = sessions
        .iter()
        .take(index)
        .filter(|session| session.occurred() == occurred)
        .count();
    numbered
        .iter()
        .filter(|(listed, _, _)| listed == occurred)
        .nth(nth)
        .map(|(_, on, number)| (*on, *number))
}

page! {
    /// What the list of what may need correcting reads (#896).
    pub struct Checking {
        checks: Checks,
    }
}

impl Checking {
    #[must_use]
    pub fn open(store: &Store, zone: &OperatorZone) -> Self {
        Self {
            checks: Checks::open(store, zone).into(),
        }
    }

    /// Everything to check, newest first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn to_check(&self) -> Result<Vec<ToCheck>, StoreError> {
        read!(self.checks.to_check()).await
    }
}

/// What the list reads, and so what its kept drawing is drawn from.
#[must_use]
pub fn reads() -> Reads {
    <Checking as Input>::reads()
}

/// The operator says `exercise` is right as recorded, whatever `kind` found.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be written.
pub async fn say_right(
    store: &Store,
    exercise: CanonicalExerciseId,
    kind: Kind,
) -> Result<(), StoreError> {
    store
        .adapter(SqliteCanonicalGymSessionStore::new)
        .call("SqliteCanonicalGymSessionStore::say_right", |adapter| {
            adapter.say_right(Timestamp::now(), exercise, kind)
        })
        .await
}

/// Each kind, in the order listed, and how many of `checks` are of it.
#[must_use]
pub fn by_kind(checks: &[ToCheck]) -> Vec<(Kind, Vec<&ToCheck>)> {
    Kind::ALL
        .into_iter()
        .map(|kind| {
            (
                kind,
                checks
                    .iter()
                    .filter(|check| check.doubt.kind() == kind)
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, checks)| !checks.is_empty())
        .collect()
}
