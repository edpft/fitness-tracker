//! School and public holidays, read from the bodies that set them (#181).
//!
//! **Kept as last served, and asked whether it has changed** (#809). Both are
//! facts about the world consulted when planning, and § 14 needs only their
//! current value; a run asks each publisher, and nothing else reaches it.

pub mod public;
pub mod school;

use std::{sync::OnceLock, time::Duration};

use application::{Served, SourceError};
use reqwest::{
    StatusCode,
    header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED},
};

pub use public::{BankHolidays, GOV_UK_BANK_HOLIDAYS};
pub use school::SchoolCalendar;

/// How long to wait. A school's website and a government page, not APIs.
const TIMEOUT: Duration = Duration::from_secs(30);

/// The client, built once on first use: building it initialises TLS, which can
/// fail, and a constructor is the wrong place for that.
fn client(
    cell: &OnceLock<Result<reqwest::Client, String>>,
) -> Result<&reqwest::Client, SourceError> {
    cell.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .map_err(|error| error.to_string())
    })
    .as_ref()
    .map_err(|detail| SourceError::Unavailable {
        detail: detail.clone(),
    })
}

/// What `url` serves now, or `None` where it answers that nothing has changed
/// since `kept` (#809). A body is served only once `read` can read it, so what
/// is kept is always something a page can read.
///
/// **Both validators are sent where both are known**: the school's feed
/// answers `If-Modified-Since` and gov.uk answers `If-None-Match` (checked
/// 2026-11-13). No retry: it is one request, and a failure is reported rather
/// than hidden.
async fn served_since<T>(
    client: &reqwest::Client,
    url: &str,
    kept: Option<&Served>,
    read: impl FnOnce(&str) -> Result<T, SourceError>,
) -> Result<Option<Served>, SourceError> {
    let mut request = client.get(url);
    if let Some(etag) = kept.and_then(|kept| kept.etag.as_deref()) {
        request = request.header(IF_NONE_MATCH, etag);
    }
    if let Some(modified) = kept.and_then(|kept| kept.last_modified.as_deref()) {
        request = request.header(IF_MODIFIED_SINCE, modified);
    }
    crate::http::send(request)
        .await
        .map_err(|error| SourceError::Unavailable {
            detail: format!("{url}: {error}"),
        })?
        .read(|reply| {
            let status = reply.status();
            if status == StatusCode::NOT_MODIFIED && kept.is_some() {
                return Ok(None);
            }
            if !status.is_success() {
                return Err(SourceError::Unavailable {
                    detail: format!("{url}: {status}"),
                });
            }
            let body = reply.text();
            read(&body)?;
            let header = |name| {
                reply
                    .headers()
                    .get(name)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned)
            };
            Ok(Some(Served {
                body,
                etag: header(ETAG),
                last_modified: header(LAST_MODIFIED),
            }))
        })
        .await
}
