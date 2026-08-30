//! What the operator supplies, and where it comes from.
//!
//! Nothing is compiled in: the database path, the base URL and the credential
//! are all configuration, so moving between machines needs no code change.
//! Nor is anything named after one source — the variables come from
//! [`crate::catalogue`], which derives them from whichever source an
//! invocation names.

use std::{
    env::VarError,
    path::{Path, PathBuf},
};

use domain::gym::OperatorZone;
use jiff::civil::Date;

use infrastructure::Settings;

use crate::catalogue::KnownSource;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error(
        "{variable} is not set. Get a key from {credential_url} and put it in the environment \
         or an untracked .env — never on the command line"
    )]
    MissingApiKey {
        variable: String,
        credential_url: &'static str,
    },

    #[error("{variable} is set but is not valid text")]
    UnreadableApiKey { variable: String },

    #[error(transparent)]
    Settings(#[from] infrastructure::SettingsError),

    #[error(transparent)]
    Credentials(#[from] infrastructure::CredentialError),

    #[error(transparent)]
    NoBaseDirectory(#[from] crate::paths::NoBaseDirectory),

    #[error(
        "no time zone: pass --timezone, set FITNESS_TRACKER_TIMEZONE, or put \
         `timezone = \"Europe/London\"` in {path}. Nothing is compiled in, because a default \
         would be an assumption about where you train — silently right here and silently \
         wrong elsewhere"
    )]
    MissingTimeZone { path: String },

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
/// The value is passed in rather than read here, so this is testable without
/// touching the process environment. Which of the flag and the variable it came
/// from is clap's business.
///
/// # Errors
///
/// [`ConfigError`] if it is unset or is not an identifier the database knows.
pub fn timezone(
    declared: Option<&str>,
    settings: &Settings,
    settings_path: &Path,
) -> Result<OperatorZone, ConfigError> {
    // Flag or variable first — clap has already collapsed those two — then what
    // the operator stated once. A value passed for this invocation beats a value
    // stated for every invocation, which is the only ordering that lets a single
    // run be done from somewhere else.
    let stated = declared
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            settings
                .timezone
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        });

    let Some(value) = stated else {
        return Err(ConfigError::MissingTimeZone {
            path: settings_path.display().to_string(),
        });
    };

    OperatorZone::try_from(value.as_str()).map_err(|_| ConfigError::UnknownTimeZone { value })
}

/// What it takes to reach a source.
///
/// Resolved only for commands that contact one. `status` and `reset` must keep
/// working with no credential and no network: a staleness report that is
/// itself unavailable whenever things go wrong reports nothing worth having.
#[derive(Debug, Clone)]
pub struct SourceAccess {
    pub base_url: String,
    pub api_key: String,
}

impl SourceAccess {
    /// The credential is passed in rather than read here so that this is
    /// testable without touching the process environment.
    ///
    /// # Errors
    ///
    /// [`ConfigError`] if the credential is absent or unreadable.
    pub fn resolve(
        known: &KnownSource,
        base_url: String,
        api_key: Result<String, VarError>,
        stored: Option<&str>,
    ) -> Result<Self, ConfigError> {
        let api_key = match api_key {
            Ok(key) if !key.trim().is_empty() => key,
            // **The environment wins, and the file answers when it is silent.**
            // The ordering is the same everywhere else here: a value supplied for
            // this run beats one stated for every run.
            Ok(_) | Err(VarError::NotPresent) => match stored {
                Some(key) if !key.trim().is_empty() => key.to_owned(),
                _ => {
                    return Err(ConfigError::MissingApiKey {
                        variable: known.api_key_variable(),
                        credential_url: known.credential_url(),
                    });
                }
            },
            Err(VarError::NotUnicode(_)) => {
                return Err(ConfigError::UnreadableApiKey {
                    variable: known.api_key_variable(),
                });
            }
        };

        Ok(Self { base_url, api_key })
    }
}

/// Where the store lives, if anything has said.
///
/// **Returns `None` rather than resolving a default here**, so the fallback is
/// worked out only when it is actually needed. A machine that passes the path
/// explicitly should not have to have a home directory for the sake of a value
/// nothing reads.
pub fn database(stated: Option<PathBuf>, settings: &Settings) -> Option<PathBuf> {
    stated.or_else(|| settings.database.clone())
}

