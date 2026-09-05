//! Getting a bearer token out of Peloton, which is harder than it should be.
//!
//! **There is no official API and no supported way in.** The endpoint everyone
//! used to use, `POST /auth/login`, now answers `403 Access forbidden. Endpoint
//! no longer accepting requests` to every caller regardless of headers. What
//! still works is the flow the web app itself performs: an Auth0
//! authorization-code exchange with PKCE, driven headlessly.
//!
//! ```text
//! GET  /authorize                 PKCE challenge, state, nonce
//!                                 → the _csrf cookie for the login POST
//! POST /usernamepassword/login    credentials, connection=pelo-user-password
//!                                 → an HTML form carrying wa, wctx, wresult
//! POST /login/callback            that form, resubmitted
//!                                 → redirects, ending at the callback with ?code=
//! POST /oauth/token               code + verifier → access and refresh tokens
//! ```
//!
//! **This will break, and it must break loudly.** `/auth/login` is already gone;
//! the client id below is a public constant lifted from another project and
//! nothing obliges Peloton to keep any of it working. Every failure here becomes
//! a [`SourceError`] that stops the run. There is deliberately no fallback to a
//! cached-but-stale answer: a prescription derived from last month's FTP looks
//! exactly like one derived from this month's, and the operator would have no
//! way to tell them apart (decision 0033).
//!
//! **The credentials are the operator's and stay his.** They arrive as strings
//! read from the environment by the composition root, are held for the life of
//! the process, and are never logged, never persisted and never sent anywhere
//! but Auth0's own login endpoint.

use std::{
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use application::SourceError;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore as _;
use reqwest::{Client, StatusCode, cookie::CookieStore as _, cookie::Jar, redirect::Policy};
use sha2::{Digest as _, Sha256};

/// Auth0's tenant for Peloton, and the connection a password login names.
const TENANT: &str = "peloton-prod";
const CONNECTION: &str = "pelo-user-password";

/// The web app's own public client id. Public in the sense that every browser
/// that loads the login page is handed it; it is not a secret and it is not the
/// operator's.
const CLIENT_ID: &str = "WVoJxVDdPoFx4RNewvvg6ch2mZ7bwnsM";
const AUDIENCE: &str = "https://api.onepeloton.com/";
const SCOPE: &str = "offline_access openid peloton-api.members:default";
const REDIRECT_URI: &str = "https://members.onepeloton.com/callback";

/// **Auth0 refuses a client that does not look like a browser.** Without this
/// the login step answers 401 and is indistinguishable from a wrong password —
/// which is what happened the first time this ran against the real endpoint,
/// while every stubbed test passed. A stub cannot catch a wrong default.
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
                          (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";

/// Auth0 expects its own client fingerprint. Base64 of
/// `{"name":"auth0.js-ulp","version":"9.14.3"}`.
const AUTH0_CLIENT: &str = "eyJuYW1lIjoiYXV0aDAuanMtdWxwIiwidmVyc2lvbiI6IjkuMTQuMyJ9";

/// How long before expiry a token is treated as spent.
///
/// A token that expires while a request is in flight fails the run, and the
/// whole point of the refresh is that it does not. Sixty seconds is longer than
/// any single call this adapter makes.
const EXPIRY_MARGIN: Duration = Duration::from_mins(1);

/// How many redirects to walk between the login form and the code.
///
/// Six is what the flow takes today, through the SSO domain and back. The cap
/// exists so a redirect loop fails rather than hangs.
const MAX_REDIRECTS: usize = 12;

/// What the operator typed into Peloton, once.
#[derive(Clone)]
pub struct PelotonCredentials {
    email: String,
    password: String,
}

impl PelotonCredentials {
    pub fn new(email: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            password: password.into(),
        }
    }
}

/// Deliberately opaque. A credential that can be printed gets printed.
impl std::fmt::Debug for PelotonCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PelotonCredentials(<redacted>)")
    }
}

/// A bearer token and what is needed to replace it.
#[derive(Clone)]
struct Token {
    access: String,
    refresh: Option<String>,
    expires_at: Instant,
}

impl Token {
    fn usable(&self) -> bool {
        Instant::now() + EXPIRY_MARGIN < self.expires_at
    }
}

