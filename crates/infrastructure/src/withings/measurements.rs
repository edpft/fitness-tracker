//! The HTTP adapter for Withings' measurements.
//!
//! **One landing record per measure group.** `getmeas` answers with pages of
//! `measuregrps`, and a group is what one weigh-in produced: an id, the times it
//! was taken and last changed, the device, and a list of measures each tagged
//! with a numeric type. A group is landed exactly as served, as a slice of the
//! page; nothing here reads a measure.
//!
//! **Every type, and only real measurements.** No `meastype` is named, so
//! Withings serves whatever a group holds — which is how the payloads get to say
//! what a Body Scan actually reports. `category=1` excludes the operator's
//! objectives, which are targets rather than readings.
//!
//! **The watermark is the source's own `lastupdate`.** Withings serves groups
//! created *or modified* after it, so an edited weigh-in comes back. The group's
//! later of `created` and `modified` is its event time, and the watermark is
//! passed back one second early so a sibling sharing the boundary second is
//! served again rather than stepped over — the digest comparison discards it.
//!
//! **Deletions are not seen.** Nothing in `getmeas` reports a group that has
//! gone, which is the same position Peloton's list is in.

use std::{sync::OnceLock, time::Duration};

use application::{EventBatch, SourceError, SourceEvent, WorkoutEventSource};
use domain::landing::{
    Endpoint, EventKind, EventProvenance, EventTime, RawPayload, SourceRecordId, Watermark,
};
use serde::Deserialize;
use serde_json::value::RawValue;

use super::auth::WithingsAuth;

/// Where measurements are served from.
const MEASURE_PATH: &str = "/measure";

/// Real measurements, as opposed to the operator's objectives.
const REAL_MEASUREMENTS: &str = "1";

/// Where a walk has got to: the offset Withings handed back, and the
/// `lastupdate` the walk began with, which every page of it must repeat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasurementPage {
    offset: u64,
    last_update: i64,
}

impl MeasurementPage {
    pub const fn offset(&self) -> u64 {
        self.offset
    }
}

/// The page, with the groups left unparsed.
#[derive(Debug, Deserialize)]
struct Page<'a> {
    #[serde(borrow, default)]
    measuregrps: Vec<&'a RawValue>,
    /// `0` or `1` in the documentation's examples; read as either.
    #[serde(default)]
    more: serde_json::Value,
    #[serde(default)]
    offset: Option<u64>,
}

/// What a landing record needs from a group. Everything else stays in the
/// payload.
#[derive(Debug, Deserialize)]
struct Identity {
    grpid: i64,
    created: Option<i64>,
    modified: Option<i64>,
}

/// Withings' measurements.
#[derive(Debug)]
pub struct WithingsMeasurements {
    auth: WithingsAuth,
    api_base: String,
    client: OnceLock<Result<reqwest::Client, String>>,
}

impl WithingsMeasurements {
    pub fn new(api_base: impl Into<String>, auth: WithingsAuth) -> Self {
        Self {
            auth,
            api_base: api_base.into().trim_end_matches('/').to_owned(),
            client: OnceLock::new(),
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
}

/// Where a walk that has seen nothing, or everything up to `since`, begins.
fn last_update_for(since: Option<Watermark>) -> i64 {
    since.map_or(0, |mark| {
        mark.as_timestamp().as_second().saturating_sub(1).max(0)
    })
}

/// Whether the page says there is another.
fn is_more(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Bool(more) => *more,
        serde_json::Value::Number(more) => more.as_i64().is_some_and(|more| more != 0),
        _ => false,
    }
}

impl WorkoutEventSource for WithingsMeasurements {
    type Resume = MeasurementPage;

