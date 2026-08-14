//! What the operator supplies, and where it comes from.
//!
//! Nothing is compiled in: the database path, the base URL and the credential
//! are all configuration, so moving between machines needs no code change.
//! Nor is anything named after one source — the variables come from
//! [`crate::catalogue`], which derives them from whichever source an
//! invocation names.

use std::{env::VarError, path::PathBuf};

use domain::gym::OperatorZone;

use crate::catalogue::KnownStream;

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

    #[error("no database path: pass --database or set FITNESS_TRACKER_DATABASE")]
    MissingDatabase,

    #[error(
        "no time zone: set FITNESS_TRACKER_TIMEZONE to an IANA identifier such as Europe/London. \
         Nothing is compiled in, because a default would be an assumption about where you train \
         — silently right here and silently wrong elsewhere"
    )]
    MissingTimeZone,

    #[error("FITNESS_TRACKER_TIMEZONE is set to {value:?}, which is not an IANA identifier")]
    UnknownTimeZone { value: String },
}

/// The zone the operator declares they train in.
///
/// § II.3 takes it from configuration, and § 34 forbids an environment
/// assumption — so there is no default. A compiled-in `Europe/London` would be
/// correct for this account and wrong for the next, and because it would be
/// correct here no test would ever catch it.
///
/// It is deliberately not a flag. A zone is a declared interpretive parameter,
/// not a per-invocation choice, and an operator who can pass it per run can
/// produce two derivations that disagree.
///
/// # Errors
///
/// [`ConfigError`] if it is unset or is not an identifier the database knows.
pub fn timezone(declared: Result<String, VarError>) -> Result<OperatorZone, ConfigError> {
    let value = match declared {
        Ok(value) if !value.trim().is_empty() => value,
        _ => return Err(ConfigError::MissingTimeZone),
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
        known: &KnownStream,
        base_url: String,
        api_key: Result<String, VarError>,
    ) -> Result<Self, ConfigError> {
        let api_key = match api_key {
            Ok(key) if !key.trim().is_empty() => key,
            Ok(_) | Err(VarError::NotPresent) => {
                return Err(ConfigError::MissingApiKey {
                    variable: known.api_key_variable(),
                    credential_url: known.credential_url(),
                });
            }
            Err(VarError::NotUnicode(_)) => {
                return Err(ConfigError::UnreadableApiKey {
                    variable: known.api_key_variable(),
                });
            }
        };

        Ok(Self { base_url, api_key })
    }
}

/// # Errors
///
/// [`ConfigError::MissingDatabase`] if no path was given. There is no default:
/// a store that appears wherever the command was run is worse than one that
/// has to be named.
pub fn database(path: Option<PathBuf>) -> Result<PathBuf, ConfigError> {
    path.ok_or(ConfigError::MissingDatabase)
}

#[cfg(test)]
mod tests {
    use super::{ConfigError, SourceAccess, database};
    use crate::catalogue::{KnownStream, lookup};
    use std::{env::VarError, path::PathBuf};


    fn hevy() -> Option<&'static KnownStream> {
        lookup("hevy.workouts")
    }

    #[test]
    fn a_blank_credential_is_treated_as_missing() {
        let known = hevy().expect("hevy.workouts is in the catalogue");
        let resolved = SourceAccess::resolve(
            known,
            "https://example.test".to_owned(),
            Ok("   ".to_owned()),
        );
        assert_eq!(
            resolved.unwrap_err(),
            ConfigError::MissingApiKey {
                variable: "HEVY_API_KEY".to_owned(),
                credential_url: known.credential_url(),
            }
        );
    }

    #[test]
    fn the_database_path_is_required() {
        assert_eq!(database(None).unwrap_err(), ConfigError::MissingDatabase);
        assert_eq!(
            database(Some(PathBuf::from("/tmp/x.db"))).unwrap(),
            PathBuf::from("/tmp/x.db")
        );
    }

    /// The message names the variable the invocation actually needs, which is
    /// derived from the source rather than compiled in.
    #[test]
    fn a_missing_variable_names_itself() {
        let resolved = SourceAccess::resolve(
            hevy().expect("hevy.workouts is in the catalogue"),
            "https://example.test".to_owned(),
            Err(VarError::NotPresent),
        );
        let message = resolved.unwrap_err().to_string();
        assert!(message.contains("HEVY_API_KEY"), "{message}");
        assert!(message.contains("hevy.com/settings"), "{message}");
        assert!(message.contains("never on the command line"), "{message}");
    }
}
