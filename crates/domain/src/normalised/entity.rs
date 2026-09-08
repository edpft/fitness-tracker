//! What every normalised entity has, whatever discipline it belongs to.

use crate::landing::SourceRecordId;

/// One session, as one source told it.
///
/// One method, because there is exactly one thing the derivation needs of an
/// entity generically: § II.3 makes provenance mandatory, and the identifiers
/// the source names its records by are what let a retraction find what it
/// withdraws. Everything else about an entity belongs to the entity.
///
/// It is deliberately not a home for behaviour the entities happen to share.
/// A trait that grows a `started_at` and a `duration` is a common supertype of
/// a gym session and a cycling one, and there is no such thing — the whole
/// reason § 6 works structurally here is that the two do not have a shape in
/// common.
pub trait NormalisedEntity {
    /// Every record this entity composes, by the identifier the source names
    /// each one by.
    ///
    /// **Plural since § 3.1 made the session the unit** (constitution 3.2.0). A
    /// cycling session composes two or three of Peloton's workouts, and a
    /// retraction of any one of them is a retraction of the session: the source
    /// is no longer saying part of what the entity asserts, so the entity does
    /// not stand. Returning only the first would leave a session standing on a
    /// record its source had withdrawn.
    ///
    /// A `Vec` rather than an iterator because every implementation holds a
    /// handful — one for a Hevy record, at most three for a Peloton session —
    /// and a borrowed iterator through a trait object costs more than it saves.
    fn composes(&self) -> Vec<&SourceRecordId>;
}
