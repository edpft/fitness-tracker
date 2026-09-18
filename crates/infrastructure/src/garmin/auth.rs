//! Signing in to Garmin Connect, and staying signed in.
//!
//! **The SSO embed widget, because both API front doors are shut.** Garmin's
//! Connect Developer Program serves HRV through its Health API but is not
//! accepting new applications, so this adapter reaches Connect the way a
//! browser does. That is the same standing risk already taken on Peloton: an
//! unofficial surface can change without notice.
//!
//! The phone app's JSON endpoint still exists — `sso.garmin.com/mobile/api/login`
//! — but has answered 429 from Cloudflare since March 2026 regardless of who is
//! asking, which deprecated `garth` and every client built on it. This adapter
//! was written against that endpoint and never reached it: the path it used
//! carried an extra `/sso` and answered 404, so the block was never the thing
//! that failed. The widget form carries no `clientId` and sits in a different
//! bucket, which is why it still answers.
//!
//! ```text
//! GET  sso.garmin.com/sso/embed?id=gauth-widget&…      → session cookies
//! GET  sso.garmin.com/sso/signin?…&service=…/sso/embed → the _csrf in the form
//!      (a pause)
//! POST sso.garmin.com/sso/signin?…                     → "Success", and ?ticket=ST-…
//! POST diauth.garmin.com/di-oauth2-service/oauth/token
//!      grant_type=<service-ticket URN>                 → access + refresh
//!      grant_type=refresh_token                        → access + a new refresh
//! ```
//!
//! **The service is the embed page, and the exchange must name the same one.**
//! A ticket is issued for the service that asked for it, so the `service_url`
//! sent to `diauth` is the URL the sign-in used. It is derived from the sign-in
//! host rather than being a constant, so a stub reaches both halves.
//!
//! **The pause between reading the form and posting it is load-bearing.**
//! Cloudflare reads a GET followed immediately by a credential POST as a bot.
//! A stub is not Cloudflare, so [`GarminAuth::without_pause`] exists for the
//! contract tests and for nothing else.
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
use rand::RngExt as _;
use reqwest::{Client, StatusCode};

use crate::token::{Token, TokenFile};

/// The widget that establishes the session, and the form that signs in.
///
/// Both are pinned as composed URLs in this module's tests. The endpoint this
/// adapter shipped with was `/sso/mobile/api/login`, which is `/mobile/api/login`
/// with an extra segment, and nothing in the suite could tell.
const EMBED_PATH: &str = "/sso/embed";
const SIGNIN_PATH: &str = "/sso/signin";

/// Where a service ticket is exchanged for a bearer token.
const TOKEN_PATH: &str = "/di-oauth2-service/oauth/token";

/// The grant that names a service ticket. A URN, and Garmin's own spelling of
/// it, so it is a constant rather than something composed from the API root.
const TICKET_GRANT: &str =
    "https://connectapi.garmin.com/di-oauth2-service/oauth/grant/service_ticket";

/// What the widget calls itself. Garmin's sign-in page keys its embedded mode
/// on these, and the form it serves without them has no `_csrf` to read.
const WIDGET_ID: &str = "gauth-widget";

/// How long to wait between reading the form and submitting it.
///
/// Cloudflare flags a credential POST that follows its GET too closely. The
/// bounds are the upstream Python client's, which is the implementation that
/// found this flow works at all.
const PAUSE_SECONDS: std::ops::Range<f64> = 3.0..8.0;

/// The DI client ids, newest first.
///
/// Walked in order because they are dated: see the module notes. The exchange
/// takes the first that answers.
const DI_CLIENT_IDS: [&str; 3] = [
    "GARMIN_CONNECT_MOBILE_ANDROID_DI_2025Q2",
    "GARMIN_CONNECT_MOBILE_ANDROID_DI_2024Q4",
    "GARMIN_CONNECT_MOBILE_ANDROID_DI",
];

/// What the phone app calls itself, which is what the token exchange expects.
const NATIVE_USER_AGENT: &str = "GCM-Android-5.23";