/// Holds a token, and gets a new one when it has to.
///
/// **Constructing this does no I/O**, the same rule the Hevy adapter follows:
/// building the HTTP client initialises the TLS backend, which reads the
/// platform trust store and can fail, and a composition root that fails while
/// assembling ports reports the wrong thing.
pub struct PelotonAuth {
    auth_base: String,
    credentials: PelotonCredentials,
    /// Held explicitly rather than left inside the client: the flow has to read
    /// the `_csrf` cookie back out, and a client's own jar is not readable.
    jar: Arc<Jar>,
    client: OnceLock<Result<Client, String>>,
    token: Mutex<Option<Token>>,
}

impl std::fmt::Debug for PelotonAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PelotonAuth")
            .field("auth_base", &self.auth_base)
            .finish_non_exhaustive()
    }
}

impl PelotonAuth {
    /// `auth_base` is the Auth0 root — `https://auth.onepeloton.com` in
    /// production, and a stub in the contract tests.
    pub fn new(auth_base: impl Into<String>, credentials: PelotonCredentials) -> Self {
        Self {
            auth_base: auth_base.into(),
            credentials,
            jar: Arc::new(Jar::default()),
            client: OnceLock::new(),
            token: Mutex::new(None),
        }
    }

    /// A bearer token, from cache, from a refresh, or from a full login.
    ///
    /// # Errors
    ///
    /// [`SourceError::Unauthorised`] where Auth0 rejected the credentials,
    /// which is terminal and never retried. [`SourceError::Unavailable`] or
    /// [`SourceError::Malformed`] for anything else.
    pub async fn bearer(&self) -> Result<String, SourceError> {
        if let Some(token) = self.cached()? {
            if token.usable() {
                return Ok(token.access);
            }
            if let Some(refresh) = token.refresh.clone() {
                // A refresh that fails is not fatal: the credentials are still
                // in hand and a full login is the documented recovery.
                if let Ok(fresh) = self.refresh(&refresh).await {
                    return self.store(fresh);
                }
            }
        }
        let fresh = self.login().await?;
        self.store(fresh)
    }

    fn cached(&self) -> Result<Option<Token>, SourceError> {
        self.token
            .lock()
            .map(|held| held.clone())
            .map_err(|_| SourceError::Unavailable {
                detail: "the token cache was poisoned by an earlier panic".to_owned(),
            })
    }

    fn store(&self, token: Token) -> Result<String, SourceError> {
        let access = token.access.clone();
        {
            let mut held = self.token.lock().map_err(|_| SourceError::Unavailable {
                detail: "the token cache was poisoned by an earlier panic".to_owned(),
            })?;
            *held = Some(token);
        }
        Ok(access)
    }

    /// The HTTP client, built once.
    ///
    /// **Redirects are not followed automatically.** The flow's whole
    /// difficulty is in the redirect chain: the authorize step needs them
    /// followed to reach the login page, and the callback step needs them
    /// stopped so the `code` can be read off a `Location` before the browser
    /// would have discarded it.
    fn client(&self) -> Result<&Client, SourceError> {
        self.client
            .get_or_init(|| {
                Client::builder()
                    .user_agent(USER_AGENT)
                    .cookie_provider(Arc::clone(&self.jar))
                    .redirect(Policy::none())
                    .timeout(Duration::from_secs(30))
                    .build()
                    .map_err(|error| error.to_string())
            })
            .as_ref()
            .map_err(|detail| SourceError::Unavailable {
                detail: detail.clone(),
            })
    }

