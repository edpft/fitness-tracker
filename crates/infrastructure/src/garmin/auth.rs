//! Signing in to Garmin Connect, and staying signed in.
//!
//! **The mobile app's SSO flow, because the official one is shut.** Garmin's
//! Connect Developer Program serves HRV through its Health API but is not
//! accepting new applications, so this adapter reaches Connect the way the
//! phone app does. That is the same standing risk already taken on Peloton:
//! an unofficial surface can change without notice.
//!
//! ```text
//! POST sso.garmin.com/sso/mobile/api/login?clientId=…&service=…
//!      {username, password}                      → serviceTicketId (ST-…)
//! POST diauth.garmin.com/di-oauth2-service/oauth/token
//!      grant_type=<service-ticket URN>           → access + refresh
//!      grant_type=refresh_token                  → access + a new refresh
//! ```
//!
//! **The client ids are dated, so they are a list rather than a constant.**
//! `GARMIN_CONNECT_MOBILE_ANDROID_DI_2025Q2` sits in front of two older ones,
//! and the exchange walks them until one is accepted. A quarter-stamped
//! identifier is a surface that rotates, and pinning one would make this adapter
//! fail on a Tuesday for a reason nobody could read.
//!
//! **A refused password is not retried.** The same rule Peloton's login states:
//! retrying a rejected password is what a credential-stuffing attempt looks
//! like, and Garmin rate-limits the login endpoint hard enough to say so with a
//! 429.
//!
//! **MFA is reported, not driven.** The operator's account does not have it
//! enabled. If Garmin asks for a code anyway — it has turned MFA on unilaterally
//! before — the login says so in terms that name the next step, rather than
//! failing as though the password were wrong.
//!
//! **The password is the operator's and stays theirs.** It is read from the
//! environment by the composition root, sent only to the login endpoint, and
//! never logged.

use std::{
    sync::{Mutex, OnceLock},
    time::Duration,
};

use application::SourceError;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::{Client, StatusCode};

use crate::token::{Token, TokenFile};

/// Where a password is exchanged for a service ticket.
const LOGIN_PATH: &str = "/sso/mobile/api/login";

/// Where a service ticket is exchanged for a bearer token.
const TOKEN_PATH: &str = "/di-oauth2-service/oauth/token";

/// The grant that names a service ticket. A URN, and Garmin's own spelling of
/// it, so it is a constant rather than something composed from the API root.
const TICKET_GRANT: &str =
    "https://connectapi.garmin.com/di-oauth2-service/oauth/grant/service_ticket";

/// The app this login presents itself as, and the service it asks for.
const SSO_CLIENT_ID: &str = "GCM_ANDROID_DARK";
const SSO_SERVICE_URL: &str = "https://mobile.integration.garmin.com/gcm/android";

/// The DI client ids, newest first.
///
/// Walked in order because they are dated: see the module notes. The exchange
/// takes the first that answers.
const DI_CLIENT_IDS: [&str; 3] = [
    "GARMIN_CONNECT_MOBILE_ANDROID_DI_2025Q2",
    "GARMIN_CONNECT_MOBILE_ANDROID_DI_2024Q4",
    "GARMIN_CONNECT_MOBILE_ANDROID_DI",
];

/// What the phone app calls itself. Garmin serves a different surface to a
/// browser, so this is load-bearing rather than cosmetic.
const NATIVE_USER_AGENT: &str = "GCM-Android-5.23";

/// How long a bearer token is assumed good for when the exchange does not say.
///
/// Garmin's answer normally carries `expires_in`. Where it does not, an hour is
/// short enough that a stale token costs one refresh rather than a run.
const ASSUMED_LIFETIME: i64 = 3_600;

/// The account this adapter signs in as.
#[derive(Clone)]
pub struct GarminCredentials {
    email: String,
    password: String,
}

impl GarminCredentials {
    pub fn new(email: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            password: password.into(),
        }
    }
}

/// Deliberately opaque. A secret that can be printed gets printed.
impl std::fmt::Debug for GarminCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GarminCredentials(<redacted>)")
    }
}

