//! The HTTP adapter for Garmin's activity list.
//!
//! **Every activity, and the type is a field rather than an endpoint.** Garmin
//! serves one list from `activitylist-service/activities/search/activities`,
//! and what kind of thing an activity was — a ride, a run, a gym session — is
//! stated on the record. "Gym & Fitness Equipment" and "Strength Training" are
//! a parent and a child in that taxonomy, not two services, so a walk scoped to
//! the gym ones would be this adapter cutting a source along a category the
//! source does not have (§ II.3). The operator settled that, 2026-09-18: the
//! stream is the list, and separating disciplines out of it is normalisation's
//! job.
//!
//! **A list, not a change feed**, which makes this a copy of
//! [`crate::peloton::workouts`] rather than of its sibling [`super::hrv`]. There
//! is no `since` parameter and no event kind: the endpoint answers newest first,
//! offset-paginated, and an activity that is deleted simply stops appearing. So
//! the watermark is applied here — the walk stops after the page that crosses
//! it, and the rest of that page is still landed, because the source served it
//! and a partial page is not ours to trim.
//!
//! **No floor date, which is the one thing an offset walk is better at.** The
//! HRV walk steps a day at a time and needs a constant saying where the record
//! begins; this one ends when the list does, so the operator's gym sessions
//! from 2015 need nothing declared to be reached.
//!
//! **What an activity's payload holds is not yet known**, and deliberately so.
//! Whether a gym activity carries its exercise sets inline or behind a second
//! per-activity endpoint — and whether a set the watch *guessed* from wrist
//! movement is marked as one — is settled by reading payloads that have landed,
//! the way the HRV entity was. If they are a second endpoint, that is a second
//! walk behind this same entry, as Peloton's graphs are behind its rides.
//! They are: the list summarises a gym session per movement rather than per
//! set, and [`super::exercise_sets`] is that second walk (#173).

use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use application::{EventBatch, SourceError, SourceEvent, WorkoutEventSource};
use domain::landing::{
    Endpoint, EventKind, EventProvenance, EventTime, PayloadDigest, RawPayload, SourceRecordId,
    Watermark,
};
use jiff::Timestamp;
use serde::Deserialize;
use serde_json::value::RawValue;

use super::auth::GarminAuth;

/// Where the list is served from.
const ACTIVITIES_PATH: &str = "/activitylist-service/activities/search/activities";

/// How many activities one request asks for.
///
/// A walk rather than a page an operator reads, so the only thing a smaller
/// number buys is more round trips against a source that rate-limits. Matched
/// to [`crate::peloton::workouts`]'s for want of a documented maximum: Garmin
/// publishes no specification for this API.
///
/// **Asked for, not relied on.** A source free to cap this is a source that can
/// serve fewer than it was asked for, so nothing downstream may treat it as the
/// size of a page — see [`ActivityPage::advanced_by`].
const PAGE_SIZE: u32 = 50;

/// How long to wait before asking for the next page.
///
/// Garmin rate-limits with 429s and a decade of activities is a great many
/// pages in a row. Politeness rather than a documented requirement, as on
/// [`super::hrv`].
const BETWEEN_PAGES: Duration = Duration::from_millis(250);

/// How many times a request the server fails is asked, in all.
const ATTEMPTS: u32 = 4;

/// How long to wait before the first retry. Doubled for each one after.
const FIRST_RETRY_AFTER: Duration = Duration::from_secs(2);

/// Where a walk has got to: the offset of the next page.
///
/// **An offset rather than a page number**, because that is what this source
/// counts in. Hevy's is a one-based page and Peloton's a zero-based one, which
/// is why each adapter carries its own resume type instead of sharing a
/// `PageNumber`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivityPage {
    start: u32,
}

impl ActivityPage {
    pub const fn start(&self) -> u32 {
        self.start
    }

    /// The offset after a page that served `served` activities.
    ///
    /// **By what came back, not by what was asked for.** Advancing by
    /// [`PAGE_SIZE`] would be this adapter asserting that Garmin honoured the
    /// limit it was given; a source that quietly caps it lower would then have
    /// every record between the two skipped — a walk that looks like it
    /// succeeded and is missing most of the account. No documentation says what
    /// the cap is, so nothing here depends on there not being one.
    fn advanced_by(self, served: usize) -> Self {
        let served = u32::try_from(served).unwrap_or(PAGE_SIZE);
        Self {
            start: self.start.saturating_add(served),
        }
    }
}

