//! The macrocycle in force, over this build's store: what Plan opens with
//! (#770).

use application::{
    StoreError,
    macrocycle::{InForce, Macrocycle},
};
use domain::normalised::OperatorZone;

use crate::store::Store;

/// The macrocycle in force today, `None` where no plan covers today.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn today(store: &Store, zone: &OperatorZone) -> Result<Option<Macrocycle>, StoreError> {
    let today = jiff::Timestamp::now().to_zoned(zone.as_time_zone()).date();
    InForce::new(store.plans(zone)).on(today).await
}
