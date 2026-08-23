//! What the operator states once rather than on every invocation.
//!
//! An adapter, and here for the reason [`crate::programme::document`] is: TOML
//! is an interface language kept at its adapter (§ 21), and the exemption is
//! only honest while nothing above this ring can name a `toml` type. The check
//! in the flake enforces exactly that.
//!
//! **This reads settings; it does not decide them.** Which of a flag, an
//! environment variable and a file wins is a composition question and belongs to
//! the ring above. So every field here is optional and an absent file is an
//! empty answer rather than an error — "the operator has said nothing" is a
//! perfectly good state on a machine where everything is passed explicitly.
//!
//! **No credential lives here.** § 35 allows environment or local config, and
//! the environment is what [`crate::hevy`]'s callers already require: a key in a
//! file is a key that gets committed by someone tidying their dotfiles, and the
//! file this reads is meant to be exactly that kind of thing.

use std::path::{Path, PathBuf};

/// Why a settings file could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    #[error("{path} could not be read: {detail}")]
    Unreadable { path: String, detail: String },

    #[error("{path} is not valid TOML: {detail}")]
    Malformed { path: String, detail: String },
}

/// What the settings file says, as it says it.
///
/// Values are unparsed on purpose. A zone that will not resolve should be
/// reported the same way whether it came from a flag, the environment or here,
/// and that means one place doing the parsing rather than three.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// The IANA identifier the operator trains in.
    pub timezone: Option<String>,
    /// Where the store lives, for an operator who wants it somewhere other than
    /// the default.
    pub database: Option<PathBuf>,
}

impl Settings {
    /// Read the file, or nothing if there is no file.
    ///
    /// **A missing file is not an error.** It is the ordinary state before
    /// anything has been configured, and on a machine where every value is
    /// passed explicitly it is the permanent state.
    ///
    /// # Errors
    ///
    /// [`SettingsError`] if the file exists but cannot be read or parsed. An
    /// unrecognised key is a parse failure rather than something ignored: a
    /// misspelled setting that silently does nothing is worse than one that
    /// says so.
    pub fn read(path: &Path) -> Result<Self, SettingsError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(SettingsError::Unreadable {
                    path: path.display().to_string(),
                    detail: error.to_string(),
                });
            }
        };

        toml::from_str(&text).map_err(|error| SettingsError::Malformed {
            path: path.display().to_string(),
            detail: error.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Settings, SettingsError};
    use std::path::{Path, PathBuf};

    fn written(body: &str) -> Result<(tempfile::TempDir, PathBuf), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        std::fs::write(&path, body)?;
        Ok((directory, path))
    }

    #[test]
    fn a_missing_file_is_an_empty_answer() {
        let read = Settings::read(Path::new("/nowhere/at/all/config.toml"));
        assert_eq!(read, Ok(Settings::default()));
    }

    #[test]
    fn what_the_file_states_is_read_back_unparsed() {
        let (_directory, path) = match written(
            "timezone = \"Europe/London\"\ndatabase = \"/var/lib/fitness/store.db\"\n",
        ) {
            Ok(written) => written,
            Err(error) => panic!("the fixture writes: {error}"),
        };

        let settings = Settings::read(&path).expect("the file reads");
        assert_eq!(settings.timezone.as_deref(), Some("Europe/London"));
        assert_eq!(
            settings.database.as_deref(),
            Some(Path::new("/var/lib/fitness/store.db"))
        );
    }

    #[test]
    fn a_file_stating_nothing_is_as_good_as_no_file() {
        let (_directory, path) = match written("") {
            Ok(written) => written,
            Err(error) => panic!("the fixture writes: {error}"),
        };

        assert_eq!(Settings::read(&path), Ok(Settings::default()));
    }

    /// **A misspelled key is refused, not ignored.** A setting that silently
    /// does nothing is the worst of both: it looks configured and behaves as
    /// though it is not.
    #[test]
    fn an_unrecognised_key_is_refused() {
        let (_directory, path) = match written("timezome = \"Europe/London\"\n") {
            Ok(written) => written,
            Err(error) => panic!("the fixture writes: {error}"),
        };

        match Settings::read(&path) {
            Err(SettingsError::Malformed { detail, .. }) => {
                assert!(detail.contains("timezome"), "{detail}");
            }
            other => panic!("a misspelled key is refused: {other:?}"),
        }
    }
}
