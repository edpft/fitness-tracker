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
mod newtype;
pub mod payload;
pub mod provenance;
pub mod record;
pub mod run;
pub mod time;

pub use event::{EventKind, RawEventKind};
pub use ids::{
    EntityKind, InvalidIdentifier, InvalidStream, LandingStream, STREAM_SEPARATOR, SourceName,
    SourceRecordId,
};
pub use payload::{InvalidPayload, PayloadDigest, RawPayload, WrongDigestWidth};
pub use provenance::{Endpoint, EventProvenance, InvalidEndpoint, Provenance};
pub use record::LandingRecord;
pub use run::{
    EventCount, ExtractionRun, FailureReason, NegativeRunId, RecordCount, RunId, RunOutcome,
    UnknownFailureReason,
};
pub use time::{EventTime, FetchedAt, InvalidTimestamp, Watermark};
