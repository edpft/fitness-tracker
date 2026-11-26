//! `fitness holidays` — the school and public holidays still to come.
//!
//! **As last served** (#809). Both are facts about the world, set by the
//! school and by the government, and a run of `next` asks each whether it has
//! changed. During either, the ordinary week cannot be assumed to hold.

use configurator::store::Store;
use domain::schedule::Holidays;

use crate::{Failure, output};

/// List every school and public holiday from today.
pub async fn list(store: &Store) -> Result<(), Failure> {
    output::holidays(&read(store).await?, jiff::ZonedDateTime::now().date());
    Ok(())
}

/// Every school and public holiday both sources publish, past ones included:
/// a macrocycle starts where the holiday before it ended.
///
/// # Errors
///
/// [`Failure`] if the store cannot be opened, or either has not been fetched.
pub async fn read(store: &Store) -> Result<Holidays, Failure> {
    let holidays = configurator::holidays::holidays(store).await;
    Ok(holidays?)
}
