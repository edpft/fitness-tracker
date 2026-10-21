//! Garmin's daily metrics: what the watch recorded each day, every day (#427,
//! #269).
//!
//! **One walk, one metric at a time.** Every daily metric Garmin serves is
//! addressed the same way — by the date, with no cursor — so one walk asks for
//! each day in turn and lands what came back. Which metric it asks for is a
//! [`GarminDay`], and each is a stream of its own, landing in its own table and
//! resuming from its own point.
//!
//! **One landing record per day, empty days included.** On a day nothing was
//! worn a metric answers with its fields empty. That empty answer is the
//! evidence a watch was not worn, so it is landed as served rather than
//! skipped: the operator found no all-day heart rate from 2023-10-08 to
//! 2024-07-16, and it is what takes the Vivoactive 3 out of those rides.
//!
//! **The readings are perishable.** On 2026-10-13 Garmin served two-minute
//! heart rate readings back to somewhere after 2026-05-15 and none before,
//! while every day's resting heart rate went back years; on 2026-10-21 stress,
//! respiration, floors and a night's sleep stages showed the same horizon, the
//! readings present on 2026-06-01 and only the day's summary on 2026-05-01.
//! Landing every day as it stands keeps the readings that are still there.

use std::{sync::Arc, time::Duration};

use application::{EventBatch, SourceError, SourceEvent, WorkoutEventSource};
use domain::landing::{
    Endpoint, EventKind, EventProvenance, EventTime, RawPayload, SourceRecordId, Watermark,
};
use jiff::civil::Date;

use super::{auth::GarminAuth, connect::Connect};

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

/// A metric Garmin serves one day at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GarminDay {
    /// The wrist's heart rate: its resting figure, and its readings.
    HeartRate,
    /// A night's sleep, its stages, and the readings taken through it.
    Sleep,
    /// Stress, with the Body Battery readings beside it.
    Stress,
    /// What the Body Battery charged and drained.
    BodyBattery,
    /// The events that moved the Body Battery: a night, an activity.
    BodyBatteryEvents,
    /// Breathing rate.
    Respiration,
    /// Blood oxygen saturation, as the Pulse Ox sensor read it.
    Spo2,
    /// Moderate and vigorous minutes.
    IntensityMinutes,
    /// Floors climbed and descended.
    Floors,
    /// The day's totals.
    DailySummary,
    /// Steps and activity level, a quarter of an hour at a time.
    Steps,
    /// The walks and other moves the watch detected for itself.
    DetectedMoves,
}

/// How a day is put to an endpoint: in its path, or as a query parameter.
enum Dated {
    InPath,
    Query(&'static [&'static str]),
}

impl GarminDay {
    /// The endpoint, without the date or the account's name: what every
    /// record from it is tagged with.
    pub const fn endpoint(self) -> &'static str {
        match self {
            Self::HeartRate => "/wellness-service/wellness/dailyHeartRate",
            Self::Sleep => "/wellness-service/wellness/dailySleepData",
            Self::Stress => "/wellness-service/wellness/dailyStress",
            Self::BodyBattery => "/wellness-service/wellness/bodyBattery/reports/daily",
            Self::BodyBatteryEvents => "/wellness-service/wellness/bodyBattery/events",
            Self::Respiration => "/wellness-service/wellness/daily/respiration",
            Self::Spo2 => "/wellness-service/wellness/daily/spo2",
            Self::IntensityMinutes => "/wellness-service/wellness/daily/im",
            Self::Floors => "/wellness-service/wellness/floorsChartData/daily",
            Self::DailySummary => "/usersummary-service/usersummary/daily",
            Self::Steps => "/wellness-service/wellness/dailySummaryChart",
            Self::DetectedMoves => "/wellness-service/wellness/dailyEvents",
        }
    }

    /// Whether the account's name follows the endpoint in the path.
    const fn named(self) -> bool {
        matches!(self, Self::Sleep | Self::DailySummary | Self::Steps)
    }

    const fn dated(self) -> Dated {
        match self {
            Self::HeartRate | Self::Sleep | Self::Steps => Dated::Query(&["date"]),
            Self::BodyBattery => Dated::Query(&["startDate", "endDate"]),
            Self::DailySummary | Self::DetectedMoves => Dated::Query(&["calendarDate"]),
            Self::Stress
            | Self::BodyBatteryEvents
            | Self::Respiration
            | Self::Spo2
            | Self::IntensityMinutes
            | Self::Floors => Dated::InPath,
        }
    }

    /// The path and query that ask for `date`, for the account named `name`.
    fn request(self, date: Date, name: Option<&str>) -> (String, Vec<(&'static str, String)>) {
        let mut path = self.endpoint().to_owned();
        if let Some(name) = name {
            path.push('/');
            path.push_str(name);
        }
        let mut query = Vec::new();
        match self.dated() {
            Dated::InPath => {
                path.push('/');
                path.push_str(&date.to_string());
            }
            Dated::Query(keys) => {
                query.extend(keys.iter().map(|key| (*key, date.to_string())));
            }
        }
        // **Garmin's own default for the web view**, which counts the hour
        // either side of the night as the night's: without it a night that
        // began before the window is served cut short.
        if self == Self::Sleep {
            query.push(("nonSleepBufferMinutes", "60".to_owned()));
        }
        (path, query)
    }
}