#[cfg(test)]
mod tests {
    use super::{ConfigError, Settings, SourceAccess, database, timezone};
    use crate::catalogue::{KnownSource, source};
    use std::{env::VarError, path::PathBuf};

    fn hevy() -> Option<&'static KnownSource> {
        source("hevy")
    }

    #[test]
    fn a_blank_credential_is_treated_as_missing() {
        let known = hevy().expect("hevy.workouts is in the catalogue");
        let resolved = SourceAccess::resolve(
            known,
            "https://example.test".to_owned(),
            Ok("   ".to_owned()),
            None,
        );
        assert_eq!(
            resolved.unwrap_err(),
            ConfigError::MissingApiKey {
                variable: "HEVY_API_KEY".to_owned(),
                credential_url: known.credential_url(),
            }
        );
    }

    /// **A value passed for this invocation beats one stated for every
    /// invocation**, and nothing stated at all leaves the caller to work out the
    /// default — which is what keeps it from being resolved on a machine that
    /// never needed it.
    #[test]
    fn the_stated_path_wins_and_silence_defers() {
        let stated = Settings {
            timezone: None,
            database: Some(PathBuf::from("/stated/store.db")),
        };

        assert_eq!(
            database(Some(PathBuf::from("/passed/store.db")), &stated),
            Some(PathBuf::from("/passed/store.db")),
            "the flag beats the file"
        );
        assert_eq!(
            database(None, &stated),
            Some(PathBuf::from("/stated/store.db")),
            "the file answers when the flag does not"
        );
        assert_eq!(
            database(None, &Settings::default()),
            None,
            "and silence defers to the caller's default"
        );
    }

    /// The same ordering for the zone, plus the case that reports rather than
    /// guesses.
    #[test]
    fn the_zone_prefers_the_invocation_then_the_file() {
        let stated = Settings {
            timezone: Some("Europe/London".to_owned()),
            database: None,
        };
        let path = PathBuf::from("/config/fitness-tracker/config.toml");

        let from_file = timezone(None, &stated, &path).expect("the file states a zone");
        assert_eq!(from_file.id(), "Europe/London");

        let from_flag =
            timezone(Some("Pacific/Auckland"), &stated, &path).expect("the flag states a zone");
        assert_eq!(from_flag.id(), "Pacific/Auckland");

        let refused = timezone(None, &Settings::default(), &path);
        match refused {
            Err(ConfigError::MissingTimeZone { path: named }) => {
                assert!(named.contains("config.toml"), "{named}");
            }
            other => panic!("nothing stated is refused with the path: {other:?}"),
        }
    }

    /// The message names the variable the invocation actually needs, which is
    /// derived from the source rather than compiled in.
    #[test]
    fn a_missing_variable_names_itself() {
        let resolved = SourceAccess::resolve(
            hevy().expect("hevy.workouts is in the catalogue"),
            "https://example.test".to_owned(),
            Err(VarError::NotPresent),
            None,
        );
        let message = resolved.unwrap_err().to_string();
        assert!(message.contains("HEVY_API_KEY"), "{message}");
        assert!(message.contains("hevy.com/settings"), "{message}");
        assert!(message.contains("never on the command line"), "{message}");
    }

    /// **The environment wins and the file answers when it is silent**, which is
    /// the same ordering every other setting here uses.
    #[test]
    fn the_environment_beats_the_stored_key() {
        let known = hevy().expect("hevy is a known source");

        let from_environment = SourceAccess::resolve(
            known,
            "https://example.test".to_owned(),
            Ok("from-the-environment".to_owned()),
            Some("from-the-file"),
        )
        .expect("a key is available");
        assert_eq!(from_environment.api_key, "from-the-environment");

        let from_file = SourceAccess::resolve(
            known,
            "https://example.test".to_owned(),
            Err(VarError::NotPresent),
            Some("from-the-file"),
        )
        .expect("a key is available");
        assert_eq!(from_file.api_key, "from-the-file");
    }

    /// A blank stored key is no more a key than a blank variable is.
    #[test]
    fn a_blank_stored_key_is_treated_as_missing() {
        let known = hevy().expect("hevy is a known source");
        let resolved = SourceAccess::resolve(
            known,
            "https://example.test".to_owned(),
            Err(VarError::NotPresent),
            Some("  "),
        );
        assert!(resolved.is_err());
    }
}
