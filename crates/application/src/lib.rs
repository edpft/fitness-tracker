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

pub mod body_weight;
pub mod canonicalise;
pub mod commit;
pub mod compare;
pub mod composition;
pub mod cycling;
pub mod deliver;
pub mod devices;
pub mod equipment;
pub mod error;
pub mod extract;
pub mod fills;
pub mod heart_rate;
pub mod holding;
pub mod microcycle;
pub mod next;
pub mod normalise;
pub mod one_rep_max;
pub mod per_device;
pub mod ports;
pub mod prescribe;
pub mod published;
pub mod ranges;
pub mod reschedule;
pub mod status;
pub mod strength;

pub use error::{
    ComparisonError, DeliveryError, EquipmentError, ExtractionError, FillChangeError,
    NormalisationError, PrescriptionError, RunLockError, SourceError, StatusError, StoreError,
};
pub use ports::{
    AccountReader, Authored, Build, ByReason, CanonicalCyclingSessionStore,
    CanonicalGymSessionStore, CanonicalLayer, CanonicalWeighInStore, ClassSource, ClassesListed,
    ClassesRead, Clock, CompositionHistory, CyclingDeliveryStore, CyclingMesocycleStore,
    Deliverable, Delivered, Delivery, DeliveryAttempt, DeliveryReference, DerivationLedger,
    DerivationStatus, DerivationStatusReporter, DerivedFrom, DestinationName, DestinationReply,
    DiaryAuthor, DiaryStore, EquipmentStore, EventBatch, ExerciseHistory, ExtractionRunLog,
    ExtractionStatusReporter, FillChangeStore, FtpHistory, FulfilledSession,
    GenerationParameterStore, HoldingRides, HolidayCalendar, Issuance, LadderStanding,
    LandingStore, LastPerformance, Mark, MesocycleInForce, MesocycleStore, NewestObservation,
    NextLock, NormalisationRunLog, NormalisationSummary, NormalisedCyclingSessionReader,
    NormalisedEntityStore, NormalisedGymSessionReader, NormalisedLayers, NormalisedWeighInReader,
    Occupant, OvernightHrvHistory, Part, Performance, PerformedCyclingSessions,
    PerformedGymSessions, PerformedSessionLog, PerformedSetSummary, Placed, PlanAuthor, PlanStore,
    PrescribedWorkoutId, PrescribedWorkoutStore, Prescription, PrescriptionDeliverer,
    PrescriptionDeliveryStore, PrescriptionDestination, PrescriptionLifecycle, PublishedCell,
    PublishedCycling, PublishedProgrammes, PublishedRead, RawExtent, Reached, ReadUpTo, Reading,
    RecordedDevices, Rederivation, RefusalReport, RefusalReporter, RefusalStore, RenderingDigest,
    ReplyStatus, RestingHeartRateHistory, ResumptionPointResetter, ResumptionPointStore,
    RiddenSession, RiddenSessionLog, RiddenVenues, RideDestination, RidesPlaced, RunLock,
    RunSummary, SessionOrdinal, SourceAccount, SourceEvent, StreamStatus, Taken, Terms,
    Translation, Translator, UnderivableReason, UnderivableSlot, Unexpressed, WearRecord,
    WeighInHistory, WorkoutEventSource, WorkoutExtractor, WorkoutNormaliser, WorkoutPrescriber,
};
