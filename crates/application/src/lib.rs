//! Use cases, and the ports through which they talk to the outside world.
//!
//! Depends only on `domain`. The ports are defined *here*, in terms the
//! application understands, and are implemented out in `infrastructure`,
//! `cli` and `web` — that inversion is what keeps adapters swappable, and what
//! lets a use case be tested against fakes with no I/O anywhere near it.
//!
//! Note what this file re-exports and what it does not. The ports and the
//! errors are the crate's flat surface, because every ring above implements or
//! handles them. The use cases stay behind [`extract`] and [`status`], so
//! reaching one is spelled `application::extract::Extraction` — visible in a
//! diff, and greppable by the `use-case-isolation` check, which is what stops
//! a driven adapter from quietly calling the application it is supposed to be
//! driven by.

pub mod compare;
pub mod deliver;
pub mod error;
pub mod extract;
pub mod normalise;
pub mod ports;
pub mod prescribe;
pub mod status;

pub use error::{
    ComparisonError, DeliveryError, ExtractionError, NormalisationError, PrescriptionError,
    RunLockError, SourceError, StatusError, StoreError,
};
pub use ports::{
    Authored, Clock, Deliverable, Delivered, Delivery, DeliveryReference, DerivationStatus,
    DerivationStatusReporter, DestinationName, DiaryAuthor, DiaryStore, EventBatch,
    ExerciseHistory, ExtractionRunLog, ExtractionStatusReporter, FulfilledSession,
    GenerationParameterStore, Issuance, LadderStanding, LandingRecordReader, LandingStore,
    LastPerformance, NormalisationRunLog, NormalisationSummary, NormalisedWorkoutStore,
    Performance, PerformedSetSummary, PerformedWorkoutReader, PrescribedWorkoutId,
    PrescribedWorkoutStore, Prescription, PrescriptionDeliverer, PrescriptionDeliveryStore,
    PrescriptionDestination, PrescriptionLifecycle, ProgrammeAuthor, ProgrammeStore, RefusalReport,
    RefusalReporter, RefusalStore, ResumptionPointResetter, ResumptionPointStore, RunLock,
    RunSummary, SessionOrdinal, SourceEvent, StreamStatus, Translation, UnderivableReason,
    UnderivableSlot, Unexpressed, WorkoutEventSource, WorkoutExtractor, WorkoutNormaliser,
    WorkoutPrescriber, WorkoutTranslator,
};