/// What a landing record needs from an activity. Everything else stays in the
/// payload, unparsed.
#[derive(Debug, Deserialize)]
struct Identity {
    /// Garmin's own identifier, and a number rather than a string.
    #[serde(rename = "activityId")]
    activity_id: i64,
    /// Unix milliseconds. Present on the list's records; carried as an option
    /// because an activity without one contributes nothing to the watermark
    /// rather than borrowing the clock.
    #[serde(default, rename = "beginTimestamp")]
    begin_timestamp: Option<i64>,
    /// The same instant as a string, `YYYY-MM-DD HH:MM:SS`, stated in UTC
    /// despite the space. Read only when the millisecond form is absent, so
    /// that a shape this adapter has not seen still lands with an event time.
    #[serde(default, rename = "startTimeGMT")]
    start_time_gmt: Option<String>,
}

impl Identity {
    /// When the activity began, by whichever of the two the record states.
    fn occurred_at(&self) -> Option<EventTime> {
        self.begin_timestamp
            .and_then(|millis| Timestamp::from_millisecond(millis).ok())
            .or_else(|| {
                self.start_time_gmt
                    .as_deref()
                    .and_then(parse_garmin_datetime)
            })
            .map(EventTime::from)
    }
}

/// What the next serving of this activity is compared against.
///
/// **Garmin does not state its keys in a stable order.** Two extractions minutes
/// apart landed all fifty activities of the first page again, and the payloads
/// were identical in every value: the second serving of one unchanged activity
/// simply ordered its keys differently — diverging at the seventy-third — so the
/// bytes hashed differently and every one of them read as changed. A walk that
/// re-lands its whole first page on every run makes "0 records landed" stop
/// meaning "nothing changed", and grows the table without adding a fact.
///
/// So the comparison is the payload re-serialised rather than the bytes as
/// served. **Canonical because `serde_json::Map` is a `BTreeMap`** — this build
/// does not enable `preserve_order` — so keys come out in one order whatever
/// order they arrived in. This is [`crate::peloton::workouts`]'s remedy applied
/// to a different disease: there, fields belonging to strangers had to be
/// removed; here nothing is removed and only the ordering is settled.
///
/// The bytes are still landed exactly as served (§ II.1). This changes what
/// "changed" means, not what is stored.
///
/// Falls back to the payload's own digest when the bytes will not parse or will
/// not re-serialise, which is the safe direction: an unreadable payload compares
/// on everything and so lands again rather than being wrongly judged unchanged.
pub(super) fn revision_of(payload: &RawPayload) -> PayloadDigest {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(payload.as_bytes()) else {
        return payload.digest();
    };

    serde_json::to_vec(&value)
        .ok()
        .and_then(|bytes| RawPayload::try_from(bytes.as_slice()).ok())
        .map_or_else(|| payload.digest(), |stable| stable.digest())
}

/// `2026-09-18 07:21:02` as an instant.
///
/// Garmin separates the date and the time with a space and states no offset;
/// the `GMT` in the field's name is the offset. Parsed as a civil datetime and
/// placed in UTC, which is what the name says it is.
fn parse_garmin_datetime(stated: &str) -> Option<Timestamp> {
    stated
        .replacen(' ', "T", 1)
        .parse::<jiff::civil::DateTime>()
        .ok()
        .and_then(|civil| civil.to_zoned(jiff::tz::TimeZone::UTC).ok())
        .map(|zoned| zoned.timestamp())
}

/// Garmin's activities.
#[derive(Debug)]
pub struct GarminActivities {
    /// Shared with the other walks behind this entry, so that one run signs in
    /// once: Garmin rate-limits its sign-in page, and three walks each signing
    /// in for themselves is what first ran into it.
    auth: Arc<GarminAuth>,
    api_base: String,
    client: OnceLock<Result<reqwest::Client, String>>,
}

impl GarminActivities {
    pub fn new(api_base: impl Into<String>, auth: impl Into<Arc<GarminAuth>>) -> Self {
        Self {
            auth: auth.into(),
            api_base: api_base.into().trim_end_matches('/').to_owned(),
            client: OnceLock::new(),
        }
    }

    /// The client, built once on first use: building it initialises TLS, which
    /// can fail, and a constructor is the wrong place for that.
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

    /// One page of the list, as served.
    async fn page(&self, start: u32) -> Result<String, SourceError> {
        self.get(
            ACTIVITIES_PATH,
            &[
                ("start", start.to_string()),
                ("limit", PAGE_SIZE.to_string()),
            ],
        )
        .await
    }

