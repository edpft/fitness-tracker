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
pub mod peloton;
pub mod settings;
pub mod store;

pub use credentials::{CredentialError, Credentials};
pub use hevy::{
    HevyRoutinePreview, HevyRoutines, HevyWorkoutEvents, HevyWorkoutTranslator, PageCount,
    PageNumber, RetryPolicy,
};
pub use lock::FileRunLock;
pub use peloton::{MappedSession, PelotonClass};
pub use settings::{Settings, SettingsError};
pub use store::{
    HevyWorkoutLandingReader, HevyWorkoutLandingStore, SqliteCyclingMesocycleStore,
    SqliteDiaryStore, SqliteExerciseHistory, SqliteExtractionRunLog,
    SqliteGenerationParameterStore, SqliteGymMesocycleStore, SqliteGymWorkoutStore,
    SqliteNormalisationRunLog, SqlitePerformedWorkoutReader, SqlitePlanStore,
    SqlitePrescribedWorkoutStore, SqlitePrescriptionDeliveryStore, SqliteRefusalStore,
    SqliteResumptionPointStore, connect,
};

/// The pool every store is built on, so a composition root can open one and
/// hand it to several without depending on `sqlx` itself.
pub use sqlx::SqlitePool;
