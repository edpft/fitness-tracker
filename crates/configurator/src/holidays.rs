//! The school and public holidays, as this build reads them.
//!
//! **Kept as last served** (#809). Both are facts about the world, set by the
//! school and by the government; a run of `next` asks each whether it has
//! changed, and everything else reads what is kept and asks nothing.

use application::{HolidayCalendar as _, SourceError, microcycle::Term};
use domain::schedule::Holidays;
use infrastructure::{
    BankHolidays, GOV_UK_BANK_HOLIDAYS, SchoolCalendar, SqliteCalendarStore, SqlitePool,
};

use crate::store::Store;

/// The school the operator's family follows, and the iCal feed its website
/// publishes. Its term dates, not the council's recommended ones: a school may
/// set its own.
const SCHOOL_CALENDAR: &str = "https://hemplandprimary.co.uk/?rhc_action=get_icalendar_events";

/// Both calendars: kept in the store, and asked of the school and gov.uk.
pub type Calendars =
    application::calendars::Calendars<SqliteCalendarStore, SchoolCalendar, BankHolidays>;

/// Both calendars, over the store in `pool`.
pub(crate) fn calendars(pool: &SqlitePool) -> Calendars {
    Calendars::new(
        SqliteCalendarStore::new(pool.clone()),
        SchoolCalendar::new(SCHOOL_CALENDAR),
        BankHolidays::new(GOV_UK_BANK_HOLIDAYS),
    )
}

/// Every school and public holiday, as last served.
///
/// # Errors
///
/// [`SourceError`] if either has not been fetched yet, or what was kept
/// cannot be read.
pub async fn holidays(store: &Store) -> Result<Holidays, SourceError> {
    calendars(store.pool()).holidays().await
}

/// What the walk through the plan starts a programme again against (#732):
/// the holidays as last served, and Peloton's names for its programmes.
///
/// `None` where either calendar has not been fetched yet. Nothing starts again
/// then, and a re-run lengthens its mesocycle as it did before.
pub async fn term(pool: &SqlitePool) -> Option<Term> {
    Some(Term {
        holidays: calendars(pool).holidays().await.ok()?,
        identify: infrastructure::peloton::provider::identify,
    })
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
