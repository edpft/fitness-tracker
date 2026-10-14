//! Garmin's daily heart rate: what the wrist read all day, every day (#427).
//!
//! **One landing record per day, empty days included.** `dailyHeartRate`
//! answers for any date with the day's resting heart rate and its readings,
//! and on a day nothing was worn it answers with both empty. That empty answer
//! is the evidence a watch was not worn, so it is landed as served rather than
//! skipped: the operator found no all-day heart rate from 2023-10-08 to
//! 2024-07-16, and it is what takes the Vivoactive 3 out of those rides.
//!
//! **The readings are perishable.** On 2026-10-13 Garmin served two-minute
//! readings back to somewhere after 2026-05-15 and none before, while every
//! day's resting heart rate went back years. Landing every day as it stands
//! keeps the readings that are still there; they are the only record of what
//! the wrist read during a ride.
//!
//! **The walk is by date, as the HRV walk's is** ([`super::hrv`]), and for the
//! same reason: a day is addressed by its date and there is no cursor.

use std::{sync::Arc, sync::OnceLock, time::Duration};

use application::{EventBatch, SourceError, SourceEvent, WorkoutEventSource};
use domain::landing::{
    Endpoint, EventKind, EventProvenance, EventTime, RawPayload, SourceRecordId, Watermark,
};
use jiff::civil::Date;

use super::auth::GarminAuth;

/// Where a day's heart rate is served from; the date is a query parameter.
const DAILY_PATH: &str = "/wellness-service/wellness/dailyHeartRate";

/// The earliest day worth asking for.
///
/// **The operator's device history, not a property of the API.** The Vivoactive
/// 3, first recording 2018-01-06, is the first watch he owned with a wrist
/// sensor; the Forerunner 310XT and 10 before it had none, so nothing before
/// could have been worn in this sense.
const EARLIEST: Date = jiff::civil::date(2018, 1, 6);

/// How many days one batch asks for: the unit of work between commits.
const DAYS_PER_BATCH: usize = 7;

/// How long to wait between requests, for [`super::hrv`]'s reason.
const BETWEEN_REQUESTS: Duration = Duration::from_millis(250);

/// Where a walk has got to: the next day to ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayWalk {
    next: Date,
}

/// Garmin's daily heart rate.
#[derive(Debug)]
pub struct GarminDailyHeartRate {
    auth: Arc<GarminAuth>,
    api_base: String,
    client: OnceLock<Result<reqwest::Client, String>>,
    /// What "today" means to this walk, injected so a test does not depend on
    /// the day it runs.
    today: Date,
}

impl GarminDailyHeartRate {
    pub fn new(api_base: impl Into<String>, auth: impl Into<Arc<GarminAuth>>, today: Date) -> Self {
        Self {
            auth: auth.into(),
            api_base: api_base.into().trim_end_matches('/').to_owned(),
            client: OnceLock::new(),
            today,
        }
    }

    fn client(&self) -> Result<&reqwest::Client, SourceError> {
        self.client
            .get_or_init(|| {
                reqwest::Client::builder()
                    .timeout(Duration::from_secs(30))
                    .build()
                    .map_err(|error| error.to_string())
            })
            .as_ref()
            .map_err(|detail| SourceError::Unavailable {
                detail: detail.clone(),
            })
    }

    /// One day, as served.
    async fn day(&self, date: Date, endpoint: &Endpoint) -> Result<SourceEvent, SourceError> {
        let bearer = self.auth.bearer().await?;
        let response = self
            .client()?
            .get(format!("{}{DAILY_PATH}", self.api_base))
            .query(&[("date", date.to_string())])
            .bearer_auth(bearer)
            .send()
            .await
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })?;
        let bytes = super::answer_bytes(response, DAILY_PATH).await?;

        // **The date asked for**, which is what makes the record addressable
        // and the walk monotonic.
        let source_record_id =
            SourceRecordId::try_from(date.to_string()).map_err(|error| SourceError::Malformed {
                detail: error.to_string(),
            })?;
        let payload = RawPayload::try_from(bytes).map_err(|error| SourceError::Malformed {
            detail: error.to_string(),
        })?;
        Ok(SourceEvent::new(
            source_record_id,
            EventProvenance::new(
                endpoint.clone(),
                EventKind::Updated,
                Some(EventTime::from(midnight(date)?)),
            )
            .into(),
            payload,
        ))
    }
}

/// Where a walk begins: the watermark's own day, which is asked for again
/// because today's answer grows until the day is over, or the earliest.
fn start_from(since: Option<Watermark>) -> Date {
    let Some(mark) = since else {
        return EARLIEST;
    };
    let at = mark.as_timestamp().to_zoned(jiff::tz::TimeZone::UTC).date();
    if at < EARLIEST { EARLIEST } else { at }
}

fn midnight(date: Date) -> Result<jiff::Timestamp, SourceError> {
    date.to_zoned(jiff::tz::TimeZone::UTC)
        .map(|zoned| zoned.timestamp())
        .map_err(|error| SourceError::Malformed {
            detail: format!("{date} is not a date this clock can place: {error}"),
        })
}

impl WorkoutEventSource for GarminDailyHeartRate {
    type Resume = DayWalk;

    async fn fetch(
        &self,
        since: Option<Watermark>,
        resume: Option<DayWalk>,
    ) -> Result<EventBatch<DayWalk>, SourceError> {
        let mut date = resume.map_or_else(|| start_from(since), |walk| walk.next);
        let endpoint = Endpoint::try_from(DAILY_PATH).map_err(|error| SourceError::Malformed {
            detail: error.to_string(),
        })?;

        let mut events = Vec::new();
        while events.len() < DAYS_PER_BATCH && date <= self.today {
            if !events.is_empty() {
                tokio::time::sleep(BETWEEN_REQUESTS).await;
            }
            events.push(self.day(date, &endpoint).await?);
            date = date.tomorrow().map_err(|error| SourceError::Malformed {
                detail: format!("there is no day after {date}: {error}"),
            })?;
        }
        let resume = (date <= self.today).then_some(DayWalk { next: date });
        Ok(EventBatch { events, resume })
    }
}

#[cfg(test)]
mod tests {
    use super::{DAILY_PATH, EARLIEST, start_from};
    use domain::landing::{Endpoint, EventTime, Watermark};
    use jiff::civil::date;

    #[test]
    fn the_path_is_an_endpoint() {
        assert_eq!(
            Endpoint::try_from(DAILY_PATH)
                .expect("an endpoint")
                .as_str(),
            "/wellness-service/wellness/dailyHeartRate"
        );
    }

    /// A first walk starts on the first day a wrist sensor could have been worn.
    #[test]
    fn a_first_walk_starts_with_the_first_wrist_sensor() {
        assert_eq!(start_from(None), EARLIEST);
        assert_eq!(EARLIEST, date(2018, 1, 6));
    }

    /// The watermark's own day is asked for again: today grows until it ends.
    #[test]
    fn a_later_walk_re_asks_the_watermark_s_day() {
        let at = date(2026, 10, 13)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .expect("a zoned date")
            .timestamp();
        let mark = Watermark::from(EventTime::from(at));
        assert_eq!(start_from(Some(mark)), date(2026, 10, 13));
    }
}
