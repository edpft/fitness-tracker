//! Keys, kept apart from settings.
//!
//! **A separate file from `config.toml`, and that is the whole point.** § 35
//! allows a credential in local config, and the objection to putting one there
//! is not that it is a file — it is that the settings file is meant to be
//! opened, edited and kept with an operator's dotfiles. A secret in that file is
//! a secret that gets committed by somebody tidying up.
//!
//! So the settings stay shareable and this one does not. It is created `0600`,
//! it is named for what it holds, and nothing prints its contents.
//!
//! Keyed by source name rather than by variable name: `hevy`, not
//! `HEVY_API_KEY`. The variable is how the environment spells it, which is the
//! adapter's business — see `cli::catalogue`.

use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
};

/// Why credentials could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CredentialError {
    #[error("{path} could not be read: {detail}")]
    Unreadable { path: String, detail: String },

    #[error("{path} is not valid TOML: {detail}")]
    Malformed { path: String, detail: String },

    #[error("{path} could not be written: {detail}")]
    Unwritable { path: String, detail: String },
}

/// One key per source.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(transparent)]
pub struct Credentials {
    keys: BTreeMap<String, String>,
}

impl Credentials {
    /// Read the file, or nothing if there is no file.
    ///
    /// A missing file is the ordinary state on a machine where the key lives in
    /// the environment, so it is not an error.
    ///
    /// # Errors
    ///
    /// [`CredentialError`] if the file exists but cannot be read or parsed.
    pub fn read(path: &Path) -> Result<Self, CredentialError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(CredentialError::Unreadable {
                    path: path.display().to_string(),
                    detail: error.to_string(),
                });
            }
        };

        toml::from_str(&text).map_err(|error| CredentialError::Malformed {
            path: path.display().to_string(),
            detail: error.to_string(),
        })
    }

    /// The key for a source, if this holds one.
    pub fn key(&self, source: &str) -> Option<&str> {
        self.keys.get(source).map(String::as_str)
    }

    /// Record a key, replacing any this already held for that source.
    pub fn set(&mut self, source: &str, key: &str) {
        self.keys.insert(source.to_owned(), key.to_owned());
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Write the file, readable only by its owner.
    ///
    /// **The mode is set as the file is created, not after.** Writing it
    /// world-readable and then narrowing it leaves a window in which the key is
    /// exposed, and on a shared machine that window is the whole vulnerability.
    ///
    /// # Errors
    ///
    /// [`CredentialError::Unwritable`] if the directory or the file cannot be
    /// created, and [`CredentialError::Malformed`] if the values will not
    /// serialise.
    pub fn write(&self, path: &Path) -> Result<(), CredentialError> {
        let unwritable = |detail: String| CredentialError::Unwritable {
            path: path.display().to_string(),
            detail,
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| unwritable(error.to_string()))?;
        }

        let body = toml::to_string(self).map_err(|error| CredentialError::Malformed {
            path: path.display().to_string(),
            detail: error.to_string(),
        })?;

        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }

        let mut file = options
            .open(path)
            .map_err(|error| unwritable(error.to_string()))?;

        file.write_all(PREAMBLE.as_bytes())
            .and_then(|()| file.write_all(body.as_bytes()))
            .map_err(|error| unwritable(error.to_string()))
    }
}

/// What the written file says about itself. Short: the less inviting this file
/// is to open, quote and paste, the better.
const PREAMBLE: &str = "\
# fitness-tracker credentials. Keep this file private and out of version
# control. One key per source, named as the source is named.
#
# The matching environment variable wins where it is set — HEVY_API_KEY for
# hevy — so a single run can use a different key without editing this.

";

/// Where credentials live beside a settings file.
///
/// Derived from the settings path rather than resolved separately, so the two
/// cannot end up in different directories.
#[must_use]
pub fn beside(settings: &Path) -> PathBuf {
    settings.with_file_name("credentials.toml")
}

#[cfg(test)]
mod tests {
    use super::{Credentials, beside};
    use std::path::Path;

    #[test]
    fn a_missing_file_holds_nothing() {
        let read = Credentials::read(Path::new("/nowhere/at/all/credentials.toml"));
        assert_eq!(read, Ok(Credentials::default()));
        assert!(read.expect("nothing").is_empty());
    }

    #[test]
    fn what_is_written_reads_back_under_its_source_name() {
        let directory = match tempfile::tempdir() {
            Ok(directory) => directory,
            Err(error) => panic!("a temporary directory: {error}"),
        };
        let path = directory.path().join("credentials.toml");

        let mut credentials = Credentials::default();
        credentials.set("hevy", "a-secret");
        credentials.write(&path).expect("the file writes");

        let read = Credentials::read(&path).expect("the file reads");
        assert_eq!(read.key("hevy"), Some("a-secret"));
        assert_eq!(read.key("strava"), None);
    }

    /// **Owner-only, from the moment it exists.** A key written world-readable
    /// and narrowed afterwards has already been exposed.
    #[cfg(unix)]
    #[test]
    fn the_file_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = match tempfile::tempdir() {
            Ok(directory) => directory,
            Err(error) => panic!("a temporary directory: {error}"),
        };
        let path = directory.path().join("credentials.toml");

        let mut credentials = Credentials::default();
        credentials.set("hevy", "a-secret");
        credentials.write(&path).expect("the file writes");

        let mode = std::fs::metadata(&path)
            .expect("the file exists")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "mode was {:o}", mode & 0o777);
    }

    /// Replacing a key keeps the file owner-only, which a plain rewrite could
    /// lose.
    #[cfg(unix)]
    #[test]
    fn replacing_a_key_keeps_the_file_private() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = match tempfile::tempdir() {
            Ok(directory) => directory,
            Err(error) => panic!("a temporary directory: {error}"),
        };
        let path = directory.path().join("credentials.toml");

        let mut credentials = Credentials::default();
        credentials.set("hevy", "first");
        credentials.write(&path).expect("the file writes");
        credentials.set("hevy", "second");
        credentials.write(&path).expect("the file rewrites");

        assert_eq!(
            Credentials::read(&path)
                .expect("the file reads")
                .key("hevy"),
            Some("second")
        );
        let mode = std::fs::metadata(&path)
            .expect("the file exists")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "mode was {:o}", mode & 0o777);
    }

    /// They sit together, so nothing can put them in different directories.
    #[test]
    fn credentials_sit_beside_the_settings() {
        assert_eq!(
            beside(Path::new("/config/fitness-tracker/config.toml")),
            Path::new("/config/fitness-tracker/credentials.toml")
        );
    }
}
