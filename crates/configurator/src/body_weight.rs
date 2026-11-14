//! Body weight as one time series, and what the plans aimed it at, over this
//! build's store (#250, #689).

use application::body_weight::{Aims, BodyWeight};
use infrastructure::{SqliteCalendarStore, SqliteCanonicalWeighInStore};

use crate::{rescheduling::Plans, store::Store};

/// Every canonical weigh-in, and the trend through them.
pub type Series = BodyWeight<SqliteCanonicalWeighInStore>;

/// What the plans intended over this macrocycle's mesocycles, as they now
/// stand (#689, #800).
pub type Aimed = Aims<Plans, SqliteCalendarStore>;

/// The series over `store`.
pub(crate) fn series(store: &Store) -> Series {
    BodyWeight::new(SqliteCanonicalWeighInStore::new(store.pool().clone()))
}

/// The aims of `plans`.
pub(crate) fn aims_over(store: &Store, plans: Plans) -> Aimed {
    Aims::new(plans, SqliteCalendarStore::new(store.pool().clone()))
}