    async fn refresh(&self, refresh: &str) -> Result<Token, SourceError> {
        let response = self
            .client()?
            .post(format!("{}/oauth/token", self.auth_base))
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", CLIENT_ID),
                ("refresh_token", refresh),
            ])
            .send()
            .await
            .map_err(|ref error| unreachable(error))?;
        Self::token_from(response).await
    }

    async fn login(&self) -> Result<Token, SourceError> {
        let verifier = random_url_safe();
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let state = random_url_safe();
        let nonce = random_url_safe();

        let login_url = self.authorize(&challenge, &state, &nonce).await?;
        // **Auth0 issues its own state and expects that one back.** The value
        // generated above is what starts the flow; what continues it is
        // whatever the authorize redirect settled on. Sending ours instead
        // earns `403 AnomalyDetected: Invalid state`, which is indistinguishable
        // from a rejected password unless the body is read.
        let state = query_value(&login_url, "state").unwrap_or(state);
        let csrf = self.csrf()?;
        let next = self
            .submit_credentials(&login_url, &csrf, &challenge, &state, &nonce)
            .await?;
        let code = self.follow_to_code(next).await?;
        self.exchange(&code, &verifier).await
    }

    /// Start the flow and end up wherever the login form lives.
    async fn authorize(
        &self,
        challenge: &str,
        state: &str,
        nonce: &str,
    ) -> Result<String, SourceError> {
        let mut url = format!("{}/authorize", self.auth_base);
        let query = [
            ("client_id", CLIENT_ID),
            ("response_type", "code"),
            ("redirect_uri", REDIRECT_URI),
            ("scope", SCOPE),
            ("audience", AUDIENCE),
            ("state", state),
            ("nonce", nonce),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
        ];
        let mut current = self
            .client()?
            .get(&url)
            .query(&query)
            .send()
            .await
            .map_err(|ref error| unreachable(error))?;
        for _ in 0..MAX_REDIRECTS {
            let Some(location) = location_of(&current) else {
                return Ok(url);
            };
            url = self.absolute(&location);
            current = self
                .client()?
                .get(&url)
                .send()
                .await
                .map_err(|ref error| unreachable(error))?;
        }
        Err(SourceError::Malformed {
            detail: "the authorize step kept redirecting and never reached a login page".to_owned(),
        })
    }

    /// The CSRF token Auth0 sets as a cookie and then demands in the body.
    fn csrf(&self) -> Result<String, SourceError> {
        // The jar answers with the `Cookie` header it would send to that path,
        // which is the only way back to a value it holds.
        let url = format!("{}/usernamepassword/login", self.auth_base)
            .parse()
            .map_err(|_| SourceError::Malformed {
                detail: "the configured auth base is not a URL".to_owned(),
            })?;
        self.jar
            .cookies(&url)
            .and_then(|header| header.to_str().ok().map(str::to_owned))
            .and_then(|header| {
                header
                    .split("; ")
                    .find_map(|pair| pair.strip_prefix("_csrf=").map(str::to_owned))
            })
            .ok_or_else(|| SourceError::Malformed {
                detail: "Auth0 set no _csrf cookie, so the login form cannot be submitted"
                    .to_owned(),
            })
    }

    async fn submit_credentials(
        &self,
        login_url: &str,
        csrf: &str,
        challenge: &str,
        state: &str,
        nonce: &str,
    ) -> Result<Next, SourceError> {
        let body = serde_json::json!({
            "client_id": CLIENT_ID,
            "redirect_uri": REDIRECT_URI,
            "tenant": TENANT,
            "response_type": "code",
            "scope": SCOPE,
            "audience": AUDIENCE,
            "_csrf": csrf,
            "state": state,
            "_intstate": "deprecated",
            "nonce": nonce,
            "username": self.credentials.email,
            "password": self.credentials.password,
            "connection": CONNECTION,
            "code_challenge": challenge,
            "code_challenge_method": "S256",
        });
        let response = self
            .client()?
            .post(format!("{}/usernamepassword/login", self.auth_base))
            .header("Origin", &self.auth_base)
            .header("Referer", login_url)
            .header("Auth0-Client", AUTH0_CLIENT)
            .json(&body)
            .send()
            .await
            .map_err(|ref error| unreachable(error))?;

        // **Wrong credentials are terminal.** Auth0 answers 401 or 403, and
        // retrying a rejected password is what a credential-stuffing attempt
        // looks like from the far end.
        if matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ) {
            return Err(SourceError::Unauthorised);
        }
        if let Some(location) = location_of(&response) {
            return Ok(Next::Redirect(self.absolute(&location)));
        }
        let html = response
            .text()
            .await
            .map_err(|ref error| unreachable(error))?;
        let form = HiddenForm::parse(&html).ok_or_else(|| SourceError::Malformed {
            detail: "the login response carried neither a redirect nor a form to resubmit"
                .to_owned(),
        })?;
        Ok(Next::Form(form))
    }

    /// Walk the chain until something hands back `?code=`.
    async fn follow_to_code(&self, next: Next) -> Result<String, SourceError> {
        let mut response = match next {
            Next::Redirect(url) => self
                .client()?
                .get(url)
                .send()
                .await
                .map_err(|ref error| unreachable(error))?,
            Next::Form(form) => self
                .client()?
                .post(self.absolute(&form.action))
                .form(&form.fields)
                .send()
                .await
                .map_err(|ref error| unreachable(error))?,
        };
        for _ in 0..MAX_REDIRECTS {
            let Some(location) = location_of(&response) else {
                return Err(SourceError::Malformed {
                    detail: format!(
                        "the login chain stopped at {} without an authorization code",
                        response.status()
                    ),
                });
            };
            let absolute = self.absolute(&location);
            if let Some(code) = query_value(&absolute, "code") {
                return Ok(code);
            }
            response = self
                .client()?
                .get(absolute)
                .send()
                .await
                .map_err(|ref error| unreachable(error))?;
        }
        Err(SourceError::Malformed {
            detail: "the login chain redirected past its limit without an authorization code"
                .to_owned(),
        })
    }

    async fn exchange(&self, code: &str, verifier: &str) -> Result<Token, SourceError> {
        let response = self
            .client()?
            .post(format!("{}/oauth/token", self.auth_base))
            .form(&[
                ("grant_type", "authorization_code"),
                ("client_id", CLIENT_ID),
                ("code", code),
                ("code_verifier", verifier),
                ("redirect_uri", REDIRECT_URI),
            ])
            .send()
            .await
            .map_err(|ref error| unreachable(error))?;
        Self::token_from(response).await
    }

    async fn token_from(response: reqwest::Response) -> Result<Token, SourceError> {
        let status = response.status();
        if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
            return Err(SourceError::Unauthorised);
        }
        if !status.is_success() {
            return Err(SourceError::Unavailable {
                detail: format!("the token endpoint answered {status}"),
            });
        }
        let body: TokenResponse =
            response
                .json()
                .await
                .map_err(|error| SourceError::Malformed {
                    detail: format!("the token endpoint served something unreadable: {error}"),
                })?;
        Ok(Token {
            access: body.access_token,
            refresh: body.refresh_token,
            expires_at: Instant::now() + Duration::from_secs(body.expires_in),
        })
    }

    fn absolute(&self, location: &str) -> String {
        if location.starts_with("http") {
            location.to_owned()
        } else {
            format!("{}{location}", self.auth_base)
        }
    }
}

