//! The FTP tests, in watts and over body weight, over this build's store
//! (#469).

use application::{
    StoreError,
    ftp_tests::{FtpTests, TestHeartRates},
    next::Scope,
};
use domain::{
    analytical::{
        ftp_test::{Test, TestHeartRate},
        range::Ranges,
    },
    normalised::OperatorZone,
};
use infrastructure::{
    GarminRecordedDevices, GarminWear, SqliteCanonicalCyclingSessionStore,
    SqliteCanonicalWeighInStore, SqliteFtpHistory,
};

use crate::clock::Today;
use crate::store::Spanned;
use crate::{
    devices,
    graph::{page, read},
    next::NextFailure,
    ranges::{self, Current},
    store::Store,
};

/// Every test, oldest first, in watts and over body weight.
pub(crate) type Tests = FtpTests<
    Spanned<SqliteCanonicalCyclingSessionStore>,
    Spanned<SqliteFtpHistory>,
    Spanned<SqliteCanonicalWeighInStore>,
>;

page! {
    /// What the FTP tests page reads: the canonical rides, the FTP derived
    /// with them, the canonical weigh-ins, which watch was in use (#548), and
    /// the ranges.
    pub struct Page {
        tests: Tests,
        heart_rates: TestHeartRates<
            Spanned<SqliteCanonicalCyclingSessionStore>,
            Spanned<GarminRecordedDevices>,
            Spanned<GarminWear>,
        >,
        ranges: Current,
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

/// The tests over `store`.
pub(crate) fn over(store: &Store) -> Tests {
    FtpTests::new(
        store.adapter(SqliteCanonicalCyclingSessionStore::new),
        store.adapter(SqliteFtpHistory::new),
        store.adapter(SqliteCanonicalWeighInStore::new),
    )
}

impl Page {
    /// The page over `store`.
    ///
    /// # Errors
    ///
    /// [`NextFailure`] if the store cannot be read.
    pub async fn open(store: &Store, zone: &OperatorZone) -> Result<Self, NextFailure> {
        Ok(Self {
            tests: over(store).into(),
            heart_rates: TestHeartRates::new(
                store.adapter(SqliteCanonicalCyclingSessionStore::new),
                devices::register(store),
            )
            .into(),
            ranges: ranges::over(store, zone).await?.into(),
            today: Today::new(zone.clone()).into(),
        })
    }

    /// Every test, oldest first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn tests(&self) -> Result<Vec<Test>, StoreError> {
        read!(self.tests.tests()).await
    }

    /// The heart rate each test was ridden at, oldest first (#548).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn heart_rates(&self) -> Result<Vec<TestHeartRate>, StoreError> {
        read!(self.heart_rates.heart_rates()).await
    }

    /// The ranges today.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn ranges(&self) -> Result<Ranges, StoreError> {
        let today = self.today.date();
        read!(self.ranges, |of| of.on(today)).await
    }
}
