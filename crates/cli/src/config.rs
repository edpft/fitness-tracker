//! What the operator supplies, and where it comes from.
//!
//! Nothing is compiled in: the database path, the base URL and the credential
//! are all configuration, so moving between machines needs no code change.

use std::{env::VarError, path::PathBuf};

/// The credential's variable. Deliberately env-only, with no flag anywhere:
/// a secret passed on the command line lands in shell history and in `ps`
/// output for every other user on the machine.
pub const API_KEY_VARIABLE: &str = "HEVY_API_KEY";

/// Hevy's API root. A default rather than a constant, because the contract
/// tests point this at a local stub.
///
/// The **root**, with no version segment: the endpoint constant carries the
/// full path (`/v1/workouts/events`), which is also what each landing record
/// records as its provenance. Putting `/v1` in both composed
/// `/v1/v1/workouts/events`, which no test caught because the stub's URI has
/// no version segment to double up — only a live run did.
pub const DEFAULT_BASE_URL: &str = "https://api.hevyapp.com";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error(
        "{API_KEY_VARIABLE} is not set. Get a key from https://hevy.com/settings?developer and \
         put it in the environment or an untracked .env — never on the command line"
    )]
    MissingApiKey,

    #[error("{API_KEY_VARIABLE} is set but is not valid text")]
    UnreadableApiKey,

    #[error("no database path: pass --database or set FITNESS_TRACKER_DATABASE")]
    MissingDatabase,
}

/// Everything a command needs from outside itself.
#[derive(Debug, Clone)]
pub struct Config {
    pub database: PathBuf,
    pub base_url: String,
    pub api_key: String,
}

impl Config {
    /// # Errors
    ///
    /// [`ConfigError`] if the database path or the credential is absent.
    pub fn resolve(
        database: Option<PathBuf>,
        base_url: String,
        api_key: Result<String, VarError>,
    ) -> Result<Self, ConfigError> {
        let database = database.ok_or(ConfigError::MissingDatabase)?;
        let api_key = match api_key {
            Ok(key) if !key.trim().is_empty() => key,
            Ok(_) | Err(VarError::NotPresent) => return Err(ConfigError::MissingApiKey),
            Err(VarError::NotUnicode(_)) => return Err(ConfigError::UnreadableApiKey),
        };

        Ok(Self {
            database,
            base_url,
            api_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, ConfigError};
    use std::{env::VarError, path::PathBuf};

    #[test]
    fn a_blank_credential_is_treated_as_missing() {
        let resolved = Config::resolve(
            Some(PathBuf::from("/tmp/x.db")),
            "https://example.test".to_owned(),
            Ok("   ".to_owned()),
        );
        assert_eq!(resolved.unwrap_err(), ConfigError::MissingApiKey);
    }

    #[test]
    fn the_database_path_is_required() {
        let resolved = Config::resolve(
            None,
            "https://example.test".to_owned(),
            Ok("a-key".to_owned()),
        );
        assert_eq!(resolved.unwrap_err(), ConfigError::MissingDatabase);
    }

    #[test]
    fn a_missing_variable_names_where_to_get_a_key() {
        let resolved = Config::resolve(
            Some(PathBuf::from("/tmp/x.db")),
            "https://example.test".to_owned(),
            Err(VarError::NotPresent),
        );
        let message = resolved.unwrap_err().to_string();
        assert!(message.contains("hevy.com/settings"), "{message}");
        assert!(message.contains("never on the command line"), "{message}");
    }
}
