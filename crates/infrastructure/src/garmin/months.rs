//! Garmin's metrics that are served over a span of dates rather than one day at
//! a time (#269): VO2 max as it changed, and the Health Snapshots taken.
//!
//! **A month at a time.** Both answer for any range, with one entry per day
//! that has one, and most days have none: on 2026-10-21 the operator's account
//! held 608 VO2 max entries from 2018 on and one Health Snapshot. A month is a
//! landing record, so a year's walk is twelve requests rather than the 365 a
//! day walk would make, and a record is still small enough to read.
//!
//! **The current month is asked for again**, because its answer grows until the
//! month is over; an unchanged answer lands nothing.

use std::{sync::Arc, time::Duration};

use application::{EventBatch, SourceError, SourceEvent, WorkoutEventSource};
use domain::landing::{
    Endpoint, EventKind, EventProvenance, EventTime, RawPayload, SourceRecordId, Watermark,
};
use jiff::{ToSpan as _, civil::Date};

use super::{auth::GarminAuth, connect::Connect};

/// The first month worth asking for: the operator's first recording with a
/// heart rate, on the Forerunner 310XT, was 2014-12-12.
const EARLIEST: Date = jiff::civil::date(2014, 12, 1);

/// How many months one batch asks for: the unit of work between commits.
const MONTHS_PER_BATCH: usize = 6;

/// How long to wait between requests, for [`super::hrv`]'s reason.
const BETWEEN_REQUESTS: Duration = Duration::from_millis(250);

/// A metric Garmin serves over a span of dates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GarminMonth {
    /// VO2 max, each day it changed, for running and for cycling.
    Vo2Max,
    /// The two-minute Health Snapshots taken on the watch.
    HealthSnapshots,
}

impl GarminMonth {
    /// The endpoint, without the range: what every record from it is tagged
    /// with.
    pub const fn endpoint(self) -> &'static str {
        match self {
            Self::Vo2Max => "/metrics-service/metrics/maxmet/daily",
            Self::HealthSnapshots => "/wellnessactivity-service/activity/summary/range",
        }
    }

    /// The path that asks for `first` to `last`, both inclusive.
    fn path(self, first: Date, last: Date) -> String {
        format!("{}/{first}/{last}", self.endpoint())
    }
}

/// Where a walk has got to: the first day of the next month to ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthWalk {
    next: Date,
}

/// One of Garmin's span-served metrics, a month at a time.
#[derive(Debug)]
pub struct GarminMonths {
    month: GarminMonth,
    connect: Connect,
    /// What "today" means to this walk, injected so a test does not depend on
    /// the day it runs.
    today: Date,
}

impl GarminMonths {
    pub fn new(
        month: GarminMonth,
        api_base: impl Into<String>,
        auth: impl Into<Arc<GarminAuth>>,
        today: Date,
    ) -> Self {
        Self {
            month,
            connect: Connect::new(api_base, auth),
            today,
        }
    }

    /// One month, as served.
    async fn month(&self, first: Date, endpoint: &Endpoint) -> Result<SourceEvent, SourceError> {
        let malformed = |detail: String| SourceError::Malformed { detail };
        let last = first.last_of_month();
        let bytes = self.connect.get(&self.month.path(first, last), &[]).await?;

        // **The month asked for**, which is what makes the record addressable
        // and the walk monotonic.
        let source_record_id = SourceRecordId::try_from(first.strftime("%Y-%m").to_string())
            .map_err(|error| malformed(error.to_string()))?;
        let payload = RawPayload::try_from(bytes).map_err(|error| malformed(error.to_string()))?;
        Ok(SourceEvent::new(
            source_record_id,
            EventProvenance::new(
                endpoint.clone(),
                EventKind::Updated,
                Some(EventTime::from(midnight(first)?)),
            )
            .into(),
            payload,
        ))
    }
}

/// Where a walk begins: the watermark's own month, which is asked for again,
/// or the earliest.
fn start_from(since: Option<Watermark>) -> Date {
    let Some(mark) = since else {
        return EARLIEST;
    };
    let at = mark
        .as_timestamp()
        .to_zoned(jiff::tz::TimeZone::UTC)
        .date()
        .first_of_month();
    if at < EARLIEST { EARLIEST } else { at }
}

fn midnight(date: Date) -> Result<jiff::Timestamp, SourceError> {
    date.to_zoned(jiff::tz::TimeZone::UTC)
        .map(|zoned| zoned.timestamp())
        .map_err(|error| SourceError::Malformed {
            detail: format!("{date} is not a date this clock can place: {error}"),
        })
}

impl WorkoutEventSource for GarminMonths {
    type Resume = MonthWalk;

    async fn fetch(
        &self,
        since: Option<Watermark>,
        resume: Option<MonthWalk>,
    ) -> Result<EventBatch<MonthWalk>, SourceError> {
        let mut first = resume.map_or_else(|| start_from(since), |walk| walk.next);
        let endpoint =
            Endpoint::try_from(self.month.endpoint()).map_err(|error| SourceError::Malformed {
                detail: error.to_string(),
            })?;

        let mut events = Vec::new();
        while events.len() < MONTHS_PER_BATCH && first <= self.today {
            if !events.is_empty() {
                tokio::time::sleep(BETWEEN_REQUESTS).await;
            }
            events.push(self.month(first, &endpoint).await?);
            first = first
                .checked_add(1.month())
                .map_err(|error| SourceError::Malformed {
                    detail: format!("there is no month after {first}: {error}"),
                })?;
        }
        let resume = (first <= self.today).then_some(MonthWalk { next: first });
        Ok(EventBatch { events, resume })
    }
}

#[cfg(test)]
mod tests {
    use super::{EARLIEST, GarminMonth, start_from};
    use domain::landing::{Endpoint, EventTime, Watermark};
    use jiff::civil::date;

    #[test]
    fn every_path_is_an_endpoint() {
        for month in [GarminMonth::Vo2Max, GarminMonth::HealthSnapshots] {
            assert_eq!(
                Endpoint::try_from(month.endpoint())
                    .expect("an endpoint")
                    .as_str(),
                month.endpoint()
            );
        }
    }

    /// The requests a live run composes, which no stub can catch.
    #[test]
    fn a_month_is_its_first_day_to_its_last() {
        let first = date(2024, 2, 1);
        assert_eq!(
            GarminMonth::Vo2Max.path(first, first.last_of_month()),
            "/metrics-service/metrics/maxmet/daily/2024-02-01/2024-02-29"
        );
        assert_eq!(
            GarminMonth::HealthSnapshots.path(first, first.last_of_month()),
            "/wellnessactivity-service/activity/summary/range/2024-02-01/2024-02-29"
        );
    }

    #[test]
    fn a_first_walk_starts_with_the_first_heart_rate() {
        assert_eq!(start_from(None), EARLIEST);
    }

    /// The watermark's own month is asked for again: it grows until it ends.
    #[test]
    fn a_later_walk_re_asks_the_watermark_s_month() {
        let at = date(2026, 10, 1)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .expect("a zoned date")
            .timestamp();
        let mark = Watermark::from(EventTime::from(at));
        assert_eq!(start_from(Some(mark)), date(2026, 10, 1));
    }
}
