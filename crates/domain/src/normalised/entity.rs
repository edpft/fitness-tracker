//! What every normalised entity has, whatever discipline it belongs to.

use crate::landing::SourceRecordId;

/// One thing one source said, in our terms.
///
/// One method, because there is exactly one thing the derivation needs of an
/// entity generically: § II.3 makes provenance mandatory, and the identifier
/// the source names a record by is what lets a retraction find what it
/// withdraws. Everything else about an entity belongs to the entity.
///
/// It is deliberately not a home for behaviour the entities happen to share.
/// A trait that grows a `started_at` and a `duration` is a common supertype of
/// a gym workout and a bike ride, and there is no such thing — the whole reason
/// § 6 works structurally here is that the two do not have a shape in common.
pub trait NormalisedEntity {
    /// The identifier by which the source names the record this came from.
    fn source_record_id(&self) -> &SourceRecordId;
}
