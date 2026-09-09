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
pub mod cycling;
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
    AccountReader, Authored, Clock, CyclingMesocycleStore, Deliverable, Delivered, Delivery,
    DeliveryAttempt, DeliveryReference, DerivationStatus, DerivationStatusReporter,
    DestinationName, DestinationReply, DiaryAuthor, DiaryStore, EventBatch, ExerciseHistory,
    ExtractionRunLog, ExtractionStatusReporter, FtpHistory, FulfilledSession,
    GenerationParameterStore, Issuance, LadderStanding, LandingStore, LastPerformance,
    MesocycleStore, NormalisationRunLog, NormalisationSummary, NormalisedEntityStore, Performance,
    PerformedSetSummary, PerformedWorkoutReader, PlanAuthor, PlanStore, PrescribedWorkoutId,
    PrescribedWorkoutStore, Prescription, PrescriptionDeliverer, PrescriptionDeliveryStore,
    PrescriptionDestination, PrescriptionLifecycle, RawExtent, RefusalReport, RefusalReporter,
    RefusalStore, ReplyStatus, ResumptionPointResetter, ResumptionPointStore, RunLock, RunSummary,
    SessionOrdinal, SourceAccount, SourceEvent, StreamStatus, Translation, Translator,
    UnderivableReason, UnderivableSlot, Unexpressed, WorkoutEventSource, WorkoutExtractor,
    WorkoutNormaliser, WorkoutPrescriber,
};
