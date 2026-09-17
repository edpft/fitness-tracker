//! The HTTP adapter for Garmin's overnight HRV.
//!
//! **One landing record per night.** `hrv-service/hrv/{date}` answers with that
//! night's `hrvSummary` — the averages, the status, the feedback phrase and the
//! personal baseline — together with `hrvReadings`, the individual readings the
//! watch took while the operator slept. The whole answer is landed as served;
//! nothing here reads a measurement.
//!
//! **The walk is by date, because the source is.** There is no cursor and no
//! `since` parameter: a night is addressed by its calendar date, so the walk
//! steps forward a day at a time from wherever the watermark left it. A night's
//! event time is its calendar date at midnight UTC, which makes the watermark
//! mean "walked up to here" and keeps it strictly increasing.
//!
//! **A night with no HRV lands nothing**, and that is the one rough edge. The
//! resumption point advances only to event times a run actually saw — never to
//! the clock — so a stretch with no data, a holiday without the watch, is walked
//! again by the next run. Each of those days is one cheap answer, and the
//! alternative is landing empty records to move a pointer, which would be
//! inventing observations.
//!
//! **The range endpoint is not used yet.** `hrv-service/hrv/daily/{start}/{end}`
//! exists and would collapse the backfill into a few dozen requests, but what it
//! answers with — whether it carries `hrvReadings` or only the summaries — is
//! not documented anywhere reachable, and a walk built on a guess would land
//! whatever it happened to return. It is the first thing a live run should
//! establish.

use std::{sync::OnceLock, time::Duration};

use application::{EventBatch, SourceError, SourceEvent, WorkoutEventSource};
use domain::landing::{
    Endpoint, EventKind, EventProvenance, EventTime, RawPayload, SourceRecordId, Watermark,
};
use jiff::civil::Date;
use serde::Deserialize;

use super::auth::GarminAuth;

/// Where a night's HRV is served from, before the date is appended.
const HRV_PATH: &str = "/hrv-service/hrv";

/// The earliest night worth asking for.
///
/// **The operator's device history, not a property of the API.** HRV Status
/// arrived with Garmin's May 2022 firmware and needs a watch that supports it;
/// the operator's first is a Forerunner 165 he has had since at least
/// 2024-12-20, and the Vivoactive 3 before it reported the older all-day stress
/// metric, which § 6 makes a different series rather than earlier HRV. Garmin
/// also needs three weeks of consistent sleep before it states a status at all,
/// so the earliest nights here carry readings and no status.
///
/// Lowering this is a one-line change if earlier data ever turns out to exist.
const EARLIEST: Date = jiff::civil::date(2024, 12, 20);

/// How many nights one batch asks for.
///
/// Each night is its own request, so this is the unit of work between commits
/// rather than a page size. A week keeps a rate-limited backfill from losing
/// much, and keeps the transaction count sane across the six hundred-odd
/// nights of the first run.
const NIGHTS_PER_BATCH: usize = 7;

/// How long to wait between a batch's requests.
///
/// Garmin rate-limits sign-in and the API with 429s, and the first run makes
/// several hundred calls in a row. This is politeness rather than a documented
/// requirement.
const BETWEEN_REQUESTS: Duration = Duration::from_millis(250);

/// Where a walk has got to: the next night to ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HrvWalk {
    next: Date,
}

impl HrvWalk {
    pub const fn next(&self) -> Date {
        self.next
    }
}

/// Garmin's overnight HRV.
#[derive(Debug)]
pub struct GarminHrv {
    auth: GarminAuth,
    api_base: String,
    client: OnceLock<Result<reqwest::Client, String>>,
    /// What "today" means to this walk. Injected so a test is not at the mercy
    /// of the day it runs on.
    today: Date,
}

