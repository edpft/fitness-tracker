//! Raw landing: source responses persisted as received, before interpretation.
//!
//! The first input of the observation data model, and the only part of it this
//! feature builds. Nothing here knows what a set or a rep is — a payload is
//! bytes, and reading into it is the normalised layer's job.
//!
//! What it does know is the rules the layer carries: records are append-only,
//! provenance is mandatory, and what a source served that we do not recognise
//! is kept rather than discarded.

pub mod event;
pub mod ids;
pub mod payload;
pub mod record;
pub mod run;
pub mod time;

pub use event::{EventKind, RawEventKind};
pub use ids::{Endpoint, EntityKind, InvalidIdentifier, LandingStream, SourceName, SourceRecordId};
pub use payload::{InvalidPayload, PayloadDigest, RawPayload};
pub use record::LandingRecord;
pub use run::{EventCount, ExtractionRun, FailureReason, RecordCount, RunId, RunOutcome};
pub use time::{EventTime, FetchedAt, InvalidTimestamp, Watermark};
