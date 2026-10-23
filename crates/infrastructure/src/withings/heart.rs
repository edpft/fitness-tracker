//! The HTTP adapter for Withings' ECGs (#270).
//!
//! **Everything the Body Scan records beyond `/measure`.** Its weight,
//! composition, standing heart rate, pulse wave velocity and nerve scores are
//! measure groups; its six-lead ECG is not. Heart v2 serves it in two parts:
//! `action=list` pages through the recordings — when, on which device, the
//! heart rate and the classification — and `action=get` serves one recording's
//! signal by the `signalid` the list names.
//!
//! **Two streams, because they are two answers** (§ II.1): an entry of the list
//! as served, and a signal as served. The signals walk enumerates the list
//! itself rather than reading the other table, for the reason
//! [`crate::garmin::files`] gives.
//!
//! **The watermark is the recording's `timestamp`.** The list is filtered by
//! `startdate` and `enddate`, both the time a recording was taken; it has no
//! `lastupdate`, so a recording Withings reclassified after the walk passed it
//! is not served again.
//!
//! **Read under `user.metrics`**, the scope the sign-in already asks for.

use std::{sync::Arc, sync::OnceLock, time::Duration};

use application::{EventBatch, SourceError, WorkoutEventSource};
use domain::landing::{
    Endpoint, EventKind, EventProvenance, EventTime, RawPayload, SourceRecordId, Watermark,
};
use serde::Deserialize;
use serde_json::value::RawValue;

use super::auth::WithingsAuth;

/// Where both parts are served from.
const HEART_PATH: &str = "/v2/heart";

/// How long to wait between one signal and the next.
///
/// Withings allows 120 requests a minute per application; this keeps a long
/// backfill well inside it.
const BETWEEN_REQUESTS: Duration = Duration::from_millis(500);

/// Where a walk of the list has got to: the offset Withings handed back, and
/// the span the walk began with, which every page of it must repeat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EcgPage {
    offset: u64,
    start: i64,
    end: i64,
}

impl EcgPage {
    pub const fn offset(&self) -> u64 {
        self.offset
    }
}

/// The page, with the entries left unparsed.
#[derive(Debug, Deserialize)]
struct Page<'a> {
    #[serde(borrow, default)]
    series: Vec<&'a RawValue>,
    #[serde(default)]
    more: serde_json::Value,
    #[serde(default)]
    offset: Option<u64>,
}

/// What a landing record needs from an entry. Everything else stays in the
/// payload.
#[derive(Debug, Deserialize)]
struct Identity {
    #[serde(default)]
    ecg: Option<Recording>,
    #[serde(default)]
    deviceid: Option<String>,
    timestamp: i64,
}

#[derive(Debug, Deserialize)]
struct Recording {
    signalid: i64,
}

/// The recording an entry names: its `signalid`, or, for an entry with no ECG
/// (a blood pressure reading, from a device the operator does not have), the
/// device and the second it was taken.
fn record_id_of(identity: &Identity) -> String {
    identity.ecg.as_ref().map_or_else(
        || {
            format!(
                "{}@{}",
                identity.deviceid.as_deref().unwrap_or("unknown"),
                identity.timestamp
            )
        },
        |ecg| ecg.signalid.to_string(),
    )
}

/// Where a walk that has seen nothing, or everything up to `since`, begins.
fn start_for(since: Option<Watermark>) -> i64 {
    since.map_or(0, |mark| {
        mark.as_timestamp().as_second().saturating_sub(1).max(0)
    })
}

/// The HTTP half both parts share.
#[derive(Debug)]
struct Heart {
    auth: Arc<WithingsAuth>,
    api_base: String,
    client: OnceLock<Result<reqwest::Client, String>>,
}

impl Heart {
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

    /// The text of one answer from `/v2/heart`.
    async fn post(&self, form: &[(&str, String)]) -> Result<String, SourceError> {
        let bearer = self.auth.bearer().await?;
        let response = self
            .client()?
            .post(format!("{}{HEART_PATH}", self.api_base))
            .bearer_auth(bearer)
            .form(form)
            .send()
            .await
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })?;
        super::answer(response, HEART_PATH).await
    }
}

fn endpoint() -> Result<Endpoint, SourceError> {
    Endpoint::try_from(HEART_PATH).map_err(|error| SourceError::Malformed {
        detail: error.to_string(),
    })
}

fn payload_of(bytes: &str) -> Result<RawPayload, SourceError> {
    RawPayload::try_from(bytes.as_bytes()).map_err(|error| SourceError::Malformed {
        detail: error.to_string(),
    })
}

/// Withings' ECG list.
#[derive(Debug)]
pub struct WithingsEcgs {
    heart: Heart,
}

impl WithingsEcgs {
    pub fn new(api_base: impl Into<String>, auth: impl Into<Arc<WithingsAuth>>) -> Self {
        Self {
            heart: Heart {
                auth: auth.into(),
                api_base: api_base.into().trim_end_matches('/').to_owned(),
                client: OnceLock::new(),
            },
        }
    }
}

impl WorkoutEventSource for WithingsEcgs {
    type Resume = EcgPage;

