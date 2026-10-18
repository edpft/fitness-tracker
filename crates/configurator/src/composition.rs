//! Body composition, one series per method, over this build's store (#256).

use application::{StoreError, composition::Composition};
use domain::analytical::composition::ComponentSeries;
use infrastructure::SqliteCompositionHistory;

use crate::store::Store;

/// Every component anything measured, one series per method.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn by_method(store: &Store) -> Result<Vec<ComponentSeries>, StoreError> {
    Composition::new(SqliteCompositionHistory::new(store.pool().clone()))
        .by_method()
        .await
}