/// Holds the token, and renews or re-earns it when it has to.
///
/// **The token file is optional here, as it is for Peloton**, and for the same
/// reason: this adapter holds a password, so it can sign in again unattended.
/// Losing the cache costs a login rather than a person at a browser, which is
/// what made Withings' cache mandatory.
///
/// Constructing this does no I/O, for the reason the other adapters give.
pub struct GarminAuth {
    sso_base: String,
    token_base: String,
    credentials: GarminCredentials,
    cache: Option<TokenFile>,
    token: Mutex<Option<Held>>,
    http: OnceLock<Result<Client, String>>,
}

/// A token and the client id that minted it, which its refresh must repeat.
#[derive(Clone)]
struct Held {
    token: Token,
    client_id: String,
}

impl std::fmt::Debug for GarminAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GarminAuth")
            .field("sso_base", &self.sso_base)
            .field("token_base", &self.token_base)
            .finish_non_exhaustive()
    }
}

impl GarminAuth {
    pub fn new(
        sso_base: impl Into<String>,
        token_base: impl Into<String>,
        credentials: GarminCredentials,
        cache: Option<TokenFile>,
    ) -> Self {
        Self {
            sso_base: sso_base.into().trim_end_matches('/').to_owned(),
            token_base: token_base.into().trim_end_matches('/').to_owned(),
            credentials,
            cache,
            token: Mutex::new(None),
            http: OnceLock::new(),
        }
    }

    /// A bearer token: the one held, a renewed one, or a freshly earned one.
    ///
    /// # Errors
    ///
    /// [`SourceError::Unauthorised`] if Garmin refuses the password;
    /// [`SourceError::Unavailable`] if it cannot be reached or rate-limits the
    /// attempt; [`SourceError::Malformed`] if it answers something unreadable.
    pub async fn bearer(&self) -> Result<String, SourceError> {
        let held = self.held()?;

        if let Some(held) = &held
            && held.token.usable()
        {
            return Ok(held.token.access().to_owned());
        }

        // A refresh is cheaper than a login and does not touch the password.
        if let Some(held) = held
            && let Some(refresh) = held.token.refresh()
            && let Ok(fresh) = self.refresh(&held.client_id, refresh).await
        {
            return Ok(self.keep(fresh));
        }

        let earned = self.log_in().await?;
        Ok(self.keep(earned))
    }

    /// What is held in memory, falling back to what was written down.
    ///
    /// The cached token carries no client id — the file predates knowing one is
    /// needed — so a token read from disk is refreshed against the newest id and
    /// falls back to a login if that is refused.
    fn held(&self) -> Result<Option<Held>, SourceError> {
        let in_memory = self
            .token
            .lock()
            .map_err(|_| SourceError::Unavailable {
                detail: "the token cache was poisoned by an earlier panic".to_owned(),
            })?
            .clone();

        Ok(in_memory.or_else(|| {
            self.cache.as_ref()?.read().map(|token| Held {
                token,
                client_id: DI_CLIENT_IDS[0].to_owned(),
            })
        }))
    }

    /// Hold the token, and write it down if there is somewhere to write it.
    ///
    /// **A cache that cannot be written is not an error**, unlike Withings':
    /// this adapter can log in again by itself, so the cost is a login rather
    /// than a person at a browser.
    fn keep(&self, held: Held) -> String {
        let access = held.token.access().to_owned();
        if let Some(cache) = &self.cache {
            let _ = cache.write(&held.token);
        }
        if let Ok(mut slot) = self.token.lock() {
            *slot = Some(held);
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

    /// Exchange the password for a service ticket, then the ticket for a token.
    async fn log_in(&self) -> Result<Held, SourceError> {
        let ticket = self.service_ticket().await?;
        self.exchange(&ticket).await
    }

    async fn service_ticket(&self) -> Result<String, SourceError> {
        let response = self
            .http()?
            .post(format!("{}{LOGIN_PATH}", self.sso_base))
            .query(&[
                ("clientId", SSO_CLIENT_ID),
                ("locale", "en-GB"),
                ("service", SSO_SERVICE_URL),
            ])
            .header(reqwest::header::USER_AGENT, NATIVE_USER_AGENT)
            .header(reqwest::header::ORIGIN, self.sso_base.as_str())
            .json(&serde_json::json!({
                "username": self.credentials.email,
                "password": self.credentials.password,
                "rememberMe": true,
                "captchaToken": "",
            }))
            .send()
            .await
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })?;

        // Named before the body is read: a 429 carries no JSON worth parsing,
        // and retrying a refused password is the thing not to do.
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(SourceError::Unavailable {
                detail: "Garmin is rate-limiting sign-in from this address; \
                         wait rather than retrying"
                    .to_owned(),
            });
        }
        if status == StatusCode::FORBIDDEN {
            return Err(SourceError::Unavailable {
                detail: "Garmin answered the sign-in with 403, which is its bot challenge \
                         rather than a refusal of the password"
                    .to_owned(),
            });
        }

