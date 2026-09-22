//! Public holidays, from gov.uk.
//!
//! **England and Wales**, which is where the operator lives. gov.uk serves
//! Scotland and Northern Ireland beside it, and their holidays differ.

use std::sync::OnceLock;

use application::{HolidayCalendar, SourceError};
use domain::schedule::{Holidays, PublicHoliday};
use jiff::civil::Date;
use serde::Deserialize;

/// Where gov.uk publishes them.
pub const GOV_UK_BANK_HOLIDAYS: &str = "https://www.gov.uk/bank-holidays.json";

/// gov.uk's list of bank holidays.
#[derive(Debug)]
pub struct BankHolidays {
    url: String,
    client: OnceLock<Result<reqwest::Client, String>>,
}

impl BankHolidays {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            client: OnceLock::new(),
        }
    }
}

impl HolidayCalendar for BankHolidays {
    async fn holidays(&self) -> Result<Holidays, SourceError> {
        let served = super::get(super::client(&self.client)?, &self.url).await?;
        public_holidays(&served)
    }
}

#[derive(Deserialize)]
struct Served {
    #[serde(rename = "england-and-wales")]
    england_and_wales: Division,
}

#[derive(Deserialize)]
struct Division {
    events: Vec<Event>,
}

#[derive(Deserialize)]
struct Event {
    title: String,
    date: String,
    #[serde(default)]
    notes: String,
}

/// England and Wales's public holidays, from what gov.uk serves, published to
/// the last of them.
///
/// **A substitute day says so**, because it is not the day the name suggests:
/// Boxing Day 2026 falls on a Saturday and is taken on Monday the 28th.
///
/// # Errors
///
/// [`SourceError::Malformed`] where the list or a date in it cannot be read.
pub fn public_holidays(served: &str) -> Result<Holidays, SourceError> {
    let served: Served = serde_json::from_str(served).map_err(|error| SourceError::Malformed {
        detail: format!("gov.uk's bank holidays: {error}"),
    })?;

    served
        .england_and_wales
        .events
        .into_iter()
        .map(|event| {
            let date: Date = event.date.parse().map_err(|_| SourceError::Malformed {
                detail: format!(
                    "gov.uk's bank holiday {:?} on {:?}",
                    event.title, event.date
                ),
            })?;
            let name = if event.notes.is_empty() {
                event.title
            } else {
                format!("{}, {}", event.title, event.notes.to_lowercase())
            };
            Ok(PublicHoliday::new(date, name))
        })
        .collect::<Result<Vec<_>, SourceError>>()
        .map(|holidays| {
            let reach = holidays.iter().map(PublicHoliday::date).max();
            Holidays::from_public(holidays, reach)
        })
}

#[cfg(test)]
mod tests {
    use super::{GOV_UK_BANK_HOLIDAYS, public_holidays};
    use jiff::civil::date;

    /// Two divisions, as gov.uk serves them, and only England and Wales read.
    #[test]
    fn england_and_wales_only_and_a_substitute_day_says_so() {
        let served = r#"{
            "england-and-wales": {"division": "england-and-wales", "events": [
                {"title": "Christmas Day", "date": "2026-12-25", "notes": "", "bunting": true},
                {"title": "Boxing Day", "date": "2026-12-28", "notes": "Substitute day", "bunting": true}
            ]},
            "scotland": {"division": "scotland", "events": [
                {"title": "2nd January", "date": "2027-01-04", "notes": "Substitute day", "bunting": true}
            ]}
        }"#;
        let holidays = public_holidays(served).expect("the list reads");
        assert_eq!(holidays.public_published_to(), Some(date(2026, 12, 28)));

        let read: Vec<(jiff::civil::Date, &str)> = holidays
            .public()
            .iter()
            .map(|holiday| (holiday.date(), holiday.name()))
            .collect();
        assert_eq!(
            read,
            [
                (date(2026, 12, 25), "Christmas Day"),
                (date(2026, 12, 28), "Boxing Day, substitute day"),
            ]
        );
    }

    /// **A stub cannot catch a wrong default**, so the default is pinned.
    #[test]
    fn the_default_is_gov_uk() {
        assert_eq!(
            GOV_UK_BANK_HOLIDAYS,
            "https://www.gov.uk/bank-holidays.json"
        );
    }
}
