//! Use cases, and the ports through which they talk to the outside world.
//!
//! Depends only on `domain`. The ports are defined *here*, in terms the
//! application understands, and are implemented out in `infrastructure`,
//! `cli` and `web` — that inversion is what keeps adapters swappable, and what
//! lets a use case be tested against fakes with no I/O anywhere near it.

pub mod error;
pub mod extract;
pub mod paging;
pub mod ports;
pub mod status;

pub use extract::{Extraction, ExtractionPorts};
pub use status::ExtractionStatus;

pub use error::{ExtractionError, RunLockError, SourceError, StatusError, StoreError};
pub use paging::{PageCount, PageNumber};
pub use ports::{
    Clock, EventPage, ExtractWorkouts, ExtractionRunLog, LandingStore, ReportExtractionStatus,
    ResetResumptionPoint, ResumptionPointStore, RunLock, RunSummary, SourceEvent, StreamStatus,
    WorkoutEventSource,
};
