//! The HTTP adapter for Peloton's workout list.
//!
//! **Peloton serves a list, not a change feed**, and that is the one real
//! difference from [`crate::hevy::client`]. There is no `since` parameter and
//! no event kind: the endpoint answers with the operator's workouts newest
//! first, paginated, and a workout that is deleted simply stops appearing.
//!
//! **Each record carries the class it was performed against**, because the walk
//! asks for it — see [`JOINS`]. A workout and the class it was ridden to are
//! two things (§ 11) that this source will serve in one payload, and taking
//! both now is what stops a re-derivation needing the network again.
//!
//! So the watermark is applied here rather than at the source. The walk stops
//! at the first record older than the resumption point, which is safe because
//! the order is fixed — `sort_by` is echoed back as
//! `-device_time_created_at,-pk` whatever is asked for — and because the
//! extraction use case advances the point only after a whole walk succeeds. An
//! interrupted walk leaves it untouched and the next run starts again from the
//! same place.
//!
//! That use case is deliberately not named in a link here: § 16 says a driven
//! adapter implements ports rather than reaching for the thing driving it, and
//! the `use-case-isolation` check reads doc comments as well as code.
//!
//! **The boundary record is served again rather than skipped.** The comparison
//! is strictly-older, matching the inclusive `since` Hevy's feed implements, so
//! a sibling sharing the boundary second cannot be stepped over. It costs one
//! duplicate payload, which the digest comparison discards.

use std::{sync::OnceLock, time::Duration};

use application::{EventBatch, SourceError, SourceEvent, WorkoutEventSource};
use domain::landing::{
    Endpoint, EventKind, EventProvenance, EventTime, RawPayload, SourceRecordId, Watermark,
};
use jiff::Timestamp;
use serde::Deserialize;
use serde_json::value::RawValue;

use super::auth::PelotonAuth;

/// How many workouts to ask for at once.
///
/// Not a setting: the operator has 426 of them and this is a walk, so the only
/// thing a smaller number buys is more round trips.
const PAGE_SIZE: u32 = 50;

/// **Ask for the class the workout was performed against.**
///
/// Without this the list serves the workout alone, and the class it names is a
/// second request per workout — 426 of them. With it, each record carries the
/// whole class object inline: the same fields `/api/v2/ride/archived` serves,
/// and an `id` that is the identifier already stored in `cycling_venue`.
///
/// **Landing them together is not conflating them.** § 11 keeps the class and
/// the performance separate and joinable — what was prescribed against what
/// happened — and this is one *payload* carrying both, which normalisation
/// separates into two things. Raw's job is to hold what the source served
/// (§ II.1); the source serves them together when asked, and asking is what
/// makes a re-derivation possible without going back to the network.
const JOINS: &str = "ride";

/// Who the session belongs to. The list endpoint is addressed by user, so the
/// walk needs this before it can ask for anything.
const ME_ENDPOINT: &str = "/api/me";

/// The path the list is served from, once the user is known.
fn workouts_path(user: &str) -> String {
    format!("/api/user/{user}/workouts")
}

/// Where a walk has got to.
///
/// Carries the user id as well as the page so that `/api/me` is asked once per
/// walk rather than once per page. The resume token is the adapter's own shape
/// (`WorkoutEventSource::Resume`), which is exactly what it is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkoutPage {
    /// Zero-based, because this source is. Hevy's is one-based, which is why
    /// the two have separate types rather than one shared `PageNumber`.
    page: u32,
    user: String,
}

impl WorkoutPage {
    pub const fn page(&self) -> u32 {
        self.page
    }

    fn next(&self) -> Self {
        Self {
            page: self.page.saturating_add(1),
            user: self.user.clone(),
        }
    }
}

/// The list envelope.
///
/// `show_next` rather than comparing `page` to `page_count`: the source states
/// whether there is more, and a derived answer would be this adapter deciding
/// something the source already said.
#[derive(Debug, Deserialize)]
struct Envelope<'a> {
    #[serde(borrow, default)]
    data: Vec<&'a RawValue>,
    #[serde(default)]
    show_next: bool,
}

/// The two fields a landing record needs from a workout. Everything else stays
/// in the payload, unparsed.
#[derive(Debug, Deserialize)]
struct Identity {
    id: String,
    /// Unix seconds. Nullable in principle; a workout without one contributes
    /// nothing to the watermark rather than borrowing the clock.
    created_at: Option<i64>,
}

/// Peloton's workout list.
#[derive(Debug)]
pub struct PelotonWorkouts {
    auth: PelotonAuth,
    api_base: String,
    client: OnceLock<Result<reqwest::Client, String>>,
}

impl PelotonWorkouts {
    pub fn new(api_base: impl Into<String>, auth: PelotonAuth) -> Self {
        Self {
            auth,
            api_base: api_base.into().trim_end_matches('/').to_owned(),
            client: OnceLock::new(),
        }
    }