    /// One answer from the API, as served.
    ///
    /// Visible to the Garmin adapter so [`super::exercise_sets`] asks through
    /// the same client, credential and error mapping rather than a second copy
    /// of them, as Peloton's graphs do through its workout list.
    pub(super) async fn get(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<String, SourceError> {
        super::answer(self.send(path, query).await?, path).await
    }

    /// One answer from the API, as the bytes served — for [`super::files`],
    /// whose answer is an archive rather than text.
    pub(super) async fn get_bytes(&self, path: &str) -> Result<Vec<u8>, SourceError> {
        super::answer_bytes(self.send(path, &[]).await?, path).await
    }

    /// One request, asked again if the server fails it.
    ///
    /// **A 5xx is retried; nothing else is.** A walk of every activity's file
    /// is two thousand requests and half an hour, and the first run against the
    /// operator's account died nineteen minutes in on a single 504 — with no
    /// resumption point, because one is only recorded when a run succeeds, so
    /// the next run starts again from the top. A gateway timeout says nothing
    /// about the request, so asking again is the whole remedy. A 429 is not
    /// retried: that is the source asking us to stop, and [`super::answer`]
    /// says so.
    async fn send(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<reqwest::Response, SourceError> {
        let mut wait = FIRST_RETRY_AFTER;
        for _ in 1..ATTEMPTS {
            let response = self.send_once(path, query).await?;
            if !response.status().is_server_error() {
                return Ok(response);
            }
            tokio::time::sleep(wait).await;
            wait = wait.saturating_mul(2);
        }
        self.send_once(path, query).await
    }

    async fn send_once(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<reqwest::Response, SourceError> {
        let bearer = self.auth.bearer().await?;
        self.client()?
            .get(format!("{}{path}", self.api_base))
            .bearer_auth(bearer)
            .query(query)
            .send()
            .await
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })
    }
}

impl WorkoutEventSource for GarminActivities {
    type Resume = ActivityPage;

    async fn fetch(
        &self,
        since: Option<Watermark>,
        resume: Option<ActivityPage>,
    ) -> Result<EventBatch<ActivityPage>, SourceError> {
        let position = resume.unwrap_or(ActivityPage { start: 0 });
        if resume.is_some() {
            tokio::time::sleep(BETWEEN_PAGES).await;
        }

        let endpoint =
            Endpoint::try_from(ACTIVITIES_PATH).map_err(|error| SourceError::Malformed {
                detail: error.to_string(),
            })?;

        let body = self.page(position.start).await?;
        // **A bare array, not an envelope.** The source states no total and no
        // "there is more", so the walk reads a full page as a reason to ask
        // again — one extra empty request at the end of the list, which is the
        // price of not inventing a count the source never gave.
        let activities: Vec<&RawValue> =
            serde_json::from_str(&body).map_err(|error| SourceError::Malformed {
                detail: format!("{ACTIVITIES_PATH} answered something unreadable: {error}"),
            })?;

        // **The walk ends on an empty page, not on a short one.** The source
        // states no total and no "there is more", and may serve fewer than it
        // was asked for, so a short page says nothing about whether the list is
        // exhausted. Asking again costs one empty request at the end of the
        // walk; reading a short page as the end would silently stop partway
        // through a decade of activities.
        let served = activities.len();
        let mut events = Vec::with_capacity(served);
        // Set where this page crosses the resumption point. The rest of the
        // page still lands; the walk stops after it.
        let mut reached_watermark = false;

        for raw in activities {
            let identity: Identity =
                serde_json::from_str(raw.get()).map_err(|error| SourceError::Malformed {
                    detail: format!(
                        "an activity in {ACTIVITIES_PATH} has no readable identity: {error}"
                    ),
                })?;

            let occurred_at = identity.occurred_at();
            if let (Some(at), Some(mark)) = (occurred_at, since)
                && at.as_timestamp() < mark.as_timestamp()
            {
                reached_watermark = true;
            }

            let source_record_id = SourceRecordId::try_from(identity.activity_id.to_string())
                .map_err(|error| SourceError::Malformed {
                    detail: error.to_string(),
                })?;
            let payload = RawPayload::try_from(raw.get().as_bytes()).map_err(|error| {
                SourceError::Malformed {
                    detail: error.to_string(),
                }
            })?;

            events.push(SourceEvent {
                source_record_id,
                // **Always `Updated`.** This source has no event kinds: it
                // serves an activity as it currently stands, and a deletion is
                // an absence rather than an event.
                provenance: EventProvenance::new(endpoint.clone(), EventKind::Updated, occurred_at)
                    .into(),
                revision: revision_of(&payload),
                payload,
            });
        }

        let resume = (served > 0 && !reached_watermark).then(|| position.advanced_by(served));

        Ok(EventBatch { events, resume })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ACTIVITIES_PATH, ActivityPage, Identity, PAGE_SIZE, RawPayload, parse_garmin_datetime,
        revision_of,
    };
    use domain::landing::Endpoint;

