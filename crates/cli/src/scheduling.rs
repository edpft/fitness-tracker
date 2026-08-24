//! `fitness schedule` — what the operator's week looks like.
//!
//! **Operator-level, so no programme is consulted here.** The diary records
//! when there is room to train; which of those slots the gym may use is
//! allocation, and allocation is planning rather than fact.

use std::path::Path;

use application::{DiaryAuthor as _, DiaryStore as _};
use infrastructure::{ScheduleDocument, SqliteDiaryStore, connect};

use crate::{Failure, exit, output};

/// Read a document and store the week and holidays it describes.
pub async fn add(database: &Path, path: &Path) -> Result<(), Failure> {
    let document = ScheduleDocument::read(path).map_err(|error| Failure::usage(&error))?;
    let week = document.week().map_err(|error| Failure::usage(&error))?;
    let patches = document.patches().map_err(|error| Failure::usage(&error))?;

    let pool = connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    let diary = SqliteDiaryStore::new(pool);

    diary
        .record_week(&week)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    for patch in &patches {
        diary
            .record_patch(patch)
            .await
            .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    }

    output::schedule_recorded(&week, &patches);
    Ok(())
}

/// Report the ordinary week and the holidays that depart from it.
pub async fn show(database: &Path) -> Result<(), Failure> {
    let pool = connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let diary = SqliteDiaryStore::new(pool)
        .diary()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    output::schedule(&diary);
    Ok(())
}
