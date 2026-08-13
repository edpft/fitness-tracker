//! One payload as the source served it, plus its provenance.

use super::{
    ids::{LandingStream, SourceRecordId},
    payload::{PayloadDigest, RawPayload},
    provenance::Provenance,
    time::FetchedAt,
};

/// A landing record. Immutable.
///
/// There is no setter, no `&mut` accessor and no update path through any port.
/// The store enforces the same thing independently with triggers, so the
/// guarantee does not rest on this type alone — nor on anyone remembering it.
///
/// The fields here are the ones every record has whatever served it: which
/// stream it belongs to, what that source calls it, when we asked, and the
/// bytes we were given. Anything true only of the transport that carried it is
/// in [`Provenance`].
///
/// Note there is no fallible constructor. Every component arrives already
/// validated, so a record that exists is a record with complete provenance;
/// there is no state left to reject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandingRecord {
    stream: LandingStream,
    fetched_at: FetchedAt,
    source_record_id: SourceRecordId,
    provenance: Provenance,
    payload: RawPayload,
    digest: PayloadDigest,
}

impl LandingRecord {
    /// The digest is computed here rather than accepted as an argument, so a
    /// record whose digest does not match its payload cannot be built.
    pub fn land(
        stream: LandingStream,
        fetched_at: FetchedAt,
        source_record_id: SourceRecordId,
        provenance: Provenance,
        payload: RawPayload,
    ) -> Self {
        let digest = payload.digest();
        Self {
            stream,
            fetched_at,
            source_record_id,
            provenance,
            payload,
            digest,
        }
    }

    pub const fn stream(&self) -> &LandingStream {
        &self.stream
    }

    pub const fn fetched_at(&self) -> FetchedAt {
        self.fetched_at
    }

    pub const fn source_record_id(&self) -> &SourceRecordId {
        &self.source_record_id
    }

    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    pub const fn payload(&self) -> &RawPayload {
        &self.payload
    }

    pub const fn digest(&self) -> PayloadDigest {
        self.digest
    }
}
