//! The school and public holidays, as this build reads them.
//!
//! **Kept as last served** (#809). Both are facts about the world, set by the
//! school and by the government; a run of `next` asks each whether it has
//! changed, and everything else reads what is kept and asks nothing.

use application::{Calendar, HolidayCalendar as _, SourceError, microcycle::Term};
use domain::schedule::Holidays;
use infrastructure::{BankHolidays, GOV_UK_BANK_HOLIDAYS, SchoolCalendar, SqliteCalendarStore};

use crate::store::Spanned;
use crate::store::Store;

/// The school the operator's family follows, and the iCal feed its website
/// publishes. Its term dates, not the council's recommended ones: a school may
/// set its own.
const SCHOOL_CALENDAR: &str = "https://hemplandprimary.co.uk/?rhc_action=get_icalendar_events";

/// Both calendars: kept in the store, and asked of the school and gov.uk.
pub type Calendars =
    application::calendars::Calendars<Spanned<SqliteCalendarStore>, SchoolCalendar, BankHolidays>;

/// Both calendars, over `store`.
pub(crate) fn calendars(store: &Store) -> Calendars {
    Calendars::new(
        store.adapter(SqliteCalendarStore::new),
        SchoolCalendar::new(url(Calendar::School, SCHOOL_CALENDAR)),
        BankHolidays::new(url(Calendar::Public, GOV_UK_BANK_HOLIDAYS)),
    )
}

/// Where `calendar` is asked: the URL its variable states, as a source's
/// root is read from its own, and its publisher's otherwise.
fn url(calendar: Calendar, publisher: &str) -> String {
    std::env::var(variable(calendar)).unwrap_or_else(|_| publisher.to_owned())
}

/// `SCHOOL_CALENDAR_URL`, named from the calendar.
fn variable(calendar: Calendar) -> String {
    format!("{}_CALENDAR_URL", calendar.as_str().to_uppercase())
}

/// Every school and public holiday, as last served.
///
/// # Errors
///
/// [`SourceError`] if either has not been fetched yet, or what was kept
/// cannot be read.
pub async fn holidays(store: &Store) -> Result<Holidays, SourceError> {
    calendars(store).holidays().await
}

/// What the walk through the plan starts a programme again against (#732):
/// the holidays as last served, and Peloton's names for its programmes.
///
/// `None` where either calendar has not been fetched yet. Nothing starts again
/// then, and a re-run lengthens its mesocycle as it did before.
pub async fn term(store: &Store) -> Option<Term> {
    Some(Term {
        holidays: calendars(store).holidays().await.ok()?,
        identify: infrastructure::peloton::provider::identify,
    })
}

#[cfg(test)]
mod tests {
    use application::Calendar;

    use super::{SCHOOL_CALENDAR, variable};

    /// Each calendar's variable, named from it.
    #[test]
    fn each_calendar_is_reached_where_its_variable_says() {
        assert_eq!(variable(Calendar::School), "SCHOOL_CALENDAR_URL");
        assert_eq!(variable(Calendar::Public), "PUBLIC_CALENDAR_URL");
    }

    /// **A stub cannot catch a wrong default**, so the feed is pinned.
    #[test]
    fn the_school_calendar_is_hempland_primarys_feed() {
        assert_eq!(
            SCHOOL_CALENDAR,
            "https://hemplandprimary.co.uk/?rhc_action=get_icalendar_events"
        );
    }
}
