//! The school's calendar, as the iCal feed its website publishes.
//!
//! **Holidays are recognised by title**, because nothing else marks them: the
//! feed carries trips, discos and swimming lessons beside them with no
//! category, and every holiday — Christmas and summer included — is titled
//! "Half term". A title naming a *bank* holiday is not a school holiday: the
//! feed carries the May ones, and gov.uk is where public holidays come from.
//!
//! **Training days are not holidays.** The school is shut to pupils, and for
//! the operator it is an ordinary working day (2026-09-22).

use std::{num::NonZeroU8, sync::OnceLock};

use application::{HolidayCalendar, SourceError};
use domain::schedule::{Holidays, SchoolHoliday};
use jiff::civil::Date;

/// A school's calendar feed.
#[derive(Debug)]
pub struct SchoolCalendar {
    url: String,
    client: OnceLock<Result<reqwest::Client, String>>,
}

impl SchoolCalendar {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            client: OnceLock::new(),
        }
    }

    fn client(&self) -> Result<&reqwest::Client, SourceError> {
        super::client(&self.client)
    }
}

impl HolidayCalendar for SchoolCalendar {
    async fn holidays(&self) -> Result<Holidays, SourceError> {
        let served = super::get(self.client()?, &self.url).await?;
        school_holidays(&served)
    }
}

/// One event's fields, as far as a holiday needs them.
#[derive(Default)]
struct Event {
    start: Option<String>,
    end: Option<String>,
    summary: Option<String>,
}

/// Every school holiday in a served feed, and how far the feed reaches.
///
/// **The reach is the last day of any event, holiday or not**: the feed is
/// the school's whole calendar, so past its last event it says nothing at all,
/// which is no data rather than no holiday.
///
/// # Errors
///
/// [`SourceError::Malformed`] where a holiday's dates cannot be read, or it
/// runs longer than a run of days can say.
pub fn school_holidays(served: &str) -> Result<Holidays, SourceError> {
    // A long line is folded by starting its continuation with a space.
    let unfolded = served
        .replace("\r\n", "\n")
        .replace("\n ", "")
        .replace("\n\t", "");

    let mut holidays = Vec::new();
    let mut reach: Option<Date> = None;
    let mut event: Option<Event> = None;
    for line in unfolded.lines() {
        match line {
            "BEGIN:VEVENT" => event = Some(Event::default()),
            "END:VEVENT" => {
                let Some(ended) = event.take() else { continue };
                reach = reach.max(last_day(&ended));
                if let Some(holiday) = holiday(ended)? {
                    holidays.push(holiday);
                }
            }
            _ => {
                let (Some(current), Some((name, value))) = (event.as_mut(), line.split_once(':'))
                else {
                    continue;
                };
                // Parameters follow the name after a semicolon:
                // `DTSTART;VALUE=DATE:20261221`.
                match name.split(';').next() {
                    Some("DTSTART") => current.start = Some(value.to_owned()),
                    Some("DTEND") => current.end = Some(value.to_owned()),
                    Some("SUMMARY") => current.summary = Some(value.to_owned()),
                    _ => {}
                }
            }
        }
    }
    Ok(Holidays::from_school(holidays, reach))
}

/// The last day an event covers: the day before an all-day event's
/// exclusive end, or else the day it starts.
fn last_day(event: &Event) -> Option<Date> {
    let start = event
        .start
        .as_deref()
        .and_then(|value| day(value.get(..8)?))?;
    let end = event
        .end
        .as_deref()
        .and_then(day)
        .filter(|end| *end > start)
        .and_then(|end| end.yesterday().ok());
    Some(end.unwrap_or(start))
}

fn is_holiday(summary: &str) -> bool {
    let summary = summary.to_lowercase();
    !summary.contains("bank holiday")
        && (summary.contains("half term") || summary.contains("holiday"))
}

/// An all-day date, `20261221`. A timed one carries a `T` and is not a
/// holiday.
fn day(value: &str) -> Option<Date> {
    let digits = value.get(..8)?;
    if value.len() != 8 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Date::new(
        digits.get(..4)?.parse().ok()?,
        digits.get(4..6)?.parse().ok()?,
        digits.get(6..)?.parse().ok()?,
    )
    .ok()
}

