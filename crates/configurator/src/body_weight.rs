//! Body weight as one time series, over this build's store (#250).

use application::body_weight::BodyWeight;
use infrastructure::SqliteCanonicalWeighInStore;

use crate::store::Spanned;
use crate::store::Store;

/// Every canonical weigh-in, and the trend through them.
pub type Series = BodyWeight<Spanned<SqliteCanonicalWeighInStore>>;

/// The series over `store`.
pub(crate) fn series(store: &Store) -> Series {
    BodyWeight::new(store.adapter(SqliteCanonicalWeighInStore::new))
}