impl GarminHrv {
    pub fn new(api_base: impl Into<String>, auth: GarminAuth, today: Date) -> Self {
        Self {
            auth,
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

    /// One night, or nothing if Garmin has no HRV for it.
    async fn night(
        &self,
        date: Date,
        endpoint: &Endpoint,
    ) -> Result<Option<SourceEvent>, SourceError> {
        let bearer = self.auth.bearer().await?;
        let response = self
            .client()?
            .get(format!("{}{HRV_PATH}/{date}", self.api_base))
            .bearer_auth(bearer)
            .send()
            .await
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })?;

        let status = response.status();
        // **A night Garmin has nothing for is not a failure**, and which way it
        // says so is not yet known: an unworn night, a date before the account
        // existed and a date before HRV Status shipped are three cases, and no
        // reachable documentation says whether they answer 404, 204, or a 200
        // with an empty body. All of them are read as "no reading" here, and the
        // first live run is what settles which actually occur.
        if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::NO_CONTENT {
            return Ok(None);
        }
        let text = super::answer(response, HRV_PATH).await?;

        // A 200 whose body is `null` or `{}` is the third way of saying it.
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed == "null" || trimmed == "{}" {
            return Ok(None);
        }

        let night: Night =
            serde_json::from_str(trimmed).map_err(|error| SourceError::Malformed {
                detail: format!("{HRV_PATH}/{date} answered something unreadable: {error}"),
            })?;
        if night.hrv_summary.is_none() && night.hrv_readings.is_empty() {
            return Ok(None);
        }

        // **The date asked for, not the one answered.** They agree, but the
        // request is what makes this record addressable and the walk monotonic.
        let source_record_id =
            SourceRecordId::try_from(date.to_string()).map_err(|error| SourceError::Malformed {
                detail: error.to_string(),
            })?;
        let payload =
            RawPayload::try_from(trimmed.as_bytes()).map_err(|error| SourceError::Malformed {
                detail: error.to_string(),
            })?;

        Ok(Some(SourceEvent::new(
            source_record_id,
            EventProvenance::new(
                endpoint.clone(),
                // **Always `Updated`**: the endpoint serves a night as it now
                // stands and has no event kinds. A night re-served after Garmin
                // revises it is a new record, and the digest decides.
                EventKind::Updated,
                Some(EventTime::from(midnight(date)?)),
            )
            .into(),
            payload,
        )))
    }
}

/// Where a walk begins: the night after the watermark, or the earliest there is.
///
/// **The watermark's own night is asked for again.** Garmin revises a night's
/// status as later nights move the baseline, so re-asking costs one request and
/// the digest comparison discards it when nothing changed.
fn start_from(since: Option<Watermark>) -> Date {
    let Some(mark) = since else {
        return EARLIEST;
    };
    let at = mark.as_timestamp().to_zoned(jiff::tz::TimeZone::UTC).date();
    if at < EARLIEST { EARLIEST } else { at }
}

/// A calendar date as an instant, which is what an event time is.
fn midnight(date: Date) -> Result<jiff::Timestamp, SourceError> {
    date.to_zoned(jiff::tz::TimeZone::UTC)
        .map(|zoned| zoned.timestamp())
        .map_err(|error| SourceError::Malformed {
            detail: format!("{date} is not a date this clock can place: {error}"),
        })
}

impl WorkoutEventSource for GarminHrv {
    type Resume = HrvWalk;

    async fn fetch(
        &self,
        since: Option<Watermark>,
        resume: Option<HrvWalk>,
    ) -> Result<EventBatch<HrvWalk>, SourceError> {
        let mut date = resume.map_or_else(|| start_from(since), |walk| walk.next);

        let endpoint = Endpoint::try_from(HRV_PATH).map_err(|error| SourceError::Malformed {
            detail: error.to_string(),
        })?;

        let mut events = Vec::new();
        let mut asked = 0_usize;

        while asked < NIGHTS_PER_BATCH && date <= self.today {
            if asked > 0 {
                tokio::time::sleep(BETWEEN_REQUESTS).await;
            }
            if let Some(event) = self.night(date, &endpoint).await? {
                events.push(event);
            }
            asked = asked.saturating_add(1);
            date = date.tomorrow().map_err(|error| SourceError::Malformed {
                detail: format!("there is no night after {date}: {error}"),
            })?;
        }

        // The walk is finished when it has reached today, not when a batch came
        // back empty: an unworn night says nothing about the one after it.
        let resume = (date <= self.today).then_some(HrvWalk { next: date });

        Ok(EventBatch { events, resume })
    }
}

