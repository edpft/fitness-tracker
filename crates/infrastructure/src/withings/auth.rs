//! Signing in to Withings, and staying signed in.
//!
//! **An OAuth 2 authorisation-code flow against the operator's own registered
//! application.** Withings issues nothing like Hevy's personal key: an
//! application is registered on its developer dashboard, the operator grants it
//! access in a browser, and the code that comes back is exchanged for an access
//! token and a refresh token.
//!
//! ```text
//! browser  GET  account.withings.com/oauth2_user/authorize2   → callback?code=…&state=…
//! here     POST wbsapi.withings.net/v2/oauth2  action=requesttoken
//!               grant_type=authorization_code                  → access + refresh
//!               grant_type=refresh_token                       → access + a new refresh
//! ```
//!
//! **Nothing receives the callback.** The dashboard refuses localhost, and the
//! code is in the browser's address bar whatever page it lands on — so the
//! operator pastes that address back and it is exchanged at once. Withings'
//! codes last thirty seconds, which is why the exchange happens in the same
//! breath as the paste rather than in a later command.
//!
//! **The refresh token rotates, so losing it costs a sign-in.** Every refresh
//! returns a new one. A renewed token that cannot be written down is therefore
//! an error rather than the shrug Peloton's cache gives it: Peloton can log in
//! again unattended, and Withings needs somebody at a browser.
//!
//! **The client secret is the operator's and stays theirs.** It is read by the
//! composition root, sent only to the token endpoint, and never logged.

use std::{
    sync::{Mutex, OnceLock},
    time::Duration,
};

use application::SourceError;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng as _;
use reqwest::{Client, Url};

use crate::token::{Token, TokenFile};

/// What the application is allowed to read: measurements, and nothing else.
pub const SCOPE: &str = "user.metrics";

/// Where the operator is sent to grant access. On the account host, not the API.
const AUTHORIZE_PATH: &str = "/oauth2_user/authorize2";

/// Where codes and refresh tokens are exchanged. On the API host.
const TOKEN_PATH: &str = "/v2/oauth2";

/// The operator's registered application, as the dashboard issued it.
#[derive(Clone)]
pub struct WithingsClient {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

impl WithingsClient {
    pub fn new(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        redirect_uri: impl Into<String>,
    ) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            redirect_uri: redirect_uri.into(),
        }
    }
}

/// Deliberately opaque. A secret that can be printed gets printed.
impl std::fmt::Debug for WithingsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WithingsClient(<redacted>)")
    }
}

/// Holds the token, and renews it when it has to.
///
/// **The token file is not optional here**, where it is for Peloton: a Withings
/// token cannot be obtained without a person, so a composition that did not
/// keep one could never run twice.
///
/// Constructing this does no I/O, for the reason the other adapters give.
pub struct WithingsAuth {
    api_base: String,
    auth_base: String,
    client: WithingsClient,
    cache: TokenFile,
    http: OnceLock<Result<Client, String>>,
    token: Mutex<Option<Token>>,
}

impl std::fmt::Debug for WithingsAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WithingsAuth")
            .field("api_base", &self.api_base)
            .field("auth_base", &self.auth_base)
            .finish_non_exhaustive()
    }
}

impl WithingsAuth {
    /// `api_base` is `https://wbsapi.withings.net` and `auth_base` is
    /// `https://account.withings.com` in production; both are stubs in the
    /// contract tests.
    pub fn new(
        api_base: impl Into<String>,
        auth_base: impl Into<String>,
        client: WithingsClient,
        cache: TokenFile,
    ) -> Self {
        Self {
            api_base: api_base.into().trim_end_matches('/').to_owned(),
            auth_base: auth_base.into().trim_end_matches('/').to_owned(),
            client,
            cache,
            http: OnceLock::new(),
            token: Mutex::new(None),
        }
    }

