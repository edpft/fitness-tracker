//! The SQLite store: raw landing, run history, and resumption points.
//!
//! SQLite because this is a single-operator system whose whole history is a
//! few thousand records — a server-based store buys nothing and costs an
//! operational dependency. It also keeps the primary test suite hermetic:
//! integration tests at the port boundaries run against a temporary file
//! inside the nix sandbox, with no service to start and no network.

pub mod history;
pub mod landing;
pub mod normalisation_run_log;
pub mod normalised;
pub mod parameters;
pub mod pool;
pub mod prescription;
pub mod programme;
pub mod refusals;
pub mod resumption;
pub mod run_log;

use application::StoreError;
use domain::{gym::NormalisationRunId, landing::RunId};

pub use history::{SqliteExerciseHistory, SqlitePerformedWorkoutReader};
pub use landing::HevyWorkoutLandingStore;
pub use normalisation_run_log::SqliteNormalisationRunLog;
pub use normalised::{HevyWorkoutLandingReader, SqliteGymWorkoutStore};
pub use parameters::SqliteGenerationParameterStore;
pub use pool::connect;
pub use prescription::SqlitePrescribedWorkoutStore;
pub use programme::SqliteProgrammeStore;
pub use refusals::SqliteRefusalStore;
pub use resumption::SqliteResumptionPointStore;
pub use run_log::SqliteExtractionRunLog;

/// Translate a store failure into the application's view of one.
///
/// This is the boundary: no `sqlx::Error` and no SQLite result code exists
/// above this function.
fn store_error(error: &sqlx::Error) -> StoreError {
    match error {
        // A row that will not decode is not a transient fault — something is in
        // the file that this program did not put there, or could not have.
        sqlx::Error::ColumnDecode { .. }
        | sqlx::Error::Decode(_)
        | sqlx::Error::TypeNotFound { .. } => StoreError::Corrupt {
            detail: error.to_string(),
        },
        _ => StoreError::Unavailable {
            detail: error.to_string(),
        },
    }
}

/// SQLite counts rows in `i64` and a run id does not go negative, so the two
/// representations meet here and nowhere else.
fn run_id_for_storage(run: RunId) -> Result<i64, StoreError> {
    i64::try_from(run.as_u64()).map_err(|_| StoreError::Corrupt {
        detail: format!("run id {run} is larger than the store can hold"),
    })
}

/// A negative id is not a run this program started.
fn run_id_from_row(id: i64) -> Result<RunId, StoreError> {
    RunId::try_from(id).map_err(|error| StoreError::Corrupt {
        detail: error.to_string(),
    })
}

/// The same narrowing for a derivation's run id.
fn normalisation_run_for_storage(run: NormalisationRunId) -> Result<i64, StoreError> {
    i64::try_from(run.as_u64()).map_err(|_| StoreError::Corrupt {
        detail: format!("run id {run} is larger than the store can hold"),
    })
}

/// Something in the file that this program did not put there, or could not
/// have. Written once here because every reader needs it and none of them needs
/// to phrase it differently.
fn corrupt(error: &dyn std::fmt::Display) -> StoreError {
    StoreError::Corrupt {
        detail: error.to_string(),
    }
}

/// A count on its way into the store.
///
/// SQLite holds signed 64-bit integers and a count does not go negative, so the
/// two representations meet here. It returns an error rather than saturating:
/// a count that will not fit is a bug worth hearing about, and a silent
/// `i64::MAX` would be a wrong number recorded as if it were right.
fn count_for_storage(count: usize) -> Result<i64, StoreError> {
    i64::try_from(count).map_err(|_| StoreError::Corrupt {
        detail: format!("a count of {count} is larger than the store can hold"),
    })
}

/// A count read back out of the store. Negative means the file holds something
/// this program did not write.
fn count_from_storage(value: Option<i64>) -> Result<usize, StoreError> {
    let value = value.unwrap_or_default();
    usize::try_from(value).map_err(|_| StoreError::Corrupt {
        detail: format!("a stored count of {value} is not a count"),
    })
}
