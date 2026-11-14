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

use crate::{
    devices,
    graph::page,
    next::NextFailure,
    ranges::{self, Current},
    store::Store,
};

/// Every test, oldest first, in watts and over body weight.
pub(crate) type Tests =
    FtpTests<SqliteCanonicalCyclingSessionStore, SqliteFtpHistory, SqliteCanonicalWeighInStore>;

page! {
    /// What the FTP tests page reads: the canonical rides, the FTP derived
    /// with them, the canonical weigh-ins, which watch was in use (#548), and
    /// the ranges.
    pub struct Page {
        tests: Tests,
        heart_rates: TestHeartRates<
            SqliteCanonicalCyclingSessionStore,
            GarminRecordedDevices,
            GarminWear,
        >,
        ranges: Current,
    }
}

/// The parts a run collecting what the page reads takes.
#[must_use]
pub fn collects() -> Scope {
    crate::graph::collects::<Page>()
}

/// The tests over `store`.
pub(crate) fn over(store: &Store) -> Tests {
    let pool = store.pool();
    FtpTests::new(
        SqliteCanonicalCyclingSessionStore::new(pool.clone()),
        SqliteFtpHistory::new(pool.clone()),
        SqliteCanonicalWeighInStore::new(pool.clone()),
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
            tests: over(store),
            heart_rates: TestHeartRates::new(
                SqliteCanonicalCyclingSessionStore::new(store.pool().clone()),
                devices::register(store),
            ),
            ranges: ranges::over(store, zone).await?,
        })
    }

    /// Every test, oldest first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn tests(&self) -> Result<Vec<Test>, StoreError> {
        self.tests.tests().await
    }

    /// The heart rate each test was ridden at, oldest first (#548).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn heart_rates(&self) -> Result<Vec<TestHeartRate>, StoreError> {
        self.heart_rates.heart_rates().await
    }

    /// The ranges today.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn ranges(&self, zone: &OperatorZone) -> Result<Ranges, StoreError> {
        ranges::today(&self.ranges, zone).await
    }
}
