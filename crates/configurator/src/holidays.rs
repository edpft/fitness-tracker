//! The school and public holidays, as this build reads them.
//!
//! **Read, not recorded** (#181). Both are facts about the world, set by the
//! school and by the government, so they are asked each time and nothing is
//! stored.

use application::{HolidayCalendar, SourceError};
use domain::schedule::Holidays;
use infrastructure::{BankHolidays, GOV_UK_BANK_HOLIDAYS, SchoolCalendar};

/// The school the operator's family follows, and the iCal feed its website
/// publishes. Its term dates, not the council's recommended ones: a school may
/// set its own.
const SCHOOL_CALENDAR: &str = "https://hemplandprimary.co.uk/?rhc_action=get_icalendar_events";

/// Both calendars, as one: what a macrocycle is read against.
pub struct Calendars;

impl HolidayCalendar for Calendars {
    async fn holidays(&self) -> Result<Holidays, SourceError> {
        let school = SchoolCalendar::new(SCHOOL_CALENDAR).holidays().await?;
        let public = BankHolidays::new(GOV_UK_BANK_HOLIDAYS).holidays().await?;
        Ok(school.and(public))
    }
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