    /// A fresh `state`, to be handed to [`Self::authorisation_url`] and checked
    /// again by [`Self::sign_in`].
    #[must_use]
    pub fn new_state() -> String {
        let mut bytes = [0_u8; 24];
        rand::rng().fill_bytes(&mut bytes);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Where to send the operator's browser.
    ///
    /// # Errors
    ///
    /// [`SourceError::Malformed`] if the configured account root is not a URL.
    pub fn authorisation_url(&self, state: &str) -> Result<String, SourceError> {
        Url::parse_with_params(
            &format!("{}{AUTHORIZE_PATH}", self.auth_base),
            [
                ("response_type", "code"),
                ("client_id", self.client.client_id.as_str()),
                ("redirect_uri", self.client.redirect_uri.as_str()),
                ("scope", SCOPE),
                ("state", state),
            ],
        )
        .map(String::from)
        .map_err(|error| SourceError::Malformed {
            detail: format!("the Withings account root is not a URL: {error}"),
        })
    }

    /// Exchange the address the browser landed on for a token, and keep it.
    ///
    /// # Errors
    ///
    /// [`SourceError::Malformed`] if the paste carries no code or the wrong
    /// state; whatever the token endpoint answers otherwise; and
    /// [`SourceError::Unavailable`] if the token cannot be written down.
    pub async fn sign_in(&self, pasted: &str, state: &str) -> Result<(), SourceError> {
        let code = code_from(pasted, state)?;
        let token = self
            .request_token(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", self.client.redirect_uri.as_str()),
            ])
            .await?;
        self.keep(token).map(|_| ())
    }

    /// A bearer token: the one held, or a renewed one.
    ///
    /// # Errors
    ///
    /// [`SourceError::Unauthorised`] if there is no token to renew or Withings
    /// refuses to renew it — both mean signing in again.
    pub async fn bearer(&self) -> Result<String, SourceError> {
        let held = self
            .token
            .lock()
            .map_err(|_| SourceError::Unavailable {
                detail: "the token cache was poisoned by an earlier panic".to_owned(),
            })?
            .clone()
            .or_else(|| self.cache.read());

        let Some(token) = held else {
            return Err(SourceError::Unauthorised);
        };
        if token.usable() {
            return Ok(self.hold(token));
        }
        let Some(refresh) = token.refresh().map(str::to_owned) else {
            return Err(SourceError::Unauthorised);
        };
        let fresh = self
            .request_token(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh.as_str()),
            ])
            .await?;
        self.keep(fresh)
    }

    /// Write the token down, then hold it. A token that cannot be written is
    /// an error: see the module notes on rotation.
    fn keep(&self, token: Token) -> Result<String, SourceError> {
        self.cache
            .write(&token)
            .map_err(|error| SourceError::Unavailable {
                detail: format!(
                    "the renewed Withings token could not be kept at {}: {error}. \
                     Withings will want a fresh sign-in next time",
                    self.cache.path().display()
                ),
            })?;
        Ok(self.hold(token))
    }

    fn hold(&self, token: Token) -> String {
        let access = token.access().to_owned();
        if let Ok(mut held) = self.token.lock() {
            *held = Some(token);
        }
        access
    }

    fn http(&self) -> Result<&Client, SourceError> {
        self.http
            .get_or_init(|| {
                Client::builder()
                    .timeout(Duration::from_secs(30))
                    .build()
                    .map_err(|error| error.to_string())
            })
            .as_ref()
            .map_err(|detail| SourceError::Unavailable {
                detail: detail.clone(),
            })
    }

    async fn request_token(&self, grant: &[(&str, &str)]) -> Result<Token, SourceError> {
        let mut form = vec![
            ("action", "requesttoken"),
            ("client_id", self.client.client_id.as_str()),
            ("client_secret", self.client.client_secret.as_str()),
        ];
        form.extend_from_slice(grant);

        let response = self
            .http()?
            .post(format!("{}{TOKEN_PATH}", self.api_base))
            .form(&form)
            .send()
            .await
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })?;
        let text = super::answer(response, TOKEN_PATH).await?;
        let issued: Issued = serde_json::from_str(super::body_of(&text, TOKEN_PATH)?.get())
            .map_err(|error| SourceError::Malformed {
                detail: format!("{TOKEN_PATH} answered without a usable token: {error}"),
            })?;

        let expires_at = jiff::Timestamp::now()
            .checked_add(jiff::SignedDuration::from_secs(issued.expires_in))
            .map_err(|error| SourceError::Malformed {
                detail: format!("{TOKEN_PATH} gave an expiry this clock cannot hold: {error}"),
            })?;
        Ok(Token::new(
            issued.access_token,
            Some(issued.refresh_token),
            expires_at,
        ))
    }
}

