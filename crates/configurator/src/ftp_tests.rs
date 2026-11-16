//! The FTP tests, in watts and over body weight, over this build's store
//! (#469).

use application::ftp_tests::FtpTests;
use infrastructure::SqliteCanonicalCyclingSessionStore;

use crate::store::{Spanned, Store};

/// Every test, oldest first, in watts and over body weight.
pub(crate) type Tests = FtpTests<Spanned<SqliteCanonicalCyclingSessionStore>>;

/// The tests over `store`.
pub(crate) fn over(store: &Store) -> Tests {
    FtpTests::new(store.adapter(SqliteCanonicalCyclingSessionStore::new))
}