    async fn fetch(
        &self,
        since: Option<Watermark>,
        resume: Option<MeasurementPage>,
    ) -> Result<EventBatch<MeasurementPage>, SourceError> {
        let position = resume.unwrap_or_else(|| MeasurementPage {
            offset: 0,
            last_update: last_update_for(since),
        });

        let mut form = vec![
            ("action", "getmeas".to_owned()),
            ("category", REAL_MEASUREMENTS.to_owned()),
            ("lastupdate", position.last_update.to_string()),
        ];
        if position.offset > 0 {
            form.push(("offset", position.offset.to_string()));
        }

        let bearer = self.auth.bearer().await?;
        let response = self
            .client()?
            .post(format!("{}{MEASURE_PATH}", self.api_base))
            .bearer_auth(bearer)
            .form(&form)
            .send()
            .await
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })?;
        let text = super::answer(response, MEASURE_PATH).await?;
        let page: Page<'_> = serde_json::from_str(super::body_of(&text, MEASURE_PATH)?.get())
            .map_err(|error| SourceError::Malformed {
                detail: format!("{MEASURE_PATH} answered something unreadable: {error}"),
            })?;

        let endpoint =
            Endpoint::try_from(MEASURE_PATH).map_err(|error| SourceError::Malformed {
                detail: error.to_string(),
            })?;

        let mut events = Vec::with_capacity(page.measuregrps.len());
        for raw in page.measuregrps {
            let identity: Identity =
                serde_json::from_str(raw.get()).map_err(|error| SourceError::Malformed {
                    detail: format!("a measure group has no readable identity: {error}"),
                })?;

            let occurred_at = identity
                .created
                .into_iter()
                .chain(identity.modified)
                .max()
                .and_then(|second| jiff::Timestamp::from_second(second).ok())
                .map(EventTime::from);

            let source_record_id =
                SourceRecordId::try_from(identity.grpid.to_string()).map_err(|error| {
                    SourceError::Malformed {
                        detail: error.to_string(),
                    }
                })?;
            let payload = RawPayload::try_from(raw.get().as_bytes()).map_err(|error| {
                SourceError::Malformed {
                    detail: error.to_string(),
                }
            })?;

            events.push(SourceEvent::new(
                source_record_id,
                // **Always `Updated`**: `getmeas` serves a group as it now
                // stands and has no event kinds.
                EventProvenance::new(endpoint.clone(), EventKind::Updated, occurred_at).into(),
                payload,
            ));
        }

        let resume = match (is_more(&page.more), page.offset) {
            (true, Some(offset)) if offset > position.offset => Some(MeasurementPage {
                offset,
                last_update: position.last_update,
            }),
            (true, _) => {
                return Err(SourceError::Malformed {
                    detail: format!(
                        "{MEASURE_PATH} said there was more without an offset past {}",
                        position.offset
                    ),
                });
            }
            (false, _) => None,
        };

        Ok(EventBatch { events, resume })
    }
}

#[cfg(test)]
mod tests {
    use super::{MEASURE_PATH, is_more, last_update_for};
    use domain::landing::{Endpoint, EventTime, Watermark};

    #[test]
    fn the_path_is_an_endpoint() {
        assert_eq!(
            Endpoint::try_from(MEASURE_PATH)
                .expect("an endpoint")
                .as_str(),
            "/measure"
        );
    }

    /// A first walk asks for everything, and a later one repeats the boundary
    /// second rather than stepping over it.
    #[test]
    fn the_watermark_is_passed_back_one_second_early() {
        assert_eq!(last_update_for(None), 0);
        let at = jiff::Timestamp::from_second(1_700_000_000).expect("a time");
        let mark = Watermark::from(EventTime::from(at));
        assert_eq!(last_update_for(Some(mark)), 1_699_999_999);
    }

    #[test]
    fn more_is_read_as_a_number_or_a_flag() {
        assert!(is_more(&serde_json::json!(1)));
        assert!(is_more(&serde_json::json!(true)));
        assert!(!is_more(&serde_json::json!(0)));
        assert!(!is_more(&serde_json::json!(false)));
        assert!(!is_more(&serde_json::Value::Null));
    }
}
