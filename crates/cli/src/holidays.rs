//! `fitness holidays` — the school and public holidays still to come.
//!
//! **Read, not recorded** (#181). Both are facts about the world, set by the
//! school and by the government, so this asks them each time and stores
//! nothing. During either, the ordinary week cannot be assumed to hold.

use application::HolidayCalendar as _;
use configurator::holidays::Calendars;
use domain::schedule::Holidays;

use crate::{Failure, output};

/// List every school and public holiday from today.
pub async fn list() -> Result<(), Failure> {
    output::holidays(&read().await?, jiff::Zoned::now().date());
    Ok(())
}

/// Every school and public holiday both sources publish, past ones included:
/// a macrocycle starts where the holiday before it ended.
///
/// # Errors
///
/// [`Failure`] if either source cannot be read.
pub async fn read() -> Result<Holidays, Failure> {
    Ok(Calendars.holidays().await?)
}
