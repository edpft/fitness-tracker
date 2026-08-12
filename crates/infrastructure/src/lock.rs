//! Single-flight, enforced by the operating system.
//!
//! Two runs sharing a resumption point can advance it past records neither has
//! landed, which breaks resumption silently. So a second run must fail rather
//! than wait.
//!
//! The lock is an advisory lock on a file beside the database. The kernel
//! releases it when the process dies, so a crashed or killed run leaves
//! nothing to unstick — where a `running` row in the store would survive the
//! crash and need a manual repair, which is a worse failure for a single
//! operator than a lock that simply lets go.
//!
//! `std::fs::File` grew `try_lock` and `unlock` in Rust 1.89, so this needs no
//! crate. Planning assumed one would be required; it is not.

use std::{
    fs::{File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
};

use application::{RunLock, RunLockError};
use domain::landing::LandingStream;

/// An advisory file lock, one file per stream.
///
/// Per stream rather than per database: collecting Hevy workouts and Hevy body
/// measurements are independent runs, and blocking one on the other would be a
/// restriction nothing asks for.
#[derive(Debug, Clone)]
pub struct FileRunLock {
    directory: PathBuf,
}

impl FileRunLock {
    /// Locks live beside the database, so a second copy of the store in
    /// another directory is a separate system with its own lock.
    pub fn beside(database: &Path) -> Self {
        let directory = database
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        Self { directory }
    }

    fn path_for(&self, stream: &LandingStream) -> PathBuf {
        self.directory.join(format!(".{stream}.lock"))
    }
}

/// Holds the lock until dropped.
///
/// The file handle is the lock: dropping it releases, and so does the process
/// exiting for any reason.
#[derive(Debug)]
pub struct FileRunGuard {
    file: File,
}

impl Drop for FileRunGuard {
    fn drop(&mut self) {
        // Best effort. The kernel releases the lock when the descriptor closes
        // a moment later regardless, so a failure here changes nothing.
        let _ = self.file.unlock();
    }
}

impl RunLock for FileRunLock {
    type Guard = FileRunGuard;

    fn try_acquire(&self, stream: &LandingStream) -> Result<Self::Guard, RunLockError> {
        let path = self.path_for(stream);

        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| RunLockError::Unavailable {
                detail: format!("{}: {error}", path.display()),
            })?;

        // Try, never block. Waiting would turn a loud "already running" into a
        // silent queue, and an operator running extraction twice by mistake
        // wants to be told, not to wait.
        file.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => RunLockError::Held,
            TryLockError::Error(error) => RunLockError::Unavailable {
                detail: error.to_string(),
            },
        })?;

        Ok(FileRunGuard { file })
    }
}
