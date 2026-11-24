//! The activities he has done, a page at a time, and one of them opened, over
//! this build's store (#772, #817).

use application::{
    StoreError,
    history::{
        History, Number, Page,
        opened::{self, Activity},
    },
    next::Scope,
};
use domain::normalised::OperatorZone;
use infrastructure::{
    SqliteCanonicalCyclingSessionStore, SqliteCanonicalGymSessionStore, SqlitePadelSessions,
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
    /// What the list reads: each activity as it lists it, and which gym
    /// sessions were 1RM tests.
    pub struct List {
        history: History<
            Spanned<SqliteCanonicalGymSessionStore>,
            Spanned<SqliteCanonicalCyclingSessionStore>,
            Spanned<SqlitePadelSessions>,
            one_rep_max::Tests,
        >,
        today: Today,
    }
}

page! {
    /// What an opened activity reads: the whole of it.
    pub struct Opened {
        opened: opened::Opened<
            Spanned<SqliteCanonicalGymSessionStore>,
            Spanned<SqliteCanonicalCyclingSessionStore>,
            Spanned<SqlitePadelSessions>,
            one_rep_max::Tests,
        >,
    }
}

page! {
    /// What the list of edits to settle reads: the edits, and the layer
    /// they are applied to (#905).
    pub struct Settling {
        overlay: crate::edits::Overlay,
    }
}

impl Settling {
    #[must_use]
    pub fn open(store: &Store, zone: &OperatorZone) -> Self {
        Self {
            overlay: crate::edits::Overlay::open(store, zone).into(),
        }
    }

    /// Each edit a rebuild left for the operator to settle.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn to_settle(&self) -> Result<Vec<crate::edits::ToSettle>, StoreError> {
        read!(self.overlay.to_settle()).await
    }
}

page! {
    /// What an opened session's list of its changes reads: the edits, and the
    /// layer they are applied to (#895).
    pub struct Changes {
        overlay: crate::edits::Overlay,
    }
}

impl Changes {
    #[must_use]
    pub fn open(store: &Store, zone: &OperatorZone) -> Self {
        Self {
            overlay: crate::edits::Overlay::open(store, zone).into(),
        }
    }

    /// The edits in force on `session`, newest first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn of(
        &self,
        session: &domain::gym::CanonicalGymSession,
    ) -> Result<Vec<crate::edits::InForce>, StoreError> {
        read!(self.overlay.in_force(session)).await
    }
}

/// The parts a run collecting what the list reads takes.
#[must_use]
pub fn collects() -> Scope {
    crate::graph::scope(&reads())
}

/// What the list reads, and so what its kept drawing is drawn from (#805).
#[must_use]
pub fn reads() -> application::graph::Reads {
    <List as application::graph::Input>::reads()
        .and(<Settling as application::graph::Input>::reads())
}

/// What an opened activity reads.
#[must_use]
pub fn opened_reads() -> application::graph::Reads {
    <Opened as application::graph::Input>::reads()
}

/// What an opened activity's page reads: it, and the changes in force on it.
#[must_use]
pub fn opened_page_reads() -> application::graph::Reads {
    opened_reads().and(<Changes as application::graph::Input>::reads())
}

impl List {
    /// The list over `store`.
    #[must_use]
    pub fn open(store: &Store, zone: &OperatorZone) -> Self {
        Self {
            history: History::new(
                store.adapter(SqliteCanonicalGymSessionStore::new),
                store.adapter(SqliteCanonicalCyclingSessionStore::new),
                store.adapter(SqlitePadelSessions::new),
                one_rep_max::tests(store, zone),
            )
            .into(),
            today: Today::new(zone.clone()).into(),
        }
    }

    /// The date it is now.
    #[must_use]
    pub fn today(&self) -> Date {
        self.today.date()
    }

    /// The page of days before `before`, or through today where it is `None`.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn page(&self, before: Option<Date>) -> Result<Page, StoreError> {
        let before = match before {
            Some(before) => before,
            None => self
                .today
                .date()
                .tomorrow()
                .map_err(|_| StoreError::Corrupt {
                    detail: "a day beyond the calendar".to_owned(),
                })?,
        };
        read!(self.history.page(before)).await
    }
}

impl Opened {
    /// The opened activity over `store`.
    #[must_use]
    pub fn open(store: &Store, zone: &OperatorZone) -> Self {
        Self {
            opened: opened::Opened::new(
                store.adapter(SqliteCanonicalGymSessionStore::new),
                store.adapter(SqliteCanonicalCyclingSessionStore::new),
                store.adapter(SqlitePadelSessions::new),
                one_rep_max::tests(store, zone),
            )
            .into(),
        }
    }

    /// The activity `number` of those done `on`, where there is one.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn activity(&self, on: Date, number: Number) -> Result<Option<Activity>, StoreError> {
        read!(self.opened.activity(on, number)).await
    }
}