        let answer: LoginAnswer =
            serde_json::from_str(&text).map_err(|error| SourceError::Malformed {
                detail: format!("{LOGIN_PATH} answered something unreadable: {error}"),
            })?;

        match answer.response_status.r#type.as_deref() {
            Some("SUCCESSFUL") => answer
                .service_ticket_id
                .ok_or_else(|| SourceError::Malformed {
                    detail: format!("{LOGIN_PATH} reported success without a service ticket"),
                }),
            // **Not a refused password**, and saying so matters: the password
            // was accepted and Garmin wants a second factor this build cannot
            // supply. Reported rather than driven — see the module notes.
            Some("MFA_REQUIRED") => Err(SourceError::Unavailable {
                detail: "Garmin accepted the password and then asked for a two-factor code, \
                         which this build cannot answer. Turning two-step verification off \
                         for the account, or teaching this adapter the code exchange, are \
                         the two ways forward"
                    .to_owned(),
            }),
            Some("INVALID_USERNAME_PASSWORD") => Err(SourceError::Unauthorised),
            Some(other) => Err(SourceError::Unavailable {
                detail: format!("{LOGIN_PATH} answered {other}"),
            }),
            None => Err(SourceError::Malformed {
                detail: format!("{LOGIN_PATH} answered without a status"),
            }),
        }
    }

    /// Walk the dated client ids until one accepts the ticket.
    async fn exchange(&self, ticket: &str) -> Result<Held, SourceError> {
        let mut last: Option<SourceError> = None;

        for client_id in DI_CLIENT_IDS {
            match self
                .request_token(
                    client_id,
                    &[
                        ("client_id", client_id),
                        ("service_ticket", ticket),
                        ("grant_type", TICKET_GRANT),
                        ("service_url", SSO_SERVICE_URL),
                    ],
                )
                .await
            {
                Ok(token) => {
                    return Ok(Held {
                        token,
                        client_id: client_id.to_owned(),
                    });
                }
                Err(error) => last = Some(error),
            }
        }

        Err(last.unwrap_or_else(|| SourceError::Unavailable {
            detail: format!("{TOKEN_PATH} refused every known client id"),
        }))
    }

    async fn refresh(&self, client_id: &str, refresh: &str) -> Result<Held, SourceError> {
        let token = self
            .request_token(
                client_id,
                &[
                    ("grant_type", "refresh_token"),
                    ("client_id", client_id),
                    ("refresh_token", refresh),
                ],
            )
            .await?;
        Ok(Held {
            token,
            client_id: client_id.to_owned(),
        })
    }

    async fn request_token(
        &self,
        client_id: &str,
        form: &[(&str, &str)],
    ) -> Result<Token, SourceError> {
        let response = self
            .http()?
            .post(format!("{}{TOKEN_PATH}", self.token_base))
            .header(reqwest::header::AUTHORIZATION, basic_auth(client_id))
            .header(reqwest::header::USER_AGENT, NATIVE_USER_AGENT)
            .form(form)
            .send()
            .await
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })?;

        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(SourceError::Unavailable {
                detail: "Garmin is rate-limiting token exchange from this address".to_owned(),
            });
        }
        if !status.is_success() {
            return Err(SourceError::Unavailable {
                detail: format!("{TOKEN_PATH} answered {status} for {client_id}"),
            });
        }

        let issued: Issued =
            serde_json::from_str(&text).map_err(|error| SourceError::Malformed {
                detail: format!("{TOKEN_PATH} answered without a usable token: {error}"),
            })?;

        let expires_at = jiff::Timestamp::now()
            .checked_add(jiff::SignedDuration::from_secs(
                issued.expires_in.unwrap_or(ASSUMED_LIFETIME),
            ))
            .map_err(|error| SourceError::Malformed {
                detail: format!("{TOKEN_PATH} gave an expiry this clock cannot hold: {error}"),
            })?;

        Ok(Token::new(
            issued.access_token,
            issued.refresh_token,
            expires_at,
        ))
    }
}

