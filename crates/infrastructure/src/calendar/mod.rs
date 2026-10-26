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

/// One document, as served and read by `read`. No retry: it is one request,
/// and a failure is reported rather than hidden.
async fn get<T>(
    client: &reqwest::Client,
    url: &str,
    read: impl FnOnce(&str) -> Result<T, SourceError>,
) -> Result<T, SourceError> {
    crate::http::send(client.get(url))
        .await
        .map_err(|error| SourceError::Unavailable {
            detail: format!("{url}: {error}"),
        })?
        .read(|reply| {
            let status = reply.status();
            if !status.is_success() {
                return Err(SourceError::Unavailable {
                    detail: format!("{url}: {status}"),
                });
            }
            read(&reply.text())
        })
        .await
}
