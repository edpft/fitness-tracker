//! Everything Peloton says about one ride.
//!
//! **Two responses, and § 3.1 is what allows them to be one entity.** The
//! workout list states a ride's start, duration and device; the performance
//! graph states its samples and its distance. The graph names no workout,
//! carries no time, no zone and no device, so neither response is an entity on
//! its own — and nothing is being reconciled, because the two do not overlap.
//! There are no rival claims to prefer between.
//!
//! Peloton's shape rather than the store's, which is why it is declared here
//! and not beside the query that fills it. What one source's account of one
//! thing consists of is a fact about that source.

use domain::landing::LandedRecord;

/// One ride, as far as Peloton has been asked about it.
#[derive(Debug, Clone)]
pub struct RideAccount {
    /// The workout record. What identifies the ride, and the only one of the
    /// two the source names.
    pub ride: LandedRecord,
    /// The performance graph, where one has landed.
    ///
    /// **Optional, because the two streams are collected independently.** They
    /// resume, run and lock apart on purpose — a walk of the graphs takes
    /// minutes and a walk of the list takes seconds — so a ride collected since
    /// the last walk of the graphs genuinely has no samples yet. That is not an
    /// error and not a gap in the model; it is a reason to collect the other
    /// stream, and the translator says so.
    pub samples: Option<LandedRecord>,
}
