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
    collections::BTreeSet,
    fs::{File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
};

use application::{NextLock, Part, RunLock, RunLockError, Taken};
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
        acquire(&self.path_for(stream))
    }
}

impl FileRunLock {
    /// **A file of its own for each part**, none of which a stream's can
    /// collide with: a stream's name is always `source.entity`, and every one
    /// of these starts `next`. Delivering keeps `.next.lock`, the file the
    /// whole run held before it had parts (#559).
    fn part_path(&self, part: &Part) -> PathBuf {
        let name = match part {
            Part::Delivering => return self.directory.join(".next.lock"),
            Part::Fetching(stream) => format!("fetching.{stream}"),
            Part::Normalising(stream) => format!("normalising.{stream}"),
            Part::Canonicalising(layer) => format!("canonicalising.{}", layer.as_str()),
        };
        self.directory.join(format!(".next.{name}.lock"))
    }

    /// The parts of `parts` a run holds now, from either frontend, taking none
    /// of them (#467).
    ///
    /// **Each is tried and let go at once**, so a run starting in that moment
    /// finds it held only for as long as the try, and joins nothing longer.
    ///
    /// # Errors
    ///
    /// [`RunLockError::Unavailable`] if a lock cannot be tried.
    pub fn held(&self, parts: &BTreeSet<Part>) -> Result<BTreeSet<Part>, RunLockError> {
        let mut held = BTreeSet::new();
        for part in parts {
            match acquire(&self.part_path(part)) {
                Ok(_free) => {}
                Err(RunLockError::Held) => {
                    held.insert(part.clone());
                }
                Err(error) => return Err(error),
            }
        }
        Ok(held)
    }
}

/// Every part a run of `next` took, held until dropped.
#[derive(Debug)]
pub struct NextRunGuard {
    _parts: Vec<FileRunGuard>,
}

impl NextLock for FileRunLock {
    type Guard = NextRunGuard;

    fn try_acquire(&self, parts: &BTreeSet<Part>) -> Result<Taken<Self::Guard>, RunLockError> {
        let mut taken = Vec::new();
        let mut held = BTreeSet::new();
        for part in parts {
            match acquire(&self.part_path(part)) {
                Ok(guard) => taken.push(guard),
                Err(RunLockError::Held) => {
                    held.insert(part.clone());
                }
                Err(error) => return Err(error),
            }
        }
        // **All or none**: what was taken is let go before the caller waits.
        if held.is_empty() {
            Ok(Taken::All(NextRunGuard { _parts: taken }))
        } else {
            Ok(Taken::Held(held))
        }
    }

    /// Each lock in turn, waited on and let go: once the last is free, every
    /// run that held one when this was asked has ended.
    async fn released(&self, parts: &BTreeSet<Part>) -> Result<(), RunLockError> {
        let paths: Vec<PathBuf> = parts.iter().map(|part| self.part_path(part)).collect();
        tokio::task::spawn_blocking(move || {
            for path in paths {
                let file = open(&path)?;
                file.lock().map_err(|error| RunLockError::Unavailable {
                    detail: format!("{}: {error}", path.display()),
                })?;
                let _ = file.unlock();
            }
            Ok(())
        })
        .await
        .map_err(|error| RunLockError::Unavailable {
            detail: error.to_string(),
        })?
    }
}

fn open(path: &Path) -> Result<File, RunLockError> {
    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .map_err(|error| RunLockError::Unavailable {
            detail: format!("{}: {error}", path.display()),
        })
}

fn acquire(path: &Path) -> Result<FileRunGuard, RunLockError> {
    let file = open(path)?;

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
