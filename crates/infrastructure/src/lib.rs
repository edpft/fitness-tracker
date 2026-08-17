//! Driven (outbound) adapters: the implementations of the ports that
//! `application` declared.
//!
//! This is where a technology choice is allowed to show — sqlx, reqwest, a
//! filesystem. Swap this crate out and the domain and use cases do not move.
//! Nothing here leaks upward: every vendor error is translated at the boundary
//! into the application's own view of failure.

pub mod hevy;
pub mod lock;
pub mod store;

pub use hevy::{HevyWorkoutEvents, HevyWorkoutTranslator, PageCount, PageNumber, RetryPolicy};
pub use lock::FileRunLock;
pub use store::{
    HevyWorkoutLandingReader, HevyWorkoutLandingStore, SqliteExerciseHistory,
    SqliteExtractionRunLog, SqliteGenerationParameterStore, SqliteGymWorkoutStore,
    SqliteNormalisationRunLog, SqliteProgrammeStore, SqliteRefusalStore,
    SqliteResumptionPointStore, connect,
};
