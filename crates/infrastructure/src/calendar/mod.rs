//! School and public holidays, read from the bodies that set them (#181).
//!
//! **Read each time and never stored.** Both are facts about the world
//! consulted when planning, and § 14 says such a value, received from an
//! external system, need not be persisted.

pub mod public;
pub mod school;

use std::{sync::OnceLock, time::Duration};

use application::SourceError;

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

/// One document, as served. No retry: it is one request, and a failure is
/// reported rather than hidden.
async fn get(client: &reqwest::Client, url: &str) -> Result<String, SourceError> {
    let unavailable = |error: reqwest::Error| SourceError::Unavailable {
        detail: format!("{url}: {error}"),
    };
    client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(unavailable)?
        .text()
        .await
        .map_err(unavailable)
}
