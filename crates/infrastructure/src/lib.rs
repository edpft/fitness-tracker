//! Driven (outbound) adapters: the implementations of the ports that
//! `application` declared.
//!
//! This is where a technology choice is allowed to show — sqlx, reqwest, a
//! filesystem. Swap this crate out and the domain and use cases do not move.
//! Nothing here leaks upward: every vendor error is translated at the boundary
//! into the application's own view of failure.

pub mod credentials;
pub mod hevy;
pub mod lock;
pub mod programme;
pub mod settings;
pub mod store;

pub use credentials::{CredentialError, Credentials};
pub use hevy::{
    HevyRoutinePreview, HevyRoutines, HevyWorkoutEvents, HevyWorkoutTranslator, PageCount,
    PageNumber, RetryPolicy,
};
pub use lock::FileRunLock;
pub use programme::{Document, DocumentError};
pub use settings::{Settings, SettingsError};
pub use store::{
    HevyWorkoutLandingReader, HevyWorkoutLandingStore, SqliteDiaryStore, SqliteExerciseHistory,
    SqliteExtractionRunLog, SqliteGenerationParameterStore, SqliteGymWorkoutStore,
    SqliteNormalisationRunLog, SqlitePerformedWorkoutReader, SqlitePrescribedWorkoutStore,
    SqlitePrescriptionDeliveryStore, SqliteProgrammeStore, SqliteRefusalStore,
    SqliteResumptionPointStore, connect,
};
