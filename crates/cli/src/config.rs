//! What the operator supplies, and where it comes from.
//!
//! Nothing is compiled in: the database path, the base URL and the credential
//! are all configuration, so moving between machines needs no code change.
//! Nor is anything named after one source — the variables come from
//! [`crate::catalogue`], which derives them from whichever source an
//! invocation names.

use std::env::VarError;

use domain::normalised::OperatorZone;
use jiff::civil::Date;

use crate::catalogue::KnownSource;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    /// **`init` first, the variables second.** The stored credential is the way
    /// this tool is meant to be used, and the environment is the override — so
    /// the message leads with the one that fixes the machine rather than the one
    /// that fixes the shell. It named an untracked `.env` until #61, which
    /// nothing in the binary has ever read: that is direnv loading a file inside
    /// the checkout, which is the one place an operator running the installed
    /// binary is not.
    #[error(
        "no {source_name} credential. Run `fitness init` to store one, or set {variables}. \
         Get one from {credential_url} — never on the command line"
    )]
    MissingCredential {
        source_name: &'static str,
        variables: String,
        credential_url: &'static str,
    },

    /// A key where a login belongs, or the reverse. Worth its own message: the
    /// alternative is a credential that is present, wrong in kind, and reported
    /// as absent.
    #[error(
        "the stored {source_name} credential is {held}, and {source_name} authenticates with \
         {wanted}. \
         Run `fitness init` to replace it"
    )]
    WrongCredentialKind {
        source_name: &'static str,
        held: &'static str,
        wanted: &'static str,
    },

    #[error("{variable} is set but is not valid text")]
    UnreadableVariable { variable: String },

    #[error(transparent)]
    Credentials(#[from] infrastructure::CredentialError),

    #[error(transparent)]
    NoBaseDirectory(#[from] crate::paths::NoBaseDirectory),

    #[error(
        "no time zone: pass --timezone (for example `--timezone Europe/London`), set \
         FITNESS_TRACKER_TIMEZONE, or run `fitness init` to state one once. Nothing is \
         compiled in, because a default would be an assumption about where you train — \
         silently right here and silently wrong elsewhere"
    )]
    MissingTimeZone,

    #[error("{value:?} is not an IANA time zone identifier")]
    UnknownTimeZone { value: String },

    #[error("{value:?} is not a date: {detail}")]
    NotADate { value: String, detail: String },
}

/// A date the operator typed.
///
/// Split out because a named date needs no calendar: since programmes succeed
/// one another (decision 0012), which programme covers that date is a question
/// for the store, and there may be no programme covering *today* to default
/// from at all.
///
/// # Errors
///
/// [`ConfigError::NotADate`] if it is not a civil date.
pub fn named_date(text: &str) -> Result<Date, ConfigError> {
    text.parse::<Date>().map_err(|error| ConfigError::NotADate {
        value: text.to_owned(),
        detail: error.to_string(),
    })
}

/// The zone the operator declares they train in.
///
/// § II.3 takes it from configuration, and § 34 forbids an environment
/// assumption — so there is no default. A compiled-in `Europe/London` would be
/// correct for this account and wrong for the next, and because it would be
/// correct here no test would ever catch it.
///
/// Both values are passed in rather than read here, so this is testable without
/// touching the process environment or the store. Which of the flag and the
/// variable `declared` came from is clap's business.
///
/// # Errors
///
/// [`ConfigError`] if it is unset or is not an identifier the database knows.
pub fn timezone(declared: Option<&str>, stored: Option<&str>) -> Result<OperatorZone, ConfigError> {
    // Flag or variable first — clap has already collapsed those two — then what
    // the operator stated once. A value passed for this invocation beats a value
    // stated for every invocation, which is the only ordering that lets a single
    // run be done from somewhere else.
    let stated = [declared, stored]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty());

    let Some(value) = stated else {
        return Err(ConfigError::MissingTimeZone);
    };

    OperatorZone::try_from(value).map_err(|_| ConfigError::UnknownTimeZone {
        value: value.to_owned(),
    })
}

