//! Body weight as one time series, and what the plans aimed it at, over this
//! build's store (#250, #689).

use application::body_weight::{Aims, BodyWeight};
use infrastructure::{SqliteCalendarStore, SqliteCanonicalWeighInStore};

use crate::store::Spanned;
use crate::store::Store;

/// Every canonical weigh-in, and the trend through them.
pub type Series = BodyWeight<Spanned<SqliteCanonicalWeighInStore>>;

/// What the plans intended over this macrocycle's mesocycles (#689).
pub type Aimed = Aims<Spanned<SqliteCalendarStore>>;

/// The series over `store`.
pub(crate) fn series(store: &Store) -> Series {
    BodyWeight::new(store.adapter(SqliteCanonicalWeighInStore::new))
}

/// The aims, over `store`'s school calendar.
pub(crate) fn aims_over(store: &Store) -> Aimed {
    Aims::new(store.adapter(SqliteCalendarStore::new))
}
