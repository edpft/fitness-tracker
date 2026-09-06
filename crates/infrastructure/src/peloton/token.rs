//! The cached access token, kept between runs.
//!
//! **Every invocation used to log in.** The token lived in a `Mutex` for the
//! life of the process, so `fitness cycling next` walked the whole Auth0 flow —
//! five round trips through the SSO domain — to read one row it already had.
//! That was tolerable while `plan` was a thing run once; it is not for a sink
//! that writes on every prescription (#54, #70).
//!
//! **A file, because that is what tools in this position do.** The AWS CLI keeps
//! its config and credentials as INI where the operator edits them and caches
//! its SSO token under `~/.aws/sso/cache` where they do not; the split is
//! between what a person supplies and what a flow derives. This is the second
//! kind, so it lives apart from `credentials.toml` and in the state directory
//! rather than the config one.
//!
//! **JSON, not TOML, and the difference is the point.** TOML's advantage is that
//! a person can read and edit it, which is exactly what should not happen here:
//! nothing in this file is a decision, and a hand-edited expiry is a run that
//! fails in a way nobody can explain. It is written by the program and read by
//! the program.
//!
//! **Losing it costs a login, not a fact** (§ II on reconstructible state), so
//! nothing here is fatal. A file that will not parse is one this program did not
//! write, and the answer is to log in and overwrite it rather than to stop.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::auth::Token;

/// The token as it is written down.
///
/// Its own type rather than `serde` on [`Token`]: what is on disk is a format
/// this program has to keep reading across versions, and a domain type free to
/// change shape is the wrong thing to have promised.
#[derive(Debug, Serialize, Deserialize)]
struct Stored {
    access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    /// RFC 3339, so a person reading the file can at least see whether it is
    /// stale — which is the one question they might reasonably ask of it.
    expires_at: String,
}

/// A token cached on disk, for one source.
#[derive(Debug, Clone)]
pub struct TokenFile {
    path: PathBuf,
}

impl TokenFile {
    pub const fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The token, if a readable one is written down.
    ///
    /// `None` for every reason: no file, an unreadable file, a file that is not
    /// this format, an expiry that is not a time. None of them is an error,
    /// because all of them cost the same thing — one login.
    #[must_use]
    pub fn read(&self) -> Option<Token> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        let stored: Stored = serde_json::from_str(&text).ok()?;
        let expires_at = stored.expires_at.parse::<jiff::Timestamp>().ok()?;
        Some(Token::new(
            stored.access_token,
            stored.refresh_token,
            expires_at,
        ))
    }

    /// Write it, readable only by its owner.
    ///
    /// **The mode is set as the file is created, not after** — the reasoning is
    /// `credentials.rs`'s and applies unchanged: narrowing afterwards leaves a
    /// window in which the secret is world-readable, and on a shared machine
    /// that window is the whole vulnerability.
    ///
    /// **Written through a temporary file in the same directory**, so a run
    /// interrupted mid-write leaves the previous token rather than half of a new
    /// one. A truncated token reads as absent, which costs a login — but a
    /// half-written file that happens to parse would cost a confusing failure
    /// instead.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] if the directory or the file cannot be written.
    pub fn write(&self, token: &Token) -> std::io::Result<()> {
        let stored = Stored {
            access_token: token.access().to_owned(),
            refresh_token: token.refresh().map(str::to_owned),
            expires_at: token.expires_at().to_string(),
        };
        let body = serde_json::to_vec_pretty(&stored)
            .map_err(|error| std::io::Error::other(error.to_string()))?;

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let staging = self.path.with_extension("json.new");
        write_private(&staging, &body)?;
        std::fs::rename(&staging, &self.path)
    }

    /// Forget the token, if there is one. Absent is already forgotten.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] for anything but the file not being there.
    pub fn forget(&self) -> std::io::Result<()> {
        match std::fs::remove_file(&self.path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

#[cfg(unix)]
fn write_private(path: &Path, body: &[u8]) -> std::io::Result<()> {
    use std::{io::Write as _, os::unix::fs::OpenOptionsExt as _};

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(body)?;
    file.sync_all()
}

/// **Owner-only is not enforced off Unix**, and saying so is better than
/// implying it. Nothing here runs on Windows today; when it does, this is the
/// function that has to grow an ACL rather than the callers.
#[cfg(not(unix))]
fn write_private(path: &Path, body: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, body)
}
