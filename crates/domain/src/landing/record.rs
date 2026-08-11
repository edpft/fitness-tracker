//! One workout payload as the source served it, plus its provenance.

use super::{
    event::EventKind,
    ids::{Endpoint, SourceName, SourceRecordId},
    payload::{PayloadDigest, RawPayload},
    time::{EventTime, FetchedAt},
};

/// A landing record. Immutable.
///
/// There is no setter, no `&mut` accessor and no update path through any port.
/// The store enforces the same thing independently with triggers, so the
/// guarantee does not rest on this type alone — nor on anyone remembering it.
///
/// Note there is no fallible constructor. Every component arrives already
/// validated, so a record that exists is a record with complete provenance;
/// there is no state left to reject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandingRecord {
    source: SourceName,
    endpoint: Endpoint,
    fetched_at: FetchedAt,
    source_record_id: SourceRecordId,
    event_kind: EventKind,
    event_time: Option<EventTime>,
    payload: RawPayload,
    digest: PayloadDigest,
}

impl LandingRecord {
    /// The digest is computed here rather than accepted as an argument, so a
    /// record whose digest does not match its payload cannot be built.
    pub fn land(
        source: SourceName,
        endpoint: Endpoint,
        fetched_at: FetchedAt,
        source_record_id: SourceRecordId,
        event_kind: EventKind,
        event_time: Option<EventTime>,
        payload: RawPayload,
    ) -> Self {
        let digest = payload.digest();
        Self {
            source,
            endpoint,
            fetched_at,
            source_record_id,
            event_kind,
            event_time,
            payload,
            digest,
        }
    }

    pub fn source(&self) -> &SourceName {
        &self.source
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    pub fn fetched_at(&self) -> FetchedAt {
        self.fetched_at
    }

    pub fn source_record_id(&self) -> &SourceRecordId {
        &self.source_record_id
    }

    pub fn event_kind(&self) -> &EventKind {
        &self.event_kind
    }

    pub fn event_time(&self) -> Option<EventTime> {
        self.event_time
    }

    pub fn payload(&self) -> &RawPayload {
        &self.payload
    }

    pub fn digest(&self) -> PayloadDigest {
        self.digest
    }
}
