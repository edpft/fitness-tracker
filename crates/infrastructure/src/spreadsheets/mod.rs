//! The operator's historical spreadsheets, read from a folder (#263).
//!
//! **A folder rather than the clouds they sit in**, because this is a one-off: the
//! files in scope are copied once into a folder on the operator's machine, laid
//! out by where each came from, and everything in it lands. There is no cloud
//! adapter and no credential.
//!
//! **A file's identity is the digest of its bytes.** So a byte-identical copy
//! is the same record served again and lands nothing, and a conflicted copy
//! that differs is a record of its own. Which of two versions is right is
//! normalisation's question, not this one's.

use std::{
    convert::Infallible,
    fs,
    path::{Path, PathBuf},
};

use application::{EventBatch, SourceError, SourceEvent, WorkoutEventSource};
use domain::landing::{
    FilePath, FileProvenance, ModifiedAt, RawPayload, SourceRecordId, Watermark,
};

/// Every file under one folder, as it is on disk.
#[derive(Debug, Clone)]
pub struct SpreadsheetFiles {
    folder: PathBuf,
}

impl SpreadsheetFiles {
    pub fn new(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: folder.into(),
        }
    }
}

impl WorkoutEventSource for SpreadsheetFiles {
    /// **Never resumed.** The folder is read whole on every run: it is small,
    /// and a file's modification time is no position to resume from, since
    /// a file copied in later may carry an older one.
    type Resume = Infallible;

    /// Every file under the folder, in path order, whatever `since` says.
    async fn fetch(
        &self,
        _since: Option<Watermark>,
        _resume: Option<Infallible>,
    ) -> Result<EventBatch<Infallible>, SourceError> {
        let folder = self.folder.clone();
        let events = tokio::task::spawn_blocking(move || read_folder(&folder))
            .await
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })??;

        Ok(EventBatch {
            events,
            resume: None,
        })
    }
}

fn read_folder(folder: &Path) -> Result<Vec<SourceEvent>, SourceError> {
    let mut files = Vec::new();
    walk(folder, folder, &mut files)?;
    files.sort_by(|(left, _), (right, _)| left.cmp(right));

    files
        .into_iter()
        .map(|(path, on_disk)| event(path, &on_disk))
        .collect()
}

/// Every file under `directory`, with its path relative to `folder`.
fn walk(
    folder: &Path,
    directory: &Path,
    files: &mut Vec<(FilePath, PathBuf)>,
) -> Result<(), SourceError> {
    let entries = fs::read_dir(directory).map_err(|error| unreadable(directory, &error))?;

    for entry in entries {
        let on_disk = entry.map_err(|error| unreadable(directory, &error))?.path();
        // Followed rather than read as a link: a file linked into the folder
        // is a file the operator put there.
        let metadata = fs::metadata(&on_disk).map_err(|error| unreadable(&on_disk, &error))?;

        if metadata.is_dir() {
            walk(folder, &on_disk, files)?;
        } else if metadata.is_file() {
            files.push((relative(folder, &on_disk)?, on_disk));
        } else {
            return Err(SourceError::Malformed {
                detail: format!("{} is neither a file nor a folder", on_disk.display()),
            });
        }
    }

    Ok(())
}

/// `Dropbox/Random/Training 2014.xlsx`, separated by `/` on every platform.
fn relative(folder: &Path, on_disk: &Path) -> Result<FilePath, SourceError> {
    let not_a_path = || SourceError::Malformed {
        detail: format!("{} has no path this store can record", on_disk.display()),
    };

    let components = on_disk
        .strip_prefix(folder)
        .map_err(|_| not_a_path())?
        .components()
        .map(|component| component.as_os_str().to_str().ok_or_else(not_a_path))
        .collect::<Result<Vec<_>, _>>()?;

    FilePath::try_from(components.join("/")).map_err(|_| not_a_path())
}

fn event(path: FilePath, on_disk: &Path) -> Result<SourceEvent, SourceError> {
    let bytes = fs::read(on_disk).map_err(|error| unreadable(on_disk, &error))?;
    let payload = RawPayload::try_from(bytes).map_err(|error| SourceError::Malformed {
        detail: format!("{path}: {error}"),
    })?;

    let modified = fs::metadata(on_disk)
        .and_then(|metadata| metadata.modified())
        .map_err(|error| unreadable(on_disk, &error))?;
    let modified_at = jiff::Timestamp::try_from(modified)
        .map(ModifiedAt::from)
        .map_err(|error| SourceError::Malformed {
            detail: format!("{path} has a modification time out of range: {error}"),
        })?;

    let id = SourceRecordId::try_from(payload.digest().to_string()).map_err(|error| {
        SourceError::Malformed {
            detail: error.to_string(),
        }
    })?;

    Ok(SourceEvent::new(
        id,
        FileProvenance::new(path, modified_at).into(),
        payload,
    ))
}

fn unreadable(path: &Path, error: &std::io::Error) -> SourceError {
    SourceError::Unavailable {
        detail: format!("{}: {error}", path.display()),
    }
}

mod weigh_ins;

pub use weigh_ins::SpreadsheetWeighInTranslator;