/// What a landing record needs from a night. Everything else stays in the
/// payload.
#[derive(Debug, Deserialize)]
struct Night {
    #[serde(default, rename = "hrvSummary")]
    hrv_summary: Option<serde::de::IgnoredAny>,
    #[serde(default, rename = "hrvReadings")]
    hrv_readings: Vec<serde::de::IgnoredAny>,
}

#[cfg(test)]
mod tests {
    use super::{EARLIEST, HRV_PATH, HrvWalk, Night, start_from};
    use domain::landing::{Endpoint, EventTime, Watermark};
    use jiff::civil::date;

    #[test]
    fn the_path_is_an_endpoint() {
        assert_eq!(
            Endpoint::try_from(HRV_PATH).expect("an endpoint").as_str(),
            "/hrv-service/hrv"
        );
    }

    /// A first walk starts at the earliest night there could be one.
    #[test]
    fn a_first_walk_starts_at_the_earliest_night() {
        assert_eq!(start_from(None), EARLIEST);
        assert_eq!(EARLIEST, date(2024, 12, 20));
    }

    /// The watermark's own night is re-asked, because Garmin revises a night's
    /// status as later nights move the baseline.
    #[test]
    fn a_later_walk_re_asks_the_watermark_s_night() {
        let at = date(2026, 9, 14)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .expect("a zoned date")
            .timestamp();
        let mark = Watermark::from(EventTime::from(at));
        assert_eq!(start_from(Some(mark)), date(2026, 9, 14));
    }

    /// A watermark from before the floor cannot drag the walk below it.
    #[test]
    fn the_floor_holds_against_an_older_watermark() {
        let at = date(2019, 1, 1)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .expect("a zoned date")
            .timestamp();
        let mark = Watermark::from(EventTime::from(at));
        assert_eq!(start_from(Some(mark)), EARLIEST);
    }

    /// A night with readings and no status is real data, not an absence.
    /// Garmin states no status until three weeks of sleep are behind it, so
    /// the earliest nights of the record look exactly like this.
    #[test]
    fn a_night_with_readings_and_no_status_is_a_night() {
        let night: Night = serde_json::from_str(
            r#"{"hrvReadings":[{"hrvValue":42},{"hrvValue":45}],"startTimestampGMT":"2025-01-02T23:00:00.0"}"#,
        )
        .expect("a night");
        assert!(night.hrv_summary.is_none());
        assert_eq!(night.hrv_readings.len(), 2);
    }

    /// A summary with no readings is also a night: the two are independently
    /// absent, and refusing either would discard what the other holds.
    #[test]
    fn a_summary_without_readings_is_a_night() {
        let night: Night =
            serde_json::from_str(r#"{"hrvSummary":{"lastNightAvg":38,"status":"BALANCED"}}"#)
                .expect("a night");
        assert!(night.hrv_summary.is_some());
        assert!(night.hrv_readings.is_empty());
    }

    #[test]
    fn an_empty_answer_holds_neither() {
        let night: Night = serde_json::from_str(r#"{"userProfilePK":1}"#).expect("a night");
        assert!(night.hrv_summary.is_none());
        assert!(night.hrv_readings.is_empty());
    }

    #[test]
    fn a_walk_names_the_night_it_resumes_at() {
        let walk = HrvWalk {
            next: date(2025, 3, 1),
        };
        assert_eq!(walk.next(), date(2025, 3, 1));
    }
}