/// The holiday an event is, if it is one.
///
/// **The end date is exclusive**, as iCal's all-day events are: Christmas 2026
/// is served as `20261221` to `20270104`, and the school is back on the 4th.
/// An end on or before the start — the feed has one — is read as a single day.
fn holiday(event: Event) -> Result<Option<SchoolHoliday>, SourceError> {
    let (Some(summary), Some(start)) = (event.summary, event.start) else {
        return Ok(None);
    };
    if !is_holiday(&summary) {
        return Ok(None);
    }
    let Some(first) = day(&start) else {
        // A holiday at a time of day is not a run of days.
        return Ok(None);
    };

    let length = match event.end.as_deref().and_then(day) {
        Some(end) if end > first => first
            .until(end)
            .map_err(|error| malformed(&summary, &error))?
            .get_days(),
        _ => 1,
    };
    let days = u8::try_from(length)
        .ok()
        .and_then(NonZeroU8::new)
        .ok_or_else(|| malformed(&summary, &format!("{length} days is not a run of days")))?;

    Ok(Some(SchoolHoliday::new(first, days)))
}

fn malformed(summary: &str, detail: &dyn std::fmt::Display) -> SourceError {
    SourceError::Malformed {
        detail: format!("the school holiday {summary:?}: {detail}"),
    }
}

#[cfg(test)]
mod tests {
    use super::school_holidays;
    use jiff::civil::date;

    /// Four events as the school's feed serves them: Christmas, a training
    /// day, a bank holiday and a disco.
    const SERVED: &str = "BEGIN:VCALENDAR\r\n\
        VERSION:2.0\r\n\
        BEGIN:VEVENT\r\n\
        DTSTART;VALUE=DATE:20261221\r\n\
        DTEND;VALUE=DATE:20270104\r\n\
        SUMMARY:Half term\r\n\
        END:VEVENT\r\n\
        BEGIN:VEVENT\r\n\
        DTSTART;VALUE=DATE:20261130\r\n\
        DTEND;VALUE=DATE:20261130\r\n\
        SUMMARY:Training day\r\n\
        END:VEVENT\r\n\
        BEGIN:VEVENT\r\n\
        DTSTART;VALUE=DATE:20260504\r\n\
        DTEND;VALUE=DATE:20260504\r\n\
        SUMMARY:Bank Holiday\r\n\
        END:VEVENT\r\n\
        BEGIN:VEVENT\r\n\
        DTSTART;TZID=Europe/London:20251204T153000\r\n\
        DTEND;TZID=Europe/London:20251204T163000\r\n\
        SUMMARY:Year 1&2 Friends Christmas\r\n  Disco\r\n\
        END:VEVENT\r\n\
        END:VCALENDAR\r\n";

    /// **Only the holiday**, running to the 3rd because the end is exclusive.
    #[test]
    fn only_the_holiday_is_kept_and_its_end_is_exclusive() {
        let read = school_holidays(SERVED).expect("the feed reads");
        let holidays = read.school();

        assert_eq!(
            holidays.len(),
            1,
            "not the training day, bank holiday or disco"
        );
        assert_eq!(holidays[0].start(), date(2026, 12, 21));
        assert_eq!(holidays[0].last(), date(2027, 1, 3));
    }

    /// **The feed reaches to its last event, holiday or not**: the holiday
    /// ends on 3 January, and nothing later is served.
    #[test]
    fn the_feed_reaches_to_its_last_event() {
        let read = school_holidays(SERVED).expect("the feed reads");
        assert_eq!(read.school_published_to(), Some(date(2027, 1, 3)));
    }

    /// The feed's one holiday whose end is its start is a single day.
    #[test]
    fn an_end_on_the_start_is_one_day() {
        let served = "BEGIN:VEVENT\nDTSTART;VALUE=DATE:20250723\n\
            DTEND;VALUE=DATE:20250723\nSUMMARY:Summer Holidays\nEND:VEVENT\n";
        let read = school_holidays(served).expect("the feed reads");
        let holidays = read.school();
        assert_eq!(holidays.len(), 1);
        assert_eq!(holidays[0].days().get(), 1);
    }
}