    /// **The live failure this exists for.** A first run landed 2,271
    /// activities; a second, minutes later, landed the first fifty again with
    /// every value identical and only the key order changed. Comparing the
    /// bytes as served makes an unchanged activity look new on every run.
    #[test]
    fn two_key_orders_of_one_activity_are_one_revision() {
        let served = RawPayload::try_from(
            br#"{"activityId":123,"duration":42.0,"activityName":"Strength"}"#.as_slice(),
        )
        .expect("a payload");
        let served_again = RawPayload::try_from(
            br#"{"activityName":"Strength","activityId":123,"duration":42.0}"#.as_slice(),
        )
        .expect("a payload");

        assert_ne!(
            served.digest(),
            served_again.digest(),
            "the bytes differ, which is what made this necessary"
        );
        assert_eq!(revision_of(&served), revision_of(&served_again));
    }

    /// A value that really changed is still a new serving.
    #[test]
    fn a_changed_value_is_a_new_revision() {
        let before = RawPayload::try_from(br#"{"activityId":123,"duration":42.0}"#.as_slice())
            .expect("a payload");
        let after = RawPayload::try_from(br#"{"activityId":123,"duration":43.0}"#.as_slice())
            .expect("a payload");

        assert_ne!(revision_of(&before), revision_of(&after));
    }

    /// A payload that will not parse compares on everything, so it lands again
    /// rather than being wrongly judged unchanged.
    #[test]
    fn an_unreadable_payload_falls_back_to_its_own_digest() {
        let payload = RawPayload::try_from(b"not json at all".as_slice()).expect("a payload");
        assert_eq!(revision_of(&payload), payload.digest());
    }

    #[test]
    fn the_path_is_an_endpoint() {
        assert_eq!(
            Endpoint::try_from(ACTIVITIES_PATH)
                .expect("an endpoint")
                .as_str(),
            "/activitylist-service/activities/search/activities"
        );
    }

    /// **The base URL carries no path segment**, so composing it with this
    /// cannot produce a doubled prefix. A stub cannot catch a wrong default.
    #[test]
    fn the_path_composes_against_a_bare_root() {
        assert!(ACTIVITIES_PATH.starts_with('/'));
        assert!(!ACTIVITIES_PATH.contains("//"));
    }

    /// Pages advance by what was served, because the source counts in offsets.
    #[test]
    fn a_page_advances_by_what_was_served() {
        let first = ActivityPage { start: 0 };
        assert_eq!(first.advanced_by(PAGE_SIZE as usize).start(), PAGE_SIZE);
        assert_eq!(
            first
                .advanced_by(PAGE_SIZE as usize)
                .advanced_by(PAGE_SIZE as usize)
                .start(),
            PAGE_SIZE * 2
        );
    }

    /// **A capped page does not skip the records it did not serve.** Garmin is
    /// free to answer with fewer than it was asked for, and advancing by the
    /// limit would step over the difference.
    #[test]
    fn a_short_page_advances_only_by_what_it_held() {
        let first = ActivityPage { start: 0 };
        assert_eq!(first.advanced_by(20).start(), 20);
        assert_eq!(first.advanced_by(20).advanced_by(20).start(), 40);
    }

    /// The millisecond form is preferred, being unambiguous.
    #[test]
    fn an_activity_is_placed_by_its_begin_timestamp() {
        let identity: Identity = serde_json::from_str(
            r#"{"activityId":123,"beginTimestamp":1758179262000,"startTimeGMT":"2000-01-01 00:00:00"}"#,
        )
        .expect("an identity");
        let at = identity.occurred_at().expect("an event time");
        assert_eq!(at.as_timestamp().as_millisecond(), 1_758_179_262_000);
    }

    /// A record stating only the string form still lands with an event time,
    /// and the `GMT` in the field's name is the offset.
    #[test]
    fn the_stated_string_is_read_as_utc() {
        let identity: Identity =
            serde_json::from_str(r#"{"activityId":123,"startTimeGMT":"2026-09-18 07:21:02"}"#)
                .expect("an identity");
        let at = identity.occurred_at().expect("an event time");
        assert_eq!(at.as_timestamp().to_string(), "2026-09-18T07:21:02Z");
    }

    /// An activity stating neither contributes nothing to the watermark rather
    /// than borrowing the clock.
    #[test]
    fn an_activity_with_no_stated_start_has_no_event_time() {
        let identity: Identity =
            serde_json::from_str(r#"{"activityId":123}"#).expect("an identity");
        assert!(identity.occurred_at().is_none());
    }

    #[test]
    fn a_time_this_clock_cannot_place_is_not_one() {
        assert!(parse_garmin_datetime("not a time").is_none());
        assert!(parse_garmin_datetime("2026-13-01 00:00:00").is_none());
    }
}