/// What the token endpoint's `body` carries that this adapter uses.
#[derive(serde::Deserialize)]
struct Issued {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
}

/// The code in a pasted callback address, provided its state is ours.
///
/// **A bare code is accepted too**, since that is what an operator reading the
/// address bar might copy. It has no state to check, which is the price of
/// accepting it; the code is single-use and thirty seconds old either way.
///
/// # Errors
///
/// [`SourceError::Malformed`] if there is no code, or the address carries a
/// state other than the one this sign-in sent.
pub fn code_from(pasted: &str, state: &str) -> Result<String, SourceError> {
    let pasted = pasted.trim();
    let Ok(address) = Url::parse(pasted) else {
        if pasted.is_empty() || pasted.contains(char::is_whitespace) {
            return Err(SourceError::Malformed {
                detail: "that is neither the address the browser landed on nor a code".to_owned(),
            });
        }
        return Ok(pasted.to_owned());
    };

    let mut code = None;
    let mut returned_state = None;
    for (name, value) in address.query_pairs() {
        match name.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => returned_state = Some(value.into_owned()),
            _ => {}
        }
    }

    if returned_state
        .as_deref()
        .is_some_and(|given| given != state)
    {
        return Err(SourceError::Malformed {
            detail: "that address belongs to a different sign-in; start again".to_owned(),
        });
    }
    code.filter(|code| !code.is_empty())
        .ok_or_else(|| SourceError::Malformed {
            detail: "that address carries no code — was access granted?".to_owned(),
        })
}

#[cfg(test)]
mod tests {
    use super::{TokenFile, WithingsAuth, WithingsClient, code_from};

    fn auth() -> WithingsAuth {
        WithingsAuth::new(
            "https://wbsapi.withings.net",
            "https://account.withings.com/",
            WithingsClient::new("the-client", "the-secret", "https://example.com/back"),
            TokenFile::new("/nowhere/withings.token.json".into()),
        )
    }

    /// **A stub cannot catch a wrong default**, so the composed address is
    /// pinned here: the account host, the documented path, and every parameter
    /// the dashboard checks.
    #[test]
    fn the_authorisation_address_is_the_documented_one() {
        let url = auth().authorisation_url("xyz").expect("a URL");
        assert!(
            url.starts_with("https://account.withings.com/oauth2_user/authorize2?"),
            "{url}"
        );
        for part in [
            "response_type=code",
            "client_id=the-client",
            "redirect_uri=https%3A%2F%2Fexample.com%2Fback",
            "state=xyz",
            "scope=user.metrics",
        ] {
            assert!(url.contains(part), "{url} lacks {part}");
        }
        assert!(
            !url.contains("the-secret"),
            "the secret never goes to the browser"
        );
    }

    #[test]
    fn the_code_is_read_off_the_address_the_browser_landed_on() {
        let code = code_from(
            "https://github.com/edpft/fitness-tracker?code=abc123&state=ours",
            "ours",
        )
        .expect("a code");
        assert_eq!(code, "abc123");
    }

    #[test]
    fn a_bare_code_is_accepted() {
        assert_eq!(code_from("  abc123\n", "ours").expect("a code"), "abc123");
    }

    #[test]
    fn another_sign_ins_address_is_refused() {
        assert!(code_from("https://example.com/?code=abc&state=theirs", "ours").is_err());
    }

    #[test]
    fn an_address_without_a_code_is_refused() {
        assert!(
            code_from(
                "https://example.com/?error=access_denied&state=ours",
                "ours"
            )
            .is_err()
        );
        assert!(code_from("", "ours").is_err());
        assert!(code_from("not a code", "ours").is_err());
    }

    /// Two sign-ins never share a state.
    #[test]
    fn states_differ() {
        assert_ne!(WithingsAuth::new_state(), WithingsAuth::new_state());
    }
}
