//! The HTTP adapter for Hevy's workout events feed.

use std::time::Duration;

use application::{EventPage, SourceError, WorkoutEventSource, paging::PageNumber};
use domain::landing::Watermark;
use reqwest::{Client, StatusCode, header::RETRY_AFTER};

use super::{
    page::parse_page,
    retry::{RetryPolicy, is_retryable},
};

/// The feed's path, from the API root. Also what every landing record records
/// as its endpoint, so the two cannot drift apart.
///
/// The base URL is therefore the root and carries no version segment; see the
/// test at the foot of this file.
pub const EVENTS_ENDPOINT: &str = "/v1/workouts/events";

/// The source caps this at 10 and rejects anything larger with a 400, so it is
/// a constant rather than a setting: there is no larger page to ask for.
const PAGE_SIZE: u32 = 10;

/// The epoch, which is also the source's own default for `since`.
const EPOCH: &str = "1970-01-01T00:00:00Z";

/// Hevy's workout events feed.
#[derive(Debug, Clone)]
pub struct HevyWorkoutEvents {
    client: Client,
    base_url: String,
    api_key: String,
    retry: RetryPolicy,
}

impl HevyWorkoutEvents {
    /// # Errors
    ///
    /// Returns [`SourceError::Unavailable`] if an HTTP client cannot be built.
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Result<Self, SourceError> {
        Self::with_retry(base_url, api_key, RetryPolicy::default())
    }

    /// # Errors
    ///
    /// Returns [`SourceError::Unavailable`] if an HTTP client cannot be built.
    pub fn with_retry(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        retry: RetryPolicy,
    ) -> Result<Self, SourceError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })?;

        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            api_key: api_key.into(),
            retry,
        })
    }

    pub(crate) fn url(&self) -> String {
        format!("{}{EVENTS_ENDPOINT}", self.base_url)
    }
}

/// `Retry-After` in its delay-seconds form. The HTTP-date form is not parsed:
/// this API sends no such header at all, and guessing at a format we have
/// never seen would be inventing behaviour.
fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

impl WorkoutEventSource for HevyWorkoutEvents {
    async fn fetch_page(
        &self,
        since: Option<Watermark>,
        page: PageNumber,
    ) -> Result<EventPage, SourceError> {
        // The stored watermark is passed through unmodified. `since` is
        // inclusive at the source, so the boundary event is served again and
        // deduplicated — which costs nothing, and cannot skip a sibling that
        // shares its timestamp the way an exclusive bound would.
        let since = since.map_or_else(|| EPOCH.to_owned(), |mark| mark.to_string());
        let page_number = page.get().to_string();
        let page_size = PAGE_SIZE.to_string();

        let mut attempt = 0;
        loop {
            let outcome = self
                .client
                .get(self.url())
                .header("api-key", &self.api_key)
                .query(&[
                    ("since", since.as_str()),
                    ("page", page_number.as_str()),
                    ("pageSize", page_size.as_str()),
                ])
                .send()
                .await;

            let retryable_detail =
                match outcome {
                    Ok(response) => {
                        let status = response.status();
                        if status.is_success() {
                            let body = response.bytes().await.map_err(|error| {
                                SourceError::Unavailable {
                                    detail: error.to_string(),
                                }
                            })?;
                            return parse_page(&body);
                        }

                        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                            // Terminal. The body here is the bare string
                            // `InvalidApiKey` rather than JSON, which is why
                            // nothing tries to parse it.
                            return Err(SourceError::Unauthorised);
                        }

                        if !is_retryable(status.as_u16()) {
                            // A 400 or a 404 is a fault in our request, not a
                            // passing condition at the source. Asking again would
                            // get the same answer.
                            let detail = response.text().await.unwrap_or_default();
                            return Err(SourceError::Malformed {
                                detail: format!("{status}: {}", detail.trim()),
                            });
                        }

                        let wait = retry_after(&response);
                        let detail = format!("{status}");
                        if attempt + 1 >= self.retry.attempts() {
                            return Err(SourceError::Unavailable {
                                detail: format!("{detail} after {} attempts", attempt + 1),
                            });
                        }
                        tokio::time::sleep(self.retry.backoff(attempt, wait)).await;
                        detail
                    }
                    Err(error) => {
                        // Transport failures — refused, reset, timed out — are the
                        // ordinary shape of a source being unreachable.
                        let detail = error.to_string();
                        if attempt + 1 >= self.retry.attempts() {
                            return Err(SourceError::Unavailable {
                                detail: format!("{detail} after {} attempts", attempt + 1),
                            });
                        }
                        tokio::time::sleep(self.retry.backoff(attempt, None)).await;
                        detail
                    }
                };

            let _ = retryable_detail;
            attempt += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EVENTS_ENDPOINT, HevyWorkoutEvents};

    /// The composed URL, pinned.
    ///
    /// A live run once produced `/v1/v1/workouts/events` because the default
    /// base URL also ended in `/v1`. The stub-based contract tests could not
    /// catch it: their base URI has no version segment to double up.
    #[test]
    fn the_base_url_and_the_endpoint_compose_to_the_real_url() {
        let source = HevyWorkoutEvents::new("https://api.hevyapp.com", "k").expect("a source");
        assert_eq!(source.url(), "https://api.hevyapp.com/v1/workouts/events");
        assert_eq!(EVENTS_ENDPOINT, "/v1/workouts/events");
    }

    /// A trailing slash on the configured base must not double the separator.
    #[test]
    fn a_trailing_slash_on_the_base_url_is_tolerated() {
        let source = HevyWorkoutEvents::new("https://api.hevyapp.com/", "k").expect("a source");
        assert_eq!(source.url(), "https://api.hevyapp.com/v1/workouts/events");
    }
}