/// What it takes to reach a source.
///
/// Resolved only for commands that contact one. `status` and `reset` must keep
/// working with no credential and no network: a staleness report that is
/// itself unavailable whenever things go wrong reports nothing worth having.
///
/// **One variant per [`Credential`], and the base URLs sit inside them** rather
/// than beside them, so a composition root cannot reach for an authorisation
/// root that a key-based source has never had.
#[derive(Debug, Clone)]
pub enum SourceAccess {
    ApiKey {
        base_url: String,
        api_key: String,
        origin: Origin,
    },
    EmailPassword {
        base_url: String,
        auth_base_url: String,
        email: String,
        password: String,
        origin: Origin,
    },
}

/// Which of the two answered.
///
/// **Carried rather than worked out again at the point of failure.** A rejected
/// credential is the one moment an operator needs to know which of the file and
/// the environment was used — the failure that prompted #61 was a stored key
/// being silently used by a shell whose owner believed a variable was set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// The named variables were set.
    Environment(String),
    /// The credentials file answered, the environment being silent.
    Stored,
}

impl std::fmt::Display for Origin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Environment(variables) => write!(formatter, "{variables}"),
            Self::Stored => formatter.write_str("the stored credentials"),
        }
    }
}

impl SourceAccess {
    /// Which of the file and the environment supplied this.
    pub const fn origin(&self) -> &Origin {
        match self {
            Self::ApiKey { origin, .. } | Self::EmailPassword { origin, .. } => origin,
        }
    }
}

impl SourceAccess {
    /// Resolve a key-based source.
    ///
    /// **The environment wins and the file answers when it is silent.** That
    /// ordering is the same everywhere else here: a value supplied for this run
    /// beats one stated for every run. What changed with #61 is which of the two
    /// is the ordinary case — the file is now how the tool is set up, and the
    /// variable is the override a test or a one-off run reaches for.
    ///
    /// The credential is passed in rather than read here so that this is
    /// testable without touching the process environment.
    ///
    /// # Errors
    ///
    /// [`ConfigError`] if neither states one, or the stored one is a login.
    pub fn resolve(
        known: &KnownSource,
        base_url: String,
        api_key: Result<String, VarError>,
        stored: Option<&infrastructure::Credential>,
    ) -> Result<Self, ConfigError> {
        let variable = known.api_key_variable();
        if let Some(api_key) = from_environment(api_key, &variable)? {
            return Ok(Self::ApiKey {
                base_url,
                api_key,
                origin: Origin::Environment(variable),
            });
        }

        match stored {
            Some(infrastructure::Credential::ApiKey { key }) if !key.trim().is_empty() => {
                Ok(Self::ApiKey {
                    base_url,
                    api_key: key.clone(),
                    origin: Origin::Stored,
                })
            }
            Some(infrastructure::Credential::Login { .. }) => {
                Err(ConfigError::WrongCredentialKind {
                    source_name: known.name(),
                    held: "a login",
                    wanted: "a key",
                })
            }
            _ => Err(missing(known)),
        }
    }

    /// Resolve a login-based source.
    ///
    /// **Both halves come from the same place.** A login half-answered by the
    /// environment is not an override of the stored one — it is a shell that
    /// exported one of two variables, and quietly pairing it with a stored
    /// password would authenticate as somebody the operator did not name.
    ///
    /// # Errors
    ///
    /// [`ConfigError`] if neither states one, or the stored one is a key.
    pub fn resolve_login(
        known: &KnownSource,
        base_url: String,
        auth_base_url: String,
        email: Result<String, VarError>,
        password: Result<String, VarError>,
        stored: Option<&infrastructure::Credential>,
    ) -> Result<Self, ConfigError> {
        let email_variable = known.email_variable();
        let password_variable = known.password_variable();
        let from_env = (
            from_environment(email, &email_variable)?,
            from_environment(password, &password_variable)?,
        );

        if let (Some(email), Some(password)) = from_env {
            return Ok(Self::EmailPassword {
                base_url,
                auth_base_url,
                email,
                password,
                origin: Origin::Environment(format!("{email_variable} and {password_variable}")),
            });
        }

        match stored {
            Some(infrastructure::Credential::Login { email, password })
                if !email.trim().is_empty() && !password.trim().is_empty() =>
            {
                Ok(Self::EmailPassword {
                    base_url,
                    auth_base_url,
                    email: email.clone(),
                    password: password.clone(),
                    origin: Origin::Stored,
                })
            }
            Some(infrastructure::Credential::ApiKey { .. }) => {
                Err(ConfigError::WrongCredentialKind {
                    source_name: known.name(),
                    held: "a key",
                    wanted: "a login",
                })
            }
            _ => Err(missing(known)),
        }
    }
}

