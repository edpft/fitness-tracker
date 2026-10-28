//! The FTP tests, in watts and over body weight, over this build's store
//! (#469).

use application::{CanonicalLayer, StoreError, ftp_tests::FtpTests, next::Scope};
use domain::analytical::ftp_test::Test;
use infrastructure::{SqliteCanonicalCyclingSessionStore, SqliteFtpHistory};

use crate::{
    next::{self, Reads},
    store::Store,
    wiring::WiringError,
};

/// What the FTP tests chart reads: the canonical rides, the FTP derived with
/// them, and the canonical weigh-ins.
///
/// # Errors
///
/// [`WiringError`] if a name read is not a stream's.
pub fn collects() -> Result<Scope, WiringError> {
    next::scope([
        Reads::Canonical(CanonicalLayer::Cycling),
        Reads::Canonical(CanonicalLayer::WeighIns),
    ])
}

/// Every test, oldest first.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn tests(store: &Store) -> Result<Vec<Test>, StoreError> {
    let pool = store.pool();
    FtpTests::new(
        SqliteCanonicalCyclingSessionStore::new(pool.clone()),
        SqliteFtpHistory::new(pool.clone()),
        store.weigh_ins(),
    )
    .tests()
    .await
}
