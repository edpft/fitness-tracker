//! What a source accepts as proof of who is asking, kept between runs.
//!
//! **The state directory, and JSON.** A hand-editable secret is a secret
//! somebody eventually hand-edits, and a truncated paste is invisible until the
//! source refuses it (#61). This file is written by the program and read by the
//! program, which is `peloton.token.json`'s own argument for JSON, and it sits
//! beside that token for the same reason — persists between runs, nobody edits
//! it, losing it costs a login rather than a fact.
//!
//! **Two shapes, because there are two kinds of credential.** They are not
//! invented here — they are the ones `cli::catalogue` already declares, so a
//! third source is an entry in the catalogue and nothing in this module.
//!
//! Keyed by source name rather than by variable name: `hevy`, not
//! `HEVY_API_KEY`. The variable is how the environment spells it, which is the
//! adapter's business.

use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

/// Why credentials could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CredentialError {
    #[error("{path} could not be read: {detail}")]
    Unreadable { path: String, detail: String },

    #[error("{path} is not valid JSON: {detail}")]
    Malformed { path: String, detail: String },

    #[error("{path} could not be written: {detail}")]
    Unwritable { path: String, detail: String },
}

/// One source's credential, as it is written down.
///
/// **Tagged, so the two shapes cannot be confused for one another.** An untagged
/// union would read a login missing its password as an API key and fail much
/// later, at the source, with a message about authorisation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Credential {
    ApiKey { key: String },
    Login { email: String, password: String },
}

impl Credential {
    /// Whether this holds anything worth sending.
    ///
    /// **Blank is absent.** A key of spaces reaches the source as a rejected
    /// request rather than as the missing credential it actually is.
    pub fn is_stated(&self) -> bool {
        match self {
            Self::ApiKey { key } => !key.trim().is_empty(),
            Self::Login { email, password } => {
                !email.trim().is_empty() && !password.trim().is_empty()
            }
        }
    }
}

/// One credential per source.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Credentials {
    sources: BTreeMap<String, Credential>,
}

impl Credentials {
    /// Read the file, or nothing if there is no file.
    ///
    /// A missing file is the ordinary state on a machine that has not run
    /// `init` yet, and on one where every value comes from the environment, so
    /// it is not an error.
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

        serde_json::from_str(&text).map_err(|error| CredentialError::Malformed {
            path: path.display().to_string(),
            detail: error.to_string(),
        })
    }

    /// The credential for a source, if this holds a stated one.
    pub fn credential(&self, source: &str) -> Option<&Credential> {
        self.sources
            .get(source)
            .filter(|credential| credential.is_stated())
    }

    /// Record a credential, replacing any this already held for that source.
    pub fn set(&mut self, source: &str, credential: Credential) {
        self.sources.insert(source.to_owned(), credential);
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// The sources this holds a credential for, in name order.
    pub fn sources(&self) -> impl Iterator<Item = &str> {
        self.sources.keys().map(String::as_str)
    }

    /// Write the file, readable only by its owner.
    ///
    /// # Errors
    ///
    /// [`CredentialError::Unwritable`] if the directory or the file cannot be
    /// created, and [`CredentialError::Malformed`] if the values will not
    /// serialise.
    pub fn write(&self, path: &Path) -> Result<(), CredentialError> {
        let body = serde_json::to_vec_pretty(self).map_err(|error| CredentialError::Malformed {
            path: path.display().to_string(),
            detail: error.to_string(),
        })?;

        crate::private_file::write(path, &body).map_err(|error| CredentialError::Unwritable {
            path: path.display().to_string(),
            detail: error.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Credential, Credentials};

    fn key(key: &str) -> Credential {
        Credential::ApiKey {
            key: key.to_owned(),
        }
    }

    #[test]
    fn a_missing_file_is_no_credentials_rather_than_an_error() {
        let read = Credentials::read(std::path::Path::new("/nowhere/credentials.json"));
        match read {
            Ok(credentials) => assert!(credentials.is_empty()),
            Err(error) => panic!("a missing file read as {error}"),
        }
    }

    #[test]
    fn both_shapes_survive_a_round_trip() {
        let directory = match tempfile::tempdir() {
            Ok(directory) => directory,
            Err(error) => panic!("a temporary directory: {error}"),
        };
        let path = directory.path().join("credentials.json");

        let mut credentials = Credentials::default();
        credentials.set("hevy", key("a-secret"));
        credentials.set(
            "peloton",
            Credential::Login {
                email: "someone@example.test".to_owned(),
                password: "a-password".to_owned(),
            },
        );
        credentials.write(&path).expect("the file writes");

        let read = Credentials::read(&path).expect("the file reads");
        assert_eq!(read.credential("hevy"), Some(&key("a-secret")));
        assert_eq!(
            read.credential("peloton"),
            Some(&Credential::Login {
                email: "someone@example.test".to_owned(),
                password: "a-password".to_owned(),
            })
        );
    }

    /// A login stored where a key is expected must not be read as one.
    #[test]
    fn the_kinds_are_distinguishable_on_disk() {
        let directory = match tempfile::tempdir() {
            Ok(directory) => directory,
            Err(error) => panic!("a temporary directory: {error}"),
        };
        let path = directory.path().join("credentials.json");

        let mut credentials = Credentials::default();
        credentials.set("hevy", key("a-secret"));
        credentials.write(&path).expect("the file writes");

        let written = std::fs::read_to_string(&path).expect("the file reads");
        assert!(written.contains("\"kind\": \"api_key\""), "{written}");
    }

    /// Blank is absent: it reaches the source as a rejection rather than as the
    /// missing credential it is.
    #[test]
    fn a_blank_credential_is_not_stated() {
        let mut credentials = Credentials::default();
        credentials.set("hevy", key("   "));
        assert_eq!(credentials.credential("hevy"), None);
    }

    #[test]
    fn a_file_this_program_did_not_write_is_an_error() {
        let directory = match tempfile::tempdir() {
            Ok(directory) => directory,
            Err(error) => panic!("a temporary directory: {error}"),
        };
        let path = directory.path().join("credentials.json");
        std::fs::write(&path, "hevy = \"a-secret\"\n").expect("the file writes");

        assert!(Credentials::read(&path).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = match tempfile::tempdir() {
            Ok(directory) => directory,
            Err(error) => panic!("a temporary directory: {error}"),
        };
        let path = directory.path().join("credentials.json");

        let mut credentials = Credentials::default();
        credentials.set("hevy", key("a-secret"));
        credentials.write(&path).expect("the file writes");

        let mode = std::fs::metadata(&path)
            .expect("the file exists")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "mode was {:o}", mode & 0o777);
    }

    /// Replacing a credential keeps the file owner-only, which a plain rewrite
    /// could lose.
    #[cfg(unix)]
    #[test]
    fn replacing_a_credential_keeps_the_file_private() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = match tempfile::tempdir() {
            Ok(directory) => directory,
            Err(error) => panic!("a temporary directory: {error}"),
        };
        let path = directory.path().join("credentials.json");

        let mut credentials = Credentials::default();
        credentials.set("hevy", key("first"));
        credentials.write(&path).expect("the file writes");
        credentials.set("hevy", key("second"));
        credentials.write(&path).expect("the file rewrites");

        assert_eq!(
            Credentials::read(&path)
                .expect("the file reads")
                .credential("hevy"),
            Some(&key("second"))
        );
        let mode = std::fs::metadata(&path)
            .expect("the file exists")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "mode was {:o}", mode & 0o777);
    }
}