/// What a source needs, when nothing supplied it.
fn missing(known: &KnownSource) -> ConfigError {
    ConfigError::MissingCredential {
        source_name: known.name(),
        variables: known.required_variables().join(" and "),
        credential_url: known.credential_url(),
    }
}

/// One variable, if it is set to something other than blank.
///
/// **Blank is unset.** `VAR=` in a profile is the shape an operator leaves
/// behind after clearing one, and treating it as a credential sends an empty
/// string to a source that will reject it.
fn from_environment(
    value: Result<String, VarError>,
    variable: &str,
) -> Result<Option<String>, ConfigError> {
    match value {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
        Ok(_) | Err(VarError::NotPresent) => Ok(None),
        Err(VarError::NotUnicode(_)) => Err(ConfigError::UnreadableVariable {
            variable: variable.to_owned(),
        }),
    }
}

// **Where the store lives is not a setting**, and cannot be: it is what has to
// be known before the settings can be read at all. It stays the flag, the
// variable clap folds into it, and the specification's default — which is why
// there is no function here for it.

#[cfg(test)]
mod tests {
    use super::{ConfigError, Origin, SourceAccess, timezone};
    use crate::catalogue::{KnownSource, source};
    use infrastructure::Credential;
    use std::env::VarError;

    fn hevy() -> Option<&'static KnownSource> {
        source("hevy")
    }

    fn peloton() -> Option<&'static KnownSource> {
        source("peloton")
    }

    fn key(key: &str) -> Credential {
        Credential::ApiKey {
            key: key.to_owned(),
        }
    }

    fn login(email: &str, password: &str) -> Credential {
        Credential::Login {
            email: email.to_owned(),
            password: password.to_owned(),
        }
    }

    /// The key a resolution produced, for tests that assert on it.
    fn key_of(access: &SourceAccess) -> &str {
        match access {
            SourceAccess::ApiKey { api_key, .. } => api_key,
            SourceAccess::EmailPassword { .. } => "<a login, not a key>",
        }
    }

    #[test]
    fn a_blank_variable_falls_through_to_the_stored_credential() {
        let known = hevy().expect("hevy.workouts is in the catalogue");
        let resolved = SourceAccess::resolve(
            known,
            "https://example.test".to_owned(),
            Ok("   ".to_owned()),
            Some(&key("from-the-file")),
        )
        .expect("the file answers");

        assert_eq!(key_of(&resolved), "from-the-file");
        assert_eq!(resolved.origin(), &Origin::Stored);
    }

    #[test]
    fn nothing_anywhere_is_refused_with_both_ways_to_fix_it() {
        let known = hevy().expect("hevy.workouts is in the catalogue");
        let refused = SourceAccess::resolve(
            known,
            "https://example.test".to_owned(),
            Err(VarError::NotPresent),
            None,
        );
        let message = refused.expect_err("nothing states a key").to_string();

        assert!(message.contains("fitness init"), "{message}");
        assert!(message.contains("HEVY_API_KEY"), "{message}");
        assert!(message.contains("hevy.com/settings"), "{message}");
        assert!(message.contains("never on the command line"), "{message}");
    }

    /// **Nothing in the binary has ever read a `.env`**, and the message said
    /// to use one until #61. That is direnv, inside the checkout.
    #[test]
    fn no_message_recommends_a_dotenv_file() {
        let known = hevy().expect("hevy.workouts is in the catalogue");
        let message = SourceAccess::resolve(
            known,
            "https://example.test".to_owned(),
            Err(VarError::NotPresent),
            None,
        )
        .expect_err("nothing states a key")
        .to_string();

        assert!(!message.contains(".env"), "{message}");
    }

    /// The environment stays an override, and says so when it is the one used.
    #[test]
    fn the_environment_beats_the_stored_credential() {
        let known = hevy().expect("hevy is a known source");

        let from_environment = SourceAccess::resolve(
            known,
            "https://example.test".to_owned(),
            Ok("from-the-environment".to_owned()),
            Some(&key("from-the-file")),
        )
        .expect("a key is available");

        assert_eq!(key_of(&from_environment), "from-the-environment");
        assert_eq!(
            from_environment.origin(),
            &Origin::Environment("HEVY_API_KEY".to_owned())
        );
    }

    /// A credential of the wrong kind is present and unusable, which is a
    /// different thing from absent.
    #[test]
    fn a_login_stored_for_a_key_source_is_reported_as_such() {
        let known = hevy().expect("hevy is a known source");
        let refused = SourceAccess::resolve(
            known,
            "https://example.test".to_owned(),
            Err(VarError::NotPresent),
            Some(&login("someone@example.test", "a-password")),
        );

        assert_eq!(
            refused.expect_err("a login is not a key"),
            ConfigError::WrongCredentialKind {
                source_name: "hevy",
                held: "a login",
                wanted: "a key",
            }
        );
    }

    #[test]
    fn a_login_resolves_from_the_file_when_the_variables_are_silent() {
        let known = peloton().expect("peloton is a known source");
        let resolved = SourceAccess::resolve_login(
            known,
            "https://api.example.test".to_owned(),
            "https://auth.example.test".to_owned(),
            Err(VarError::NotPresent),
            Err(VarError::NotPresent),
            Some(&login("someone@example.test", "a-password")),
        )
        .expect("the file answers");

        match resolved {
            SourceAccess::EmailPassword { email, origin, .. } => {
                assert_eq!(email, "someone@example.test");
                assert_eq!(origin, Origin::Stored);
            }
            other @ SourceAccess::ApiKey { .. } => panic!("a login resolved as {other:?}"),
        }
    }

    /// **Half a login is not an override.** Pairing an exported email with a
    /// stored password would authenticate as somebody nobody named.
    #[test]
    fn half_a_login_in_the_environment_does_not_borrow_the_other_half() {
        let known = peloton().expect("peloton is a known source");
        let resolved = SourceAccess::resolve_login(
            known,
            "https://api.example.test".to_owned(),
            "https://auth.example.test".to_owned(),
            Ok("someone-else@example.test".to_owned()),
            Err(VarError::NotPresent),
            Some(&login("someone@example.test", "a-password")),
        )
        .expect("the file answers whole");

        match resolved {
            SourceAccess::EmailPassword { email, origin, .. } => {
                assert_eq!(email, "someone@example.test");
                assert_eq!(origin, Origin::Stored);
            }
            other @ SourceAccess::ApiKey { .. } => panic!("a login resolved as {other:?}"),
        }
    }

    /// The same ordering for the zone, plus the case that reports rather than
    /// guesses.
    #[test]
    fn the_zone_prefers_the_invocation_then_the_store() {
        let from_store = timezone(None, Some("Europe/London")).expect("the store states a zone");
        assert_eq!(from_store.id(), "Europe/London");

        let from_flag = timezone(Some("Pacific/Auckland"), Some("Europe/London"))
            .expect("the flag states a zone");
        assert_eq!(from_flag.id(), "Pacific/Auckland");

        match timezone(None, None) {
            Err(ConfigError::MissingTimeZone) => {}
            other => panic!("nothing stated is refused: {other:?}"),
        }
    }

    /// The message points at the command that states a zone.
    #[test]
    fn a_missing_zone_names_the_command_that_states_one() {
        let message = timezone(None, None)
            .expect_err("nothing states a zone")
            .to_string();

        assert!(message.contains("fitness init"), "{message}");
    }
}