    async fn fetch(
        &self,
        since: Option<Watermark>,
        resume: Option<EcgPage>,
    ) -> Result<EventBatch<EcgPage>, SourceError> {
        let position = resume.unwrap_or_else(|| EcgPage {
            offset: 0,
            start: start_for(since),
            end: jiff::Timestamp::now().as_second(),
        });

        let mut form = vec![
            ("action", "list".to_owned()),
            ("startdate", position.start.to_string()),
            ("enddate", position.end.to_string()),
        ];
        if position.offset > 0 {
            form.push(("offset", position.offset.to_string()));
        }

        let text = self.heart.post(&form).await?;
        let page: Page<'_> = serde_json::from_str(super::body_of(&text, HEART_PATH)?.get())
            .map_err(|error| SourceError::Malformed {
                detail: format!("{HEART_PATH} answered something unreadable: {error}"),
            })?;
        let endpoint = endpoint()?;

        let mut events = Vec::with_capacity(page.series.len());
        for raw in page.series {
            let identity: Identity =
                serde_json::from_str(raw.get()).map_err(|error| SourceError::Malformed {
                    detail: format!("an ECG has no readable identity: {error}"),
                })?;
            let occurred_at = jiff::Timestamp::from_second(identity.timestamp)
                .ok()
                .map(EventTime::from);
            let source_record_id =
                SourceRecordId::try_from(record_id_of(&identity)).map_err(|error| {
                    SourceError::Malformed {
                        detail: error.to_string(),
                    }
                })?;

            events.push(crate::revision::json_event(
                source_record_id,
                // **Always `Updated`**, as for measure groups: the list serves
                // a recording as it now stands.
                EventProvenance::new(endpoint.clone(), EventKind::Updated, occurred_at).into(),
                payload_of(raw.get())?,
            ));
        }

        let resume = match (super::measurements::is_more(&page.more), page.offset) {
            (true, Some(offset)) if offset > position.offset => Some(EcgPage {
                offset,
                start: position.start,
                end: position.end,
            }),
            (true, _) => {
                return Err(SourceError::Malformed {
                    detail: format!(
                        "{HEART_PATH} said there was more without an offset past {}",
                        position.offset
                    ),
                });
            }
            (false, _) => None,
        };

        Ok(EventBatch { events, resume })
    }
}

/// Withings' ECG signals, one per recording the list names.
#[derive(Debug)]
pub struct WithingsEcgSignals {
    /// The list, which is how a signal is found at all.
    ecgs: WithingsEcgs,
}

impl WithingsEcgSignals {
    pub fn new(api_base: impl Into<String>, auth: impl Into<Arc<WithingsAuth>>) -> Self {
        Self {
            ecgs: WithingsEcgs::new(api_base, auth),
        }
    }
}

impl WorkoutEventSource for WithingsEcgSignals {
    type Resume = EcgPage;

    async fn fetch(
        &self,
        since: Option<Watermark>,
        resume: Option<EcgPage>,
    ) -> Result<EventBatch<EcgPage>, SourceError> {
        let listed = self.ecgs.fetch(since, resume).await?;

        let mut events = Vec::new();
        for entry in listed.events {
            let identity: Identity =
                serde_json::from_slice(entry.payload.as_bytes()).map_err(|error| {
                    SourceError::Malformed {
                        detail: format!("an ECG has no readable identity: {error}"),
                    }
                })?;
            let Some(ecg) = identity.ecg else {
                continue;
            };

            if !events.is_empty() {
                tokio::time::sleep(BETWEEN_REQUESTS).await;
            }

            let text = self
                .ecgs
                .heart
                .post(&[
                    ("action", "get".to_owned()),
                    ("signalid", ecg.signalid.to_string()),
                ])
                .await?;
            let body = super::body_of(&text, HEART_PATH)?;

            // **The recording's time, not the request's**, as Garmin's files
            // take theirs: the resumption point advances on what the list
            // stated.
            events.push(crate::revision::json_event(
                entry.source_record_id,
                EventProvenance::new(
                    endpoint()?,
                    EventKind::Updated,
                    entry.provenance.occurred_at(),
                )
                .into(),
                payload_of(body.get())?,
            ));
        }

        Ok(EventBatch {
            events,
            resume: listed.resume,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{HEART_PATH, Identity, record_id_of, start_for};
    use domain::landing::{Endpoint, EventTime, Watermark};

    #[test]
    fn the_path_is_an_endpoint() {
        assert_eq!(
            Endpoint::try_from(HEART_PATH)
                .expect("an endpoint")
                .as_str(),
            "/v2/heart"
        );
    }

    #[test]
    fn the_watermark_is_passed_back_one_second_early() {
        assert_eq!(start_for(None), 0);
        let at = jiff::Timestamp::from_second(1_700_000_000).expect("a time");
        assert_eq!(
            start_for(Some(Watermark::from(EventTime::from(at)))),
            1_699_999_999
        );
    }

    #[test]
    fn an_ecg_is_known_by_its_signal() {
        let identity: Identity = serde_json::from_str(
            r#"{"deviceid":"abc","model":0,"ecg":{"signalid":42,"afib":0},"heart_rate":61,"timestamp":1757923200}"#,
        )
        .expect("an entry");
        assert_eq!(record_id_of(&identity), "42");
    }

    #[test]
    fn an_entry_without_one_is_known_by_its_device_and_second() {
        let identity: Identity = serde_json::from_str(
            r#"{"deviceid":"abc","bloodpressure":{"diastole":80,"systole":120},"timestamp":1757923200}"#,
        )
        .expect("an entry");
        assert_eq!(record_id_of(&identity), "abc@1757923200");
    }
}
