//! Writing a file only its owner can read.
//!
//! **One implementation, because two would diverge.** The token cache and the
//! credentials file both hold a secret and both have to create it owner-only
//! rather than narrow it afterwards — writing world-readable and then
//! tightening leaves a window in which the secret is exposed, and on a shared
//! machine that window is the whole vulnerability.
//!
//! **Written to a staging file and renamed.** A write that fails halfway
//! through leaves the original intact rather than a truncated file, and a rename
//! within a directory is atomic — so a reader sees the old contents or the new
//! ones and never a half-written line.

use std::path::Path;

/// Write `body` to `path`, owner-only, replacing whatever was there.
///
/// # Errors
///
/// [`std::io::Error`] if the directory cannot be created, or the file cannot be
/// written or renamed into place.
pub fn write(path: &Path, body: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let staging = path.with_extension("new");
    write_private(&staging, body)?;
    std::fs::rename(&staging, path)
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
