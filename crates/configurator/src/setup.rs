//! Checking a credential against its source, before it is stored.
//!
//! **An arm per source, like [`crate::wiring`]'s**, and for the same reason:
//! which adapter can answer for a credential is a fact about the source, not
//! about the shape of the credential. A build that grows a second key-based
//! source adds an arm here rather than inheriting Hevy's client by accident.
//!
//! Asking for the credential, and anything else a person has to type, is the
//! frontend's.

use infrastructure::{
    CredentialDigest, Credentials, GarminCredentials, HevyWorkoutEvents, TokenFile, WithingsAuth,
    WithingsClient, peloton::auth::PelotonCredentials,
};

use crate::{
    catalogue::{Credential as Kind, KnownSource},
    config::ConfigError,
    paths,
};

/// One source's credential, as `fitness credentials` stores it.
pub use infrastructure::Credential;

/// Every variable a source needs, if the environment sets them all.
#[must_use]
pub fn from_environment(known: &KnownSource) -> Option<Vec<String>> {
    known
        .required_variables()
        .iter()
        .map(|variable| {
            std::env::var(variable)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .collect()
}

/// Whether the application a source is reached through has been granted
/// access.
///
/// **A token this registration did not obtain is not access** (#197). Asking
/// only whether the file is there would report a source connected on a token
/// from an application the operator has since replaced, which the first request
/// would be refused on.
///
/// # Errors
///
/// [`ConfigError`] if no base directory can be worked out for the token.
pub fn granted(known: &KnownSource, stored: &Credentials) -> Result<bool, ConfigError> {
    let Some(registration) = registration(known, stored) else {
        return Ok(false);
    };
    Ok(token_file(known, registration.digest())?.read().is_some())
}

/// The registration a source would be reached with, from wherever it is stated.
///
/// The environment first, then the store: `SourceAccess::resolve` orders them
/// that way and a second order here would be a second answer to the same
/// question.
fn registration(known: &KnownSource, stored: &Credentials) -> Option<WithingsClient> {
    if let Some(values) = from_environment(known)
        && let [client_id, client_secret, redirect_uri] = values.as_slice()
    {
        return Some(WithingsClient::new(client_id, client_secret, redirect_uri));
    }
    match stored.credential(known.name()) {
        Some(Credential::OAuthClient {
            client_id,
            client_secret,
            redirect_uri,
        }) => Some(WithingsClient::new(
            client_id.clone(),
            client_secret.clone(),
            redirect_uri.clone(),
        )),
        _ => None,
    }
}

fn token_file(
    known: &KnownSource,
    obtained_with: CredentialDigest,
) -> Result<TokenFile, ConfigError> {
    Ok(paths::token(&paths::SystemEnvironment, known.name())
        .map(|path| TokenFile::new(path, obtained_with))?)
}

/// Offer a credential to its source and see whether it is accepted.
///
/// **An application is signed in to, not checked.** `paste` is shown the
/// address to open and hands back the address the browser landed on; nothing
/// else here waits on a person.
///
/// # Errors
///
/// What the source said, if it refused the credential or could not be reached.
pub async fn verify(
    known: &KnownSource,
    credential: &Credential,
    paste: impl FnOnce(&str) -> Result<String, String>,
) -> Result<(), String> {
    let base_url = std::env::var(known.base_url_variable())
        .unwrap_or_else(|_| known.default_base_url().to_owned());
    let auth_base_url = |default: &str| {
        std::env::var(known.auth_base_url_variable()).unwrap_or_else(|_| default.to_owned())
    };

    match (known.name(), credential, known.credential()) {
        ("hevy", Credential::ApiKey { key }, _) => HevyWorkoutEvents::new(base_url, key.clone())
            .verify()
            .await
            .map_err(|error| error.to_string()),
        (
            "peloton",
            Credential::Login { email, password },
            Kind::EmailPassword {
                default_auth_base_url,
            },
        ) => crate::wiring::peloton_auth(
            auth_base_url(default_auth_base_url),
            PelotonCredentials::new(email.clone(), password.clone()),
        )
        .bearer()
        .await
        .map(|_| ())
        .map_err(|error| error.to_string()),
        // **Garmin's own arm, though it shares Peloton's credential kind.**
        // Which adapter answers for a login is a fact about the source: these
        // two both take an email and a password and neither can check the
        // other's. Garmin also needs two hosts rather than one, and the second
        // is derived from the first so that a stub reaches both.
        //
        // **Through the one constructor every Garmin run uses** (#377), so
        // the token a successful check earns is the one the next run reads,
        // rather than a sign-in thrown away.
        (
            "garmin",
            Credential::Login { email, password },
            Kind::EmailPassword {
                default_auth_base_url,
            },
        ) => crate::wiring::garmin_auth(
            &auth_base_url(default_auth_base_url),
            GarminCredentials::new(email.clone(), password.clone()),
        )
        .bearer()
        .await
        .map(|_| ())
        .map_err(|error| error.to_string()),
        (
            "withings",
            Credential::OAuthClient {
                client_id,
                client_secret,
                redirect_uri,
            },
            Kind::OAuthClient {
                default_auth_base_url,
            },
        ) => {
            let registration = WithingsClient::new(
                client_id.clone(),
                client_secret.clone(),
                redirect_uri.clone(),
            );
            let cache =
                token_file(known, registration.digest()).map_err(|error| error.to_string())?;
            let auth = WithingsAuth::new(
                base_url,
                auth_base_url(default_auth_base_url),
                registration,
                cache,
            );
            let state = WithingsAuth::new_state();
            let address = auth
                .authorisation_url(&state)
                .map_err(|error| error.to_string())?;
            let pasted = paste(address.as_str())?;
            auth.sign_in(&pasted, &state)
                .await
                .map_err(|error| error.to_string())
        }
        (name, _, _) => Err(format!("this build cannot check a {name} credential")),
    }
}