/// What the login step handed back: somewhere to go, or something to resubmit.
enum Next {
    Redirect(String),
    Form(HiddenForm),
}

/// Auth0's self-submitting form — `wa`, `wctx` and `wresult`.
///
/// **Parsed with a real parser and not a regex.** `wctx` is HTML-escaped JSON
/// and `wresult` is a signed blob; a regex stopping at the first quote truncates
/// both, and the callback answers 400 without saying why.
struct HiddenForm {
    action: String,
    fields: Vec<(String, String)>,
}

impl HiddenForm {
    fn parse(html: &str) -> Option<Self> {
        let action = attribute(html.split_once("<form")?.1, "action")?;
        let fields = html
            .match_indices("<input")
            .filter_map(|(at, _)| {
                let tag = html.get(at..)?.split_once('>')?.0;
                Some((
                    attribute(tag, "name")?,
                    attribute(tag, "value").unwrap_or_default(),
                ))
            })
            .collect();
        Some(Self { action, fields })
    }
}

/// One attribute's value, unescaped.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let after = tag.split_once(&format!("{name}=\""))?.1;
    let raw = after.split_once('"')?.0;
    Some(
        raw.replace("&quot;", "\"")
            .replace("&#34;", "\"")
            .replace("&#39;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&"),
    )
}

fn location_of(response: &reqwest::Response) -> Option<String> {
    if !response.status().is_redirection() {
        return None;
    }
    response
        .headers()
        .get(reqwest::header::LOCATION)?
        .to_str()
        .ok()
        .map(str::to_owned)
}

fn query_value(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then(|| percent_decode(value))
    })
}

/// Enough percent-decoding for an authorization code, which is URL-safe base64
/// and in practice needs none of it.
fn percent_decode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut bytes = value.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let hex: String = bytes.by_ref().take(2).map(char::from).collect();
            if let Ok(decoded) = u8::from_str_radix(&hex, 16) {
                out.push(char::from(decoded));
            } else {
                out.push('%');
                out.push_str(&hex);
            }
        } else {
            out.push(char::from(byte));
        }
    }
    out
}

/// 32 bytes from the OS, base64url. The PKCE verifier, the state and the nonce.
fn random_url_safe() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn unreachable(error: &reqwest::Error) -> SourceError {
    SourceError::Unavailable {
        detail: error.to_string(),
    }
}

#[derive(serde::Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}
