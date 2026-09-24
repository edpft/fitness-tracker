//! `fitness holidays` — the school and public holidays still to come.
//!
//! **Read, not recorded** (#181). Both are facts about the world, set by the
//! school and by the government, so this asks them each time and stores
//! nothing. During either, the ordinary week cannot be assumed to hold.

use application::HolidayCalendar as _;
use domain::schedule::Holidays;
use infrastructure::{BankHolidays, GOV_UK_BANK_HOLIDAYS, SchoolCalendar};

use crate::{Failure, output};

/// The school the operator's family follows, and the iCal feed its website
/// publishes. Its term dates, not the council's recommended ones: a school may
/// set its own.
const SCHOOL_CALENDAR: &str = "https://hemplandprimary.co.uk/?rhc_action=get_icalendar_events";

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
    let school = SchoolCalendar::new(SCHOOL_CALENDAR).holidays().await?;
    let public = BankHolidays::new(GOV_UK_BANK_HOLIDAYS).holidays().await?;
    Ok(school.and(public))
}

#[cfg(test)]
mod tests {
    use super::SCHOOL_CALENDAR;

    /// **A stub cannot catch a wrong default**, so the feed is pinned.
    #[test]
    fn the_school_calendar_is_hempland_primarys_feed() {
        assert_eq!(
            SCHOOL_CALENDAR,
            "https://hemplandprimary.co.uk/?rhc_action=get_icalendar_events"
        );
    }
}