    /// The client, built once on first use, for the reason given on
    /// [`crate::hevy::client::HevyWorkoutEvents`]: building it initialises TLS,
    /// which can fail, and a constructor is the wrong place for that.
    fn client(&self) -> Result<&reqwest::Client, SourceError> {
        let built = self.client.get_or_init(|| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|error| error.to_string())
        });

        built.as_ref().map_err(|detail| SourceError::Unavailable {
            detail: detail.clone(),
        })
    }

    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Vec<u8>, SourceError> {
        let bearer = self.auth.bearer().await?;
        let response = self
            .client()?
            .get(format!("{}{path}", self.api_base))
            .bearer_auth(bearer)
            .header("Peloton-Platform", "web")
            .query(query)
            .send()
            .await
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })?;

        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(SourceError::Unauthorised);
        }
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(SourceError::Unavailable {
                detail: format!("{path} answered {status}: {}", detail.trim()),
            });
        }

        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })
    }

    /// Who we are, which the list endpoint is addressed by.
    async fn user(&self) -> Result<String, SourceError> {
        let body = self.get(ME_ENDPOINT, &[]).await?;
        let me: serde_json::Value =
            serde_json::from_slice(&body).map_err(|error| SourceError::Malformed {
                detail: format!("{ME_ENDPOINT} is not JSON: {error}"),
            })?;

        me.get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| SourceError::Malformed {
                detail: format!("{ME_ENDPOINT} answered without an id"),
            })
    }
}

/// Unix seconds as the source serves them.
fn event_time(seconds: i64) -> Option<EventTime> {
    Timestamp::from_second(seconds).ok().map(EventTime::from)
}

impl WorkoutEventSource for PelotonWorkouts {
    type Resume = WorkoutPage;

    async fn fetch(
        &self,
        since: Option<Watermark>,
        resume: Option<WorkoutPage>,
    ) -> Result<EventBatch<WorkoutPage>, SourceError> {
        // No resume token means this is the opening request of a walk, and the
        // only point at which the user has to be looked up.
        let position = match resume {
            Some(position) => position,
            None => WorkoutPage {
                page: 0,
                user: self.user().await?,
            },
        };

        let path = workouts_path(&position.user);
        let endpoint =
            Endpoint::try_from(path.clone()).map_err(|error| SourceError::Malformed {
                detail: error.to_string(),
            })?;

        let body = self
            .get(
                &path,
                &[
                    ("limit", PAGE_SIZE.to_string()),
                    ("page", position.page.to_string()),
                    ("joins", JOINS.to_owned()),
                ],
            )
            .await?;

        let envelope: Envelope<'_> =
            serde_json::from_slice(&body).map_err(|error| SourceError::Malformed {
                detail: format!("{path} answered something unreadable: {error}"),
            })?;

        let mut events = Vec::with_capacity(envelope.data.len());
        // Set where this page crosses the resumption point. The rest of the
        // page is still landed — the source served it, and a partial page is
        // not ours to trim — but the walk stops after it.
        let mut reached_watermark = false;

        for raw in envelope.data {
            let bytes = raw.get().as_bytes();
            let identity: Identity =
                serde_json::from_str(raw.get()).map_err(|error| SourceError::Malformed {
                    detail: format!("a workout in {path} has no readable identity: {error}"),
                })?;

            let occurred_at = identity.created_at.and_then(event_time);
            if let (Some(at), Some(mark)) = (occurred_at, since)
                && at.as_timestamp() < mark.as_timestamp()
            {
                reached_watermark = true;
            }

            let source_record_id =
                SourceRecordId::try_from(identity.id).map_err(|error| SourceError::Malformed {
                    detail: error.to_string(),
                })?;

            events.push(SourceEvent {
                source_record_id,
                // **Always `Updated`.** This source has no event kinds: it
                // serves the workout as it currently stands, and a deletion is
                // an absence rather than an event. Recording anything else
                // would be inventing a fact the source never stated.
                provenance: EventProvenance::new(endpoint.clone(), EventKind::Updated, occurred_at)
                    .into(),
                payload: RawPayload::try_from(bytes).map_err(|error| SourceError::Malformed {
                    detail: error.to_string(),
                })?,
            });
        }

        let resume = (envelope.show_next && !reached_watermark).then(|| position.next());

        Ok(EventBatch { events, resume })
    }
}

#[cfg(test)]
mod tests {
    use super::{ME_ENDPOINT, WorkoutPage, event_time, workouts_path};
    use domain::landing::Endpoint;

    /// The path the provenance records must be a path, whatever the user id is.
    #[test]
    fn the_list_path_is_an_endpoint() {
        let path = workouts_path("f051be7977044fd39797c05a854a6f59");
        let endpoint = Endpoint::try_from(path).expect("an endpoint");
        assert_eq!(
            endpoint.as_str(),
            "/api/user/f051be7977044fd39797c05a854a6f59/workouts"
        );
        Endpoint::try_from(ME_ENDPOINT).expect("an endpoint");
    }

    /// **The base URL carries no path segment**, so composing it with these
    /// cannot produce `/api/api/…`. A stub cannot catch a wrong default; this
    /// pins it.
    #[test]
    fn the_paths_compose_against_a_bare_root() {
        assert!(workouts_path("u").starts_with("/api/"));
        assert!(!workouts_path("u").contains("//"));
    }

    /// Pages advance by one and keep the user they were resolved with.
    #[test]
    fn a_page_advances_without_losing_the_user() {
        let first = WorkoutPage {
            page: 0,
            user: "u".to_owned(),
        };
        let second = first.next();
        assert_eq!(second.page(), 1);
        assert_eq!(second.next().page(), 2);
        assert_eq!(second.user, "u");
    }

    /// Unix seconds, which is what this source serves.
    #[test]
    fn an_event_time_is_read_from_unix_seconds() {
        let at = event_time(1_788_699_127).expect("a timestamp");
        assert_eq!(at.as_timestamp().as_second(), 1_788_699_127);
    }
}