/// The client id as a basic-auth header, with no password after the colon.
fn basic_auth(client_id: &str) -> String {
    format!("Basic {}", STANDARD.encode(format!("{client_id}:")))
}

/// What the login answers that this adapter reads.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginAnswer {
    #[serde(default)]
    response_status: ResponseStatus,
    #[serde(default)]
    service_ticket_id: Option<String>,
}

#[derive(serde::Deserialize, Default)]
struct ResponseStatus {
    #[serde(default)]
    r#type: Option<String>,
}

/// What the token endpoint carries that this adapter uses.
#[derive(serde::Deserialize)]
struct Issued {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::{DI_CLIENT_IDS, LOGIN_PATH, LoginAnswer, TICKET_GRANT, TOKEN_PATH, basic_auth};

    /// The composed paths, pinned here because a stub cannot catch a wrong
    /// default: the contract tests point the bases at a mock server, so only
    /// this test sees what a real run would compose.
    #[test]
    fn the_paths_are_what_garmin_serves() {
        assert_eq!(
            format!("https://sso.garmin.com{LOGIN_PATH}"),
            "https://sso.garmin.com/sso/mobile/api/login"
        );
        assert_eq!(
            format!("https://diauth.garmin.com{TOKEN_PATH}"),
            "https://diauth.garmin.com/di-oauth2-service/oauth/token"
        );
    }

    /// The grant names the API host, which is neither of the two bases this
    /// adapter is given. It is Garmin's spelling of a URN, not a URL we call.
    #[test]
    fn the_ticket_grant_is_a_urn_not_a_base() {
        assert!(TICKET_GRANT.ends_with("/oauth/grant/service_ticket"));
    }

    /// Newest first, because the exchange takes the first that answers.
    #[test]
    fn the_client_ids_are_newest_first() {
        assert!(DI_CLIENT_IDS[0].contains("2025Q2"));
        assert_eq!(DI_CLIENT_IDS.len(), 3);
    }

    /// The client id is the username and there is no password.
    #[test]
    fn basic_auth_is_the_client_id_and_an_empty_password() {
        assert_eq!(basic_auth("ID"), "Basic SUQ6");
    }

    #[test]
    fn a_successful_login_carries_a_ticket() {
        let answer: LoginAnswer = serde_json::from_str(
            r#"{"responseStatus":{"type":"SUCCESSFUL"},"serviceTicketId":"ST-1"}"#,
        )
        .expect("a login answer");
        assert_eq!(answer.response_status.r#type.as_deref(), Some("SUCCESSFUL"));
        assert_eq!(answer.service_ticket_id.as_deref(), Some("ST-1"));
    }

    /// MFA is a shape the answer has, not a transport failure.
    #[test]
    fn mfa_is_read_from_the_status() {
        let answer: LoginAnswer =
            serde_json::from_str(r#"{"responseStatus":{"type":"MFA_REQUIRED"}}"#)
                .expect("a login answer");
        assert_eq!(
            answer.response_status.r#type.as_deref(),
            Some("MFA_REQUIRED")
        );
        assert!(answer.service_ticket_id.is_none());
    }

    #[test]
    fn an_answer_without_a_status_is_readable_and_empty() {
        let answer: LoginAnswer = serde_json::from_str("{}").expect("a login answer");
        assert!(answer.response_status.r#type.is_none());
    }
}
