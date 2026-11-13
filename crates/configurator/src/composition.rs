//! Body composition, one series per method, over this build's store (#256).

use application::composition::Composition;
use infrastructure::SqliteCompositionHistory;

use crate::store::Store;

/// Every component anything measured, one series per method.
pub type ByMethod = Composition<SqliteCompositionHistory>;

/// The composition over `store`.
pub(crate) fn over(store: &Store) -> ByMethod {
    Composition::new(SqliteCompositionHistory::new(store.pool().clone()))
}
