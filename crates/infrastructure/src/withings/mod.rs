//! The Withings adapter.
//!
//! Everything specific to Withings lives behind this module: its hosts, its
//! envelope, its status codes and its names for things. What lands is the
//! measure group exactly as served; what a Body Scan weigh-in *is* waits for
//! those payloads to be read (#117, #153).

pub mod auth;
pub mod measurements;

pub use auth::{WithingsAuth, WithingsClient};
pub use measurements::{MeasurementPage, WithingsMeasurements};

use application::SourceError;
use serde::Deserialize;
use serde_json::value::RawValue;

/// The status Withings reports for a token it does not accept.
const INVALID_TOKEN: i64 = 401;

/// The text of a Withings answer, once the transport has said it arrived.
async fn answer(response: reqwest::Response, path: &str) -> Result<String, SourceError> {
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(SourceError::Unauthorised);
    }
    let text = response
        .text()
        .await
        .map_err(|error| SourceError::Unavailable {
            detail: error.to_string(),
        })?;
    if !status.is_success() {
        return Err(SourceError::Unavailable {
            detail: format!("{path} answered {status}: {}", text.trim()),
        });
    }
    Ok(text)
}

/// A Withings answer, with its body left as the source's own bytes.
#[derive(Deserialize)]
struct Envelope<'a> {
    status: Option<i64>,
    #[serde(borrow, default)]
    body: Option<&'a RawValue>,
    #[serde(default)]
    error: Option<String>,
}

/// The `body` of an answer, or the failure it reports.
///
/// **Withings reports failure inside a 200.** Every answer is an envelope,
/// `{"status": 0, "body": {…}}` on success and `{"status": N, "error": "…"}`
/// otherwise, so the HTTP status alone says almost nothing.
///
/// **Borrowed, not parsed**, so what is landed from the body is the bytes
/// Withings sent rather than a re-serialisation of them (§ II.1).
fn body_of<'a>(text: &'a str, path: &str) -> Result<&'a RawValue, SourceError> {
    let envelope: Envelope<'a> =
        serde_json::from_str(text).map_err(|error| SourceError::Malformed {
            detail: format!("{path} answered something that is not an envelope: {error}"),
        })?;

    match envelope.status {
        Some(0) => envelope.body.ok_or_else(|| SourceError::Malformed {
            detail: format!("{path} reported success without a body"),
        }),
        Some(INVALID_TOKEN) => Err(SourceError::Unauthorised),
        Some(code) => Err(SourceError::Unavailable {
            detail: format!(
                "{path} answered status {code}: {}",
                envelope.error.as_deref().unwrap_or("no reason given")
            ),
        }),
        None => Err(SourceError::Malformed {
            detail: format!("{path} answered without a status"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{SourceError, body_of};

    #[test]
    fn success_is_the_body() {
        let body = body_of(r#"{"status":0,"body":{ "more" : 0 }}"#, "/measure").expect("a body");
        assert_eq!(body.get(), r#"{ "more" : 0 }"#, "the bytes as served");
    }

    #[test]
    fn a_refused_token_is_unauthorised_even_inside_a_200() {
        assert!(matches!(
            body_of(
                r#"{"status":401,"body":{},"error":"invalid_token"}"#,
                "/measure"
            ),
            Err(SourceError::Unauthorised)
        ));
    }

    #[test]
    fn any_other_status_says_what_withings_said() {
        let Err(SourceError::Unavailable { detail }) = body_of(
            r#"{"status":601,"body":{},"error":"Too Many Requests"}"#,
            "/measure",
        ) else {
            panic!("an unavailable source");
        };
        assert!(
            detail.contains("601") && detail.contains("Too Many Requests"),
            "{detail}"
        );
    }

    #[test]
    fn an_answer_without_a_status_is_malformed() {
        assert!(matches!(
            body_of(r#"{"body":{}}"#, "/measure"),
            Err(SourceError::Malformed { .. })
        ));
    }
}
