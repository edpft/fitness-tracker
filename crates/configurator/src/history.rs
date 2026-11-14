//! The sessions he has done, and one of them opened, over this build's store
//! (#772).

use application::{
    StoreError,
    history::{Day, History, Number, Session},
    next::Scope,
};
use domain::normalised::OperatorZone;
use infrastructure::{
    SqliteCanonicalCyclingSessionStore, SqliteCanonicalGymSessionStore, SqlitePadelGames,
};
use jiff::civil::Date;

use crate::{graph::page, one_rep_max, store::Store};

page! {
    /// What History reads: the canonical gym and cycling sessions, the padel
    /// games Garmin's activities give, and which sessions were 1RM tests.
    pub struct Page {
        history: History<
            SqliteCanonicalGymSessionStore,
            SqliteCanonicalCyclingSessionStore,
            SqlitePadelGames,
        >,
        tests: one_rep_max::Tests,
    }
}

/// The parts a run collecting what the page reads takes.
#[must_use]
pub fn collects() -> Scope {
    crate::graph::collects::<Page>()
}

impl Page {
    /// The page over `store`.
    #[must_use]
    pub fn open(store: &Store, zone: &OperatorZone) -> Self {
        let pool = store.pool();
        Self {
            history: History::new(
                SqliteCanonicalGymSessionStore::new(pool.clone()),
                SqliteCanonicalCyclingSessionStore::new(pool.clone()),
                SqlitePadelGames::new(pool.clone()),
            ),
            tests: one_rep_max::tests(store, zone),
        }
    }

    /// Every day through today that holds a session, newest first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn days(&self, zone: &OperatorZone) -> Result<Vec<Day>, StoreError> {
        let today = crate::equipment::today(zone);
        let tests = self.tests.tests(today).await?;
        self.history.days(today, &tests).await
    }

    /// The session `number` of those done `on`, where there is one.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn session(
        &self,
        zone: &OperatorZone,
        on: Date,
        number: Number,
    ) -> Result<Option<Session>, StoreError> {
        let tests = self.tests.tests(crate::equipment::today(zone)).await?;
        self.history.session(on, number, &tests).await
    }
}
