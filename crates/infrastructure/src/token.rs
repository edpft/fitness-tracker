//! The cached access token, kept between runs, for any source that issues one.
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
//! kind, so it is kept apart from `credentials.json`, which holds the first.
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
//!
//! **A token names the credential that obtained it**, and a file naming another
//! one reads as absent (#197). A token that outlives its password is access the
//! operator believes he has revoked, and a run holding one both reports that the
//! source rejected its credential and writes to the account on the same breath.

use std::{
    fmt,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

/// How long before expiry a token is treated as spent.
///
/// A token that expires while a request is in flight fails the run, and the
/// whole point of the refresh is that it does not. Sixty seconds is longer than
/// any single call this adapter makes.
const EXPIRY_MARGIN: jiff::SignedDuration = jiff::SignedDuration::from_secs(60);

/// Which credential a token was obtained with.
///
/// **A token is bound to the credential that earned it** (#197). Without this a
/// token outlives the password it came from, so one run can report `the source
/// rejected our credential` for the half of it that logs in and succeed for the
/// half that sends a cached bearer — the reading half announcing it has no
/// access while the writing half posts to the account.
///
/// SHA-256, and a digest rather than the credential because this is written to
/// disk beside the token. It adds no exposure either way: the password is
/// already in `credentials.json` in the clear, and the digest cannot be sent
/// anywhere as a credential.
///
/// **Each field is length-prefixed**, so an email and a password that run
/// together into the same characters as another pair are still two credentials.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CredentialDigest([u8; 32]);

impl CredentialDigest {
    /// Over a credential's fields, in the order the credential states them.
    #[must_use]
    pub fn of<'a>(fields: impl IntoIterator<Item = &'a str>) -> Self {
        let mut hasher = Sha256::new();
        for field in fields {
            hasher.update(u64::try_from(field.len()).unwrap_or(u64::MAX).to_be_bytes());
            hasher.update(field.as_bytes());
        }
        Self(hasher.finalize().into())
    }
}

impl fmt::Display for CredentialDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// A bearer token, what is needed to replace it, and whose it is.
///
/// **Wall clock, not `Instant`.** A `std::time::Instant` is monotonic and has no
/// meaning outside the process that read it — it cannot be written down, and a
/// token this adapter cannot write down is one that costs a full login on
/// every invocation (#54). A `Timestamp` is an instant anyone can agree on,
/// which is what § II asks of every time this system stores.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    access: String,
    refresh: Option<String>,
    expires_at: jiff::Timestamp,
    obtained_with: CredentialDigest,
}

impl Token {
    pub const fn new(
        access: String,
        refresh: Option<String>,
        expires_at: jiff::Timestamp,
        obtained_with: CredentialDigest,
    ) -> Self {
        Self {
            access,
            refresh,
            expires_at,
            obtained_with,
        }
    }

    pub const fn obtained_with(&self) -> CredentialDigest {
        self.obtained_with
    }

    pub fn access(&self) -> &str {
        &self.access
    }

    pub fn refresh(&self) -> Option<&str> {
        self.refresh.as_deref()
    }

    pub const fn expires_at(&self) -> jiff::Timestamp {
        self.expires_at
    }

    /// Whether it is still good, with [`EXPIRY_MARGIN`] to spare.
    ///
    /// A clock that cannot add a minute to now is not a clock this can reason
    /// about, so the answer there is "spent" — a needless login is a cost, and
    /// using a token that may already be dead is a failed run.
    #[must_use]
    pub fn usable(&self) -> bool {
        jiff::Timestamp::now()
            .checked_add(EXPIRY_MARGIN)
            .is_ok_and(|soon| soon < self.expires_at)
    }
}

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
    /// Hex, and absent in a file written before #197. Absent reads as a token
    /// belonging to nobody, which is discarded: that costs one login and needs
    /// no migration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    obtained_with: Option<String>,
}

/// A token cached on disk, for one source and one credential.
///
/// **The credential is named at construction rather than passed to each call.**
/// A token file is only ever read on behalf of the credential in hand, and
/// asking "is there a token?" separately from "is it this credential's?" is the
/// split that #197 is: a run that answers the first *no* and acts on the second
/// *yes* in the same invocation.
#[derive(Debug, Clone)]
pub struct TokenFile {
    path: PathBuf,
    obtained_with: CredentialDigest,
}

impl TokenFile {
    pub const fn new(path: PathBuf, obtained_with: CredentialDigest) -> Self {
        Self {
            path,
            obtained_with,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The token this credential obtained, if a readable one is written down.
    ///
    /// `None` for every reason: no file, an unreadable file, a file that is not
    /// this format, an expiry that is not a time, and a token some other
    /// credential obtained. None of them is an error, because all of them cost
    /// the same thing — one login.
    #[must_use]
    pub fn read(&self) -> Option<Token> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        let stored: Stored = serde_json::from_str(&text).ok()?;
        let expires_at = stored.expires_at.parse::<jiff::Timestamp>().ok()?;
        if stored.obtained_with? != self.obtained_with.to_string() {
            return None;
        }
        Some(Token::new(
            stored.access_token,
            stored.refresh_token,
            expires_at,
            self.obtained_with,
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
            obtained_with: Some(token.obtained_with().to_string()),
        };
        let body = serde_json::to_vec_pretty(&stored)
            .map_err(|error| std::io::Error::other(error.to_string()))?;

        crate::private_file::write(&self.path, &body)
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