/// What the sign-in pages expect, which is not the same thing. The widget is a
/// browser surface and answers a native agent differently.
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
    (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

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
    /// Whether to wait between reading the sign-in form and submitting it.
    /// True everywhere but the contract tests; see the module notes.
    pause: bool,
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
            pause: true,
            token: Mutex::new(None),
            http: OnceLock::new(),
        }
    }

    /// Sign in without the anti-bot pause.
    ///
    /// For a stub, which is not behind Cloudflare and has nothing to be
    /// suspicious of. A live run that calls this is asking to be blocked.
    #[must_use]
    pub fn without_pause(mut self) -> Self {
        self.pause = false;
        self
    }

    /// The service a ticket is issued for, which the exchange has to repeat.
    ///
    /// Derived from the sign-in host so that pointing this adapter at a stub
    /// asks that stub for a ticket naming itself.
    fn service_url(&self) -> String {
        format!("{}{EMBED_PATH}", self.sso_base)
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
                    // The widget flow is a browser flow: the embed page sets
                    // the session cookies the sign-in form is validated
                    // against, and without a jar the POST is rejected.
                    .cookie_store(true)
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

    /// Establish a session, read the form, and submit it.
    ///
    /// Three requests, and each one's failure names itself: a flow this long
    /// that reports "something went wrong" is a flow nobody can debug from the
    /// message it prints.
    async fn service_ticket(&self) -> Result<String, SourceError> {
        let embed = self.service_url();
        let signin = format!("{}{SIGNIN_PATH}", self.sso_base);
        let host = format!("{}/sso", self.sso_base);

        // One: the embed page, for the cookies the form is validated against.
        // Nothing is read from its body.
        let landing = self
            .http()?
            .get(&embed)
            .query(&[
                ("id", WIDGET_ID),
                ("embedWidget", "true"),
                ("gauthHost", host.as_str()),
            ])
            .header(reqwest::header::USER_AGENT, BROWSER_USER_AGENT)
            .send()
            .await
            .map_err(|error| unreachable(&error))?;
        read_page(landing, EMBED_PATH).await?;

        // Two: the sign-in form, for the `_csrf` it will demand back.
        let form = self
            .http()?
            .get(&signin)
            .query(&signin_query(&embed))
            .header(reqwest::header::USER_AGENT, BROWSER_USER_AGENT)
            .header(reqwest::header::REFERER, embed.as_str())
            .send()
            .await
            .map_err(|error| unreachable(&error))?;
        let form = read_page(form, SIGNIN_PATH).await?;
        let csrf = csrf_token(&form).ok_or_else(|| SourceError::Malformed {
            detail: format!("{SIGNIN_PATH} served a form with no _csrf to submit"),
        })?;

        self.wait().await;

        // Three: the credentials.
        let answer = self
            .http()?
            .post(&signin)
            .query(&signin_query(&embed))
            .header(reqwest::header::USER_AGENT, BROWSER_USER_AGENT)
            .header(reqwest::header::REFERER, signin.as_str())
            .form(&[
                ("username", self.credentials.email.as_str()),
                ("password", self.credentials.password.as_str()),
                ("embed", "true"),
                ("_csrf", csrf.as_str()),
            ])
            .send()
            .await
            .map_err(|error| unreachable(&error))?;
        let answer = read_page(answer, SIGNIN_PATH).await?;

        ticket_from(&answer)
    }

    /// The anti-bot pause, or nothing at all against a stub.
    async fn wait(&self) {
        if !self.pause {
            return;
        }
        let seconds = rand::rng().random_range(PAUSE_SECONDS);
        tokio::time::sleep(Duration::from_secs_f64(seconds)).await;
    }

    /// Walk the dated client ids until one accepts the ticket.
    async fn exchange(&self, ticket: &str) -> Result<Held, SourceError> {
        let mut last: Option<SourceError> = None;
        // The ticket was issued for the embed page, and a ticket presented
        // under a different service is not the ticket Garmin minted.
        let service = self.service_url();

        for client_id in DI_CLIENT_IDS {
            match self
                .request_token(
                    client_id,
                    &[
                        ("client_id", client_id),
                        ("service_ticket", ticket),
                        ("grant_type", TICKET_GRANT),
                        ("service_url", service.as_str()),
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

/// The query the sign-in form is asked for and submitted under.
///
/// Every one of these names the embed page. Garmin issues the ticket for
/// `service`, and the widget refuses to render embedded without the rest.
fn signin_query(embed: &str) -> [(&'static str, &str); 7] {
    [
        ("id", WIDGET_ID),
        ("embedWidget", "true"),
        ("gauthHost", embed),
        ("service", embed),
        ("source", embed),
        ("redirectAfterAccountLoginUrl", embed),
        ("redirectAfterAccountCreationUrl", embed),
    ]
}

/// The body of a page, once the status has been read.
///
/// **The status is checked before the body is parsed**, which is the thing the
/// endpoint this replaced did not do: every field of the answer it looked for
/// was optional, so Garmin's 404 parsed cleanly into an empty one and the
/// failure reported itself as a missing field rather than as a 404.
async fn read_page(response: reqwest::Response, path: &str) -> Result<String, SourceError> {
    let status = response.status();
    let text = response.text().await.map_err(|error| unreachable(&error))?;

    if status == StatusCode::TOO_MANY_REQUESTS {
        return Err(SourceError::Unavailable {
            detail: format!(
                "Garmin is rate-limiting {path} from this address; \
                 wait rather than retrying"
            ),
        });
    }
    if status == StatusCode::FORBIDDEN {
        return Err(SourceError::Unavailable {
            detail: format!(
                "{path} answered 403, which is Garmin's bot challenge \
                 rather than a refusal of the password"
            ),
        });
    }
    if !status.is_success() {
        return Err(SourceError::Unavailable {
            detail: format!("{path} answered {status}"),
        });
    }
    Ok(text)
}

/// What the answer to the credential POST means.
///
/// Read from the page's title and its inline script, because that is all
/// Garmin gives: the widget answers 200 whether it signed in, refused the
/// password, or wants a code.
fn ticket_from(html: &str) -> Result<String, SourceError> {
    let title = title_of(html).unwrap_or_default();
    let lowered = title.to_lowercase();

    if [
        "bad gateway",
        "service unavailable",
        "cloudflare",
        "502",
        "503",
    ]
    .iter()
    .any(|hint| lowered.contains(hint))
    {
        return Err(SourceError::Unavailable {
            detail: format!("{SIGNIN_PATH} answered \"{title}\", which is Garmin's own trouble"),
        });
    }

    if ["locked", "invalid", "incorrect", "account error"]
        .iter()
        .any(|hint| lowered.contains(hint))
    {
        return Err(SourceError::Unauthorised);
    }

    // **Not a refused password**, and saying so matters: the password was
    // accepted and Garmin wants a second factor this build cannot supply.
    // Reported rather than driven — see the module notes. The MFA page carries
    // the sign-in page's own title, so the tell is the script it emits.
    if lowered.contains("mfa") || mfa_method(html).is_some() {
        return Err(SourceError::Unavailable {
            detail: "Garmin accepted the password and then asked for a two-factor code, \
                     which this build cannot answer. Turning two-step verification off \
                     for the account, or teaching this adapter the code exchange, are \
                     the two ways forward"
                .to_owned(),
        });
    }

    ticket_in(html).ok_or_else(|| SourceError::Malformed {
        detail: format!("{SIGNIN_PATH} answered \"{title}\" and carried no service ticket"),
    })
}

/// The `_csrf` the form will demand back, bounded to its own input tag.
fn csrf_token(html: &str) -> Option<String> {
    let tag = html.split_once("name=\"_csrf\"")?.1.split_once('>')?.0;
    Some(tag.split_once("value=\"")?.1.split_once('"')?.0.to_owned())
}

/// The service ticket, which the success page carries as a query parameter on
/// whichever URL it redirects the widget to.
fn ticket_in(html: &str) -> Option<String> {
    let ticket: String = html
        .split_once("?ticket=")?
        .1
        .chars()
        .take_while(|character| !"\"'&< \t\r\n".contains(*character))
        .collect();
    ticket.starts_with("ST-").then_some(ticket)
}

/// The MFA page announces its method in an inline script variable.
fn mfa_method(html: &str) -> Option<String> {
    let after = html.split_once("mfaMethod")?.1.split_once('=')?.1;
    let value = after.trim_start().strip_prefix('"')?;
    Some(value.split_once('"')?.0.to_owned())
}

fn title_of(html: &str) -> Option<String> {
    Some(
        html.split_once("<title>")?
            .1
            .split_once("</title>")?
            .0
            .trim()
            .to_owned(),
    )
}

fn unreachable(error: &reqwest::Error) -> SourceError {
    SourceError::Unavailable {
        detail: error.to_string(),
    }
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
    use application::SourceError;

    use super::{
        DI_CLIENT_IDS, EMBED_PATH, SIGNIN_PATH, TICKET_GRANT, TOKEN_PATH, basic_auth, csrf_token,
        signin_query, ticket_from, ticket_in,
    };

    /// The composed paths, pinned here because a stub cannot catch a wrong
    /// default: the contract tests point the bases at a mock server, so only
    /// this test sees what a real run would compose.
    ///
    /// **This is the test that was missing.** The endpoint this adapter shipped
    /// with composed to `https://sso.garmin.com/sso/mobile/api/login`, which is
    /// a 404 — the path is `/mobile/api/login` — and the suite could not tell,
    /// because every stub answered whatever path it was given.
    #[test]
    fn the_paths_are_what_garmin_serves() {
        assert_eq!(
            format!("https://sso.garmin.com{EMBED_PATH}"),
            "https://sso.garmin.com/sso/embed"
        );
        assert_eq!(
            format!("https://sso.garmin.com{SIGNIN_PATH}"),
            "https://sso.garmin.com/sso/signin"
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

    /// The service a ticket will be issued for is the embed page, and the
    /// exchange has to repeat it. A query that named anything else would earn
    /// a ticket the token endpoint refuses.
    #[test]
    fn the_signin_query_asks_for_the_embed_page() {
        let embed = "https://sso.garmin.com/sso/embed";
        let query = signin_query(embed);
        let service = query
            .iter()
            .find_map(|(name, value)| (*name == "service").then_some(*value));
        assert_eq!(service, Some(embed));
        assert!(
            query
                .iter()
                .all(|(name, value)| *name == "id" || *name == "embedWidget" || *value == embed)
        );
    }

    /// Garmin's own spelling: the value follows the name in the same tag.
    #[test]
    fn the_csrf_is_read_from_its_input_tag() {
        let form = r#"<form><input type="hidden" name="_csrf" value="21C2008599188DC3" />
            <input type="text" name="username" value="" /></form>"#;
        assert_eq!(csrf_token(form).as_deref(), Some("21C2008599188DC3"));
    }

    /// A form with no token is a page we cannot submit, not a token of "".
    #[test]
    fn a_form_without_a_csrf_reads_as_none() {
        assert!(csrf_token("<form><input name=\"username\" /></form>").is_none());
    }

    #[test]
    fn the_ticket_is_read_from_the_success_page() {
        let page = r#"<title>Success</title>
            <script>response_url = "https://sso.garmin.com/sso/embed?ticket=ST-12345-abcXYZ-cas";</script>"#;
        assert_eq!(ticket_in(page).as_deref(), Some("ST-12345-abcXYZ-cas"));
        assert_eq!(ticket_from(page).expect("a ticket"), "ST-12345-abcXYZ-cas");
    }

    /// A refused password is `Unauthorised`, so the wizard says the source
    /// would not have it rather than reporting a transport failure.
    #[test]
    fn a_refused_password_is_unauthorised() {
        let page = "<title>Invalid username or password</title>";
        assert!(matches!(ticket_from(page), Err(SourceError::Unauthorised)));
    }

    /// The MFA page carries the sign-in page's title, so the tell is the
    /// script variable rather than the title alone.
    #[test]
    fn mfa_is_read_from_the_script_not_the_title() {
        let page = r#"<title>GARMIN Authentication Application</title>
            <script>var mfaMethod = "email"; var customerGuid = "abc";</script>"#;
        let Err(SourceError::Unavailable { detail }) = ticket_from(page) else {
            panic!("MFA should be reported as unavailable, not as a refused password")
        };
        assert!(detail.contains("two-factor"), "{detail}");
    }

    /// The failure this replaces: a page that is neither success nor refusal
    /// names what it was, rather than reporting a missing field.
    #[test]
    fn an_unexpected_page_names_its_title() {
        let Err(SourceError::Malformed { detail }) =
            ticket_from("<title>Service Maintenance</title>")
        else {
            panic!("an unreadable answer should be malformed")
        };
        assert!(detail.contains("Service Maintenance"), "{detail}");
    }
}
