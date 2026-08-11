//! The property change detection rests on.
//!
//! A landing record is written only when its payload's digest differs from the
//! digest of the most recent record for that source record. Everything about
//! whether extraction is idempotent — and whether an edit is noticed —
//! reduces to these two properties.
//!
//! Payloads are constructed inside each test rather than by a shared helper:
//! clippy's test exemption for `expect` reaches `#[test]` functions, and a
//! free helper in an integration test file is not one.

use domain::landing::{
    Endpoint, EventKind, FetchedAt, LandingRecord, PayloadDigest, RawPayload, SourceName,
    SourceRecordId,
};
use proptest::prelude::*;

proptest! {
    /// Re-serving identical bytes must land nothing. Without this, every run
    /// would duplicate the entire history.
    #[test]
    fn identical_bytes_digest_identically(bytes in prop::collection::vec(any::<u8>(), 1..1024)) {
        let left = RawPayload::new(bytes.clone()).expect("non-empty bytes");
        let right = RawPayload::new(bytes).expect("non-empty bytes");
        prop_assert_eq!(left.digest(), right.digest());
    }

    /// A changed payload must be noticed. Without this, an edit in Hevy would
    /// be silently dropped.
    #[test]
    fn differing_bytes_digest_differently(
        left in prop::collection::vec(any::<u8>(), 1..1024),
        right in prop::collection::vec(any::<u8>(), 1..1024),
    ) {
        prop_assume!(left != right);
        let left = RawPayload::new(left).expect("non-empty bytes");
        let right = RawPayload::new(right).expect("non-empty bytes");
        prop_assert_ne!(left.digest(), right.digest());
    }

    /// Byte order is part of the payload. A digest over an unordered view
    /// would call a reordered workout unchanged.
    #[test]
    fn order_is_significant(bytes in prop::collection::vec(any::<u8>(), 2..64)) {
        let mut reversed = bytes.clone();
        reversed.reverse();
        prop_assume!(reversed != bytes);
        let forwards = RawPayload::new(bytes).expect("non-empty bytes");
        let backwards = RawPayload::new(reversed).expect("non-empty bytes");
        prop_assert_ne!(forwards.digest(), backwards.digest());
    }

    /// A digest that survives the store must equal the one that produced it,
    /// or every rehydrated comparison would report a spurious change and land
    /// a duplicate record on every run.
    #[test]
    fn a_digest_survives_the_persistence_boundary(
        bytes in prop::collection::vec(any::<u8>(), 1..256),
    ) {
        let digest = RawPayload::new(bytes).expect("non-empty bytes").digest();
        prop_assert_eq!(PayloadDigest::from_storage(*digest.as_bytes()), digest);
    }

    /// A record's digest is computed from its own payload, never supplied
    /// alongside it, so the two cannot disagree.
    #[test]
    fn a_record_digests_its_own_payload(bytes in prop::collection::vec(any::<u8>(), 1..256)) {
        let body = RawPayload::new(bytes).expect("non-empty bytes");
        let record = LandingRecord::land(
            SourceName::new("hevy").expect("valid"),
            Endpoint::new("/v1/workouts/events").expect("valid"),
            FetchedAt::parse("2026-08-11T18:19:59Z").expect("valid"),
            SourceRecordId::new("b459cba5-cd6d-463c-abd6-54f8eafcadcb").expect("valid"),
            EventKind::Updated,
            None,
            body.clone(),
        );
        prop_assert_eq!(record.digest(), body.digest());
        prop_assert_eq!(record.payload(), &body);
    }
}

#[test]
fn a_digest_renders_as_hex() {
    let digest = RawPayload::new(b"hevy".to_vec())
        .expect("non-empty bytes")
        .digest();
    let rendered = digest.to_string();
    assert_eq!(rendered.len(), 64);
    assert!(rendered.chars().all(|c| c.is_ascii_hexdigit()));
}

/// A deletion carries no workout body, but it is still a payload: the event
/// object the source served. Landing it is what makes a deletion a record
/// rather than a removal.
#[test]
fn a_deletion_is_a_record_like_any_other() {
    let body = RawPayload::new(
        br#"{"type":"deleted","id":"93d5","deleted_at":"2025-11-05T20:02:27.905Z"}"#.to_vec(),
    )
    .expect("non-empty bytes");
    let record = LandingRecord::land(
        SourceName::new("hevy").expect("valid"),
        Endpoint::new("/v1/workouts/events").expect("valid"),
        FetchedAt::parse("2026-08-11T18:19:59Z").expect("valid"),
        SourceRecordId::new("93d5").expect("valid"),
        EventKind::Deleted,
        None,
        body,
    );
    assert!(record.event_kind().is_deletion());
    assert_eq!(record.event_time(), None);
}
