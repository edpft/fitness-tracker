//! The Garmin adapter.
//!
//! Everything specific to Garmin lives behind this module: its three hosts, its
//! mobile sign-in, and its names for things. What lands is a night's HRV answer
//! exactly as served.
//!
//! **Three hosts, which is one more than any source before it.** Sign-in is at
//! `sso.garmin.com`, tokens are minted at `diauth.garmin.com`, and the API
//! answers at `connectapi.garmin.com`. Peloton needed two and the catalogue's
//! `EmailPassword` carries one auth root, so the token host is derived from the
//! sign-in host here rather than becoming a third field on a credential that
//! every other source would then carry empty.

pub mod account;
pub mod auth;
pub mod hrv;
pub mod translate;

pub use account::{NightAccount, nights};
pub use auth::{GarminAuth, GarminCredentials};
pub use hrv::{GarminHrv, HrvWalk};
pub use translate::GarminHrvTranslator;

use application::SourceError;

/// The token host that goes with a sign-in host.
///
/// `sso.garmin.com` mints nothing; `diauth.garmin.com` does. Derived rather
/// than configured so that pointing the sign-in host at a stub points the token
/// host at the same stub, which is what the contract tests need.
pub fn token_base_for(sso_base: &str) -> String {
    let trimmed = sso_base.trim_end_matches('/');
    match trimmed.split_once("://") {
        Some((scheme, rest)) if rest.starts_with("sso.") => {
            format!("{scheme}://diauth.{}", &rest["sso.".len()..])
        }
        _ => trimmed.to_owned(),
    }
}

/// The text of a Garmin answer, once the transport has said it arrived.
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
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(SourceError::Unavailable {
            detail: format!("{path} is rate-limiting this address; wait rather than retrying"),
        });
    }
    if !status.is_success() {
        return Err(SourceError::Unavailable {
            detail: format!("{path} answered {status}: {}", text.trim()),
        });
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::token_base_for;

    /// The real pair, which no stub can catch: the contract tests point both
    /// at one mock server, so only this test sees what a live run composes.
    #[test]
    fn the_token_host_sits_beside_the_sign_in_host() {
        assert_eq!(
            token_base_for("https://sso.garmin.com"),
            "https://diauth.garmin.com"
        );
    }

    /// A stub is one host, and both roots must land on it.
    #[test]
    fn a_stub_stays_a_stub() {
        assert_eq!(
            token_base_for("http://127.0.0.1:8080"),
            "http://127.0.0.1:8080"
        );
    }

    #[test]
    fn a_trailing_slash_changes_nothing() {
        assert_eq!(
            token_base_for("https://sso.garmin.com/"),
            "https://diauth.garmin.com"
        );
    }
}