/// Where a walk has got to: the next day to ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayWalk {
    next: Date,
}

/// One of Garmin's daily metrics, a day at a time.
#[derive(Debug)]
pub struct GarminDays {
    day: GarminDay,
    connect: Connect,
    /// What "today" means to this walk, injected so a test does not depend on
    /// the day it runs.
    today: Date,
}

impl GarminDays {
    pub fn new(
        day: GarminDay,
        api_base: impl Into<String>,
        auth: impl Into<Arc<GarminAuth>>,
        today: Date,
    ) -> Self {
        Self {
            day,
            connect: Connect::new(api_base, auth),
            today,
        }
    }

    /// One day, as served.
    async fn day(&self, date: Date, endpoint: &Endpoint) -> Result<SourceEvent, SourceError> {
        let name = if self.day.named() {
            Some(self.connect.name().await?)
        } else {
            None
        };
        let (path, query) = self.day.request(date, name.as_deref());
        let bytes = self.connect.get(&path, &query).await?;

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

impl WorkoutEventSource for GarminDays {
    type Resume = DayWalk;

    async fn fetch(
        &self,
        since: Option<Watermark>,
        resume: Option<DayWalk>,
    ) -> Result<EventBatch<DayWalk>, SourceError> {
        let mut date = resume.map_or_else(|| start_from(since), |walk| walk.next);
        let endpoint =
            Endpoint::try_from(self.day.endpoint()).map_err(|error| SourceError::Malformed {
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
    use super::{EARLIEST, GarminDay, start_from};
    use domain::landing::{Endpoint, EventTime, Watermark};
    use jiff::civil::date;

    const EVERY: [GarminDay; 12] = [
        GarminDay::HeartRate,
        GarminDay::Sleep,
        GarminDay::Stress,
        GarminDay::BodyBattery,
        GarminDay::BodyBatteryEvents,
        GarminDay::Respiration,
        GarminDay::Spo2,
        GarminDay::IntensityMinutes,
        GarminDay::Floors,
        GarminDay::DailySummary,
        GarminDay::Steps,
        GarminDay::DetectedMoves,
    ];

    #[test]
    fn every_path_is_an_endpoint() {
        for day in EVERY {
            assert_eq!(
                Endpoint::try_from(day.endpoint())
                    .expect("an endpoint")
                    .as_str(),
                day.endpoint()
            );
        }
    }

    /// The requests a live run composes, which no stub can catch: the contract
    /// tests answer whatever path they are asked.
    #[test]
    fn each_metric_asks_for_the_day_where_garmin_expects_it() {
        let day = date(2026, 10, 19);
        let asked = |metric: GarminDay| {
            let name = metric.named().then_some("someone");
            let (path, query) = metric.request(day, name);
            let query: Vec<String> = query
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect();
            format!("{path}?{}", query.join("&"))
        };
        assert_eq!(
            asked(GarminDay::HeartRate),
            "/wellness-service/wellness/dailyHeartRate?date=2026-10-19"
        );
        assert_eq!(
            asked(GarminDay::Sleep),
            "/wellness-service/wellness/dailySleepData/someone?date=2026-10-19&nonSleepBufferMinutes=60"
        );
        assert_eq!(
            asked(GarminDay::Stress),
            "/wellness-service/wellness/dailyStress/2026-10-19?"
        );
        assert_eq!(
            asked(GarminDay::BodyBattery),
            "/wellness-service/wellness/bodyBattery/reports/daily?startDate=2026-10-19&endDate=2026-10-19"
        );
        assert_eq!(
            asked(GarminDay::DailySummary),
            "/usersummary-service/usersummary/daily/someone?calendarDate=2026-10-19"
        );
        assert_eq!(
            asked(GarminDay::Steps),
            "/wellness-service/wellness/dailySummaryChart/someone?date=2026-10-19"
        );
        assert_eq!(
            asked(GarminDay::DetectedMoves),
            "/wellness-service/wellness/dailyEvents?calendarDate=2026-10-19"
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
