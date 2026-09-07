//! One payload as the source served it, plus its provenance.

use super::{
    ids::{LandingRecordId, LandingStream, SourceRecordId},
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
    revision: PayloadDigest,
}

impl LandingRecord {
    /// The digest is computed here rather than accepted as an argument, so a
    /// record whose digest does not match its payload cannot be built.
    ///
    /// **The revision is the digest**, which is the ordinary case: a source
    /// whose payload is entirely about us has changed exactly when its bytes
    /// have. See [`Self::land_revision`] for the source that does not.
    pub fn land(
        stream: LandingStream,
        fetched_at: FetchedAt,
        source_record_id: SourceRecordId,
        provenance: Provenance,
        payload: RawPayload,
    ) -> Self {
        let digest = payload.digest();
        Self::land_revision(
            stream,
            fetched_at,
            source_record_id,
            provenance,
            payload,
            digest,
        )
    }

    /// Land a record whose payload carries parts that are not about us.
    ///
    /// **Two servings are the same revision when the parts that are ours are
    /// unchanged.** Peloton serves a workout with the class it was ridden to,
    /// and that class carries counters belonging to Peloton's whole membership
    /// — how many people are riding it at this moment, how many ever have, what
    /// they rated it. Those tick for reasons that have nothing to do with the
    /// operator, and comparing whole payloads would append a record to his
    /// history every time a stranger pressed start.
    ///
    /// So the *bytes* are still landed verbatim — § II.1 is not weakened, and
    /// nothing here parses, strips or rewrites what is stored — but *whether
    /// this is new* is asked of the part that is ours. Which part that is, is
    /// the adapter's to know: it is the only thing that understands the
    /// source's shape.
    pub fn land_revision(
        stream: LandingStream,
        fetched_at: FetchedAt,
        source_record_id: SourceRecordId,
        provenance: Provenance,
        payload: RawPayload,
        revision: PayloadDigest,
    ) -> Self {
        let digest = payload.digest();
        Self {
            stream,
            fetched_at,
            source_record_id,
            provenance,
            payload,
            digest,
            revision,
        }
    }

    /// What the next serving of this record is compared against.
    ///
    /// Equal to [`Self::digest`] unless the source declared volatile parts.
    pub const fn revision(&self) -> PayloadDigest {
        self.revision
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

/// A landing record that is already in the store, and so has an identity.
///
/// Distinct from [`LandingRecord`] rather than an optional field on it. A
/// record being appended has no id yet and a record being read back always
/// has one, and those are two different things a caller can hold — so an
/// `Option` here would put a question at every use site whose answer is
/// already known from which direction the record is travelling.
///
/// The derivation reads these. Extraction writes the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandedRecord {
    id: LandingRecordId,
    record: LandingRecord,
}

impl LandedRecord {
    pub const fn new(id: LandingRecordId, record: LandingRecord) -> Self {
        Self { id, record }
    }

    pub const fn id(&self) -> LandingRecordId {
        self.id
    }

    pub const fn record(&self) -> &LandingRecord {
        &self.record
    }

    pub const fn source_record_id(&self) -> &SourceRecordId {
        self.record.source_record_id()
    }

    pub const fn provenance(&self) -> &Provenance {
        self.record.provenance()
    }

    pub const fn payload(&self) -> &RawPayload {
        self.record.payload()
    }
}
