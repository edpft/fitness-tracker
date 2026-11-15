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

use crate::clock::Today;
use crate::store::Spanned;
use crate::{
    graph::{page, read},
    one_rep_max,
    store::Store,
};

page! {
    /// What History reads: the canonical gym and cycling sessions, the padel
    /// games Garmin's activities give, and which sessions were 1RM tests.
    pub struct Page {
        history: History<
            Spanned<SqliteCanonicalGymSessionStore>,
            Spanned<SqliteCanonicalCyclingSessionStore>,
            Spanned<SqlitePadelGames>,
        >,
        tests: one_rep_max::Tests,
        today: Today,
    }
}

/// The parts a run collecting what the page reads takes.
#[must_use]
pub fn collects() -> Scope {
    crate::graph::scope(&reads())
}

/// What the page reads, and so what its kept drawing is drawn from (#805).
#[must_use]
pub fn reads() -> application::graph::Reads {
    <Page as application::graph::Input>::reads()
}

impl Page {
    /// The page over `store`.
    #[must_use]
    pub fn open(store: &Store, zone: &OperatorZone) -> Self {
        Self {
            history: History::new(
                store.adapter(SqliteCanonicalGymSessionStore::new),
                store.adapter(SqliteCanonicalCyclingSessionStore::new),
                store.adapter(SqlitePadelGames::new),
            )
            .into(),
            tests: one_rep_max::tests(store, zone).into(),
            today: Today::new(zone.clone()).into(),
        }
    }

    /// The date it is now.
    #[must_use]
    pub fn today(&self) -> Date {
        self.today.date()
    }

    /// Every day through today that holds a session, newest first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn days(&self) -> Result<Vec<Day>, StoreError> {
        let today = self.today.date();
        let tests = read!(self.tests.tests(today)).await?;
        read!(self.history.days(today, &tests)).await
    }

    /// The session `number` of those done `on`, where there is one.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn session(&self, on: Date, number: Number) -> Result<Option<Session>, StoreError> {
        let tests = read!(self.tests.tests(self.today.date())).await?;
        read!(self.history.session(on, number, &tests)).await
    }
}
