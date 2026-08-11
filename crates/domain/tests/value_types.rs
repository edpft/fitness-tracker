//! § 28: a randomly generated instance of a type must be valid.
//!
//! Every type here has exactly one way in, and that way validates. So the
//! property has two halves, and both matter: anything the constructor accepts
//! round-trips unchanged, and anything outside the invariant is refused rather
//! than quietly repaired. A constructor that trimmed or lowercased its input
//! would pass the first half and fail the second — and would be storing
//! something the source never said.
//!
//! `matches!` results are bound to a variable before asserting: `prop_assert!`
//! stringifies its expression into a format string, and a struct pattern's
//! braces are read as format directives.

use domain::landing::{
    Endpoint, EntityKind, EventKind, EventTime, FetchedAt, InvalidIdentifier, InvalidPayload,
    LandingStream, RawPayload, SourceName, SourceRecordId, Watermark,
};
use proptest::prelude::*;

/// Non-empty, lowercase, no whitespace — what a source name and an entity
/// kind both require.
fn lowercase_token() -> impl Strategy<Value = String> {
    "[a-z0-9_]{1,32}"
}

/// Non-empty, no whitespace, any case — the looser rule for identifiers whose
/// format we do not own.
fn opaque_token() -> impl Strategy<Value = String> {
    "[!-~]{1,64}"
}

fn whitespace_bearing() -> impl Strategy<Value = String> {
    ("[a-z]{0,8}", "[ \t\n]", "[a-z]{0,8}").prop_map(|(a, w, b)| format!("{a}{w}{b}"))
}

proptest! {
    #[test]
    fn a_source_name_round_trips(token in lowercase_token()) {
        let name = SourceName::new(token.clone()).expect("a lowercase token is a valid source name");
        prop_assert_eq!(name.as_str(), token);
    }

    #[test]
    fn a_source_name_rejects_whitespace(token in whitespace_bearing()) {
        let refused = matches!(
            SourceName::new(token),
            Err(InvalidIdentifier::ContainsWhitespace { .. } | InvalidIdentifier::Empty { .. })
        );
        prop_assert!(refused, "whitespace must be refused, not trimmed away");
    }

    #[test]
    fn a_source_name_rejects_uppercase(token in "[A-Z][a-z]{0,8}") {
        let refused = matches!(
            SourceName::new(token),
            Err(InvalidIdentifier::NotLowercase { .. })
        );
        prop_assert!(refused, "case must be refused, not silently lowered");
    }

    #[test]
    fn an_entity_kind_round_trips(token in lowercase_token()) {
        let kind = EntityKind::new(token.clone()).expect("a lowercase token is a valid entity kind");
        prop_assert_eq!(kind.as_str(), token);
    }

    #[test]
    fn a_landing_stream_reads_as_source_dot_entity(
        source in lowercase_token(),
        entity in lowercase_token(),
    ) {
        let stream = LandingStream::new(
            SourceName::new(source.clone()).expect("valid source"),
            EntityKind::new(entity.clone()).expect("valid entity"),
        );
        prop_assert_eq!(stream.to_string(), format!("{source}.{entity}"));
    }

    #[test]
    fn an_endpoint_round_trips(path in "(/[a-z0-9_]{1,12}){1,4}") {
        let endpoint = Endpoint::new(path.clone()).expect("an absolute path is a valid endpoint");
        prop_assert_eq!(endpoint.as_str(), path);
    }

    #[test]
    fn an_endpoint_must_be_absolute(path in "[a-z0-9_]{1,12}") {
        let refused = matches!(
            Endpoint::new(path),
            Err(InvalidIdentifier::NotAbsolutePath { .. })
        );
        prop_assert!(refused, "a relative path is not an endpoint");
    }

    /// The identifier is not parsed as a UUID even though Hevy serves UUIDs.
    /// Anything non-empty and whitespace-free is accepted verbatim, because
    /// validating a source's format is interpretation — and would fail
    /// extraction to defend a constraint we do not own.
    #[test]
    fn a_source_record_id_accepts_any_opaque_token(token in opaque_token()) {
        let id = SourceRecordId::new(token.clone()).expect("an opaque token is a valid id");
        prop_assert_eq!(id.as_str(), token);
    }

    #[test]
    fn a_source_record_id_rejects_whitespace(token in whitespace_bearing()) {
        let refused = matches!(
            SourceRecordId::new(token),
            Err(InvalidIdentifier::ContainsWhitespace { .. } | InvalidIdentifier::Empty { .. })
        );
        prop_assert!(refused, "whitespace must be refused");
    }

    #[test]
    fn a_payload_round_trips(bytes in prop::collection::vec(any::<u8>(), 1..512)) {
        let payload = RawPayload::new(bytes.clone()).expect("non-empty bytes are a valid payload");
        prop_assert_eq!(payload.as_bytes(), bytes.as_slice());
    }

    /// An unrecognised kind is kept verbatim rather than normalised or
    /// rejected: a kind the source adds later is unknown, not illegal.
    #[test]
    fn an_unrecognised_event_kind_survives_verbatim(kind in "[a-z]{1,16}") {
        prop_assume!(kind != "updated" && kind != "deleted");
        let event = EventKind::from_source(&kind).expect("a non-empty kind is readable");
        prop_assert_eq!(event.as_source_str(), kind);
        let unrecognised = matches!(event, EventKind::Unrecognised(_));
        prop_assert!(unrecognised);
    }

    /// The fold a run uses to accumulate its resumption point. Order of
    /// observation must not matter: the feed serves newest-first, but a page
    /// that fails and is retried can present the same events in any order.
    #[test]
    fn a_watermark_keeps_the_newest_event_time(
        offsets in prop::collection::vec(0i64..100_000, 1..32),
    ) {
        let base = Watermark::parse("1970-01-01T00:00:00Z").expect("the epoch is valid");
        let times: Vec<EventTime> = offsets
            .iter()
            .map(|seconds| {
                EventTime::new(
                    base.as_timestamp()
                        .checked_add(jiff::Span::new().seconds(*seconds))
                        .expect("an offset within a century of the epoch"),
                )
            })
            .collect();

        let forwards = times.iter().fold(base, |mark, event| mark.advanced_to(*event));
        let mut reversed = times.clone();
        reversed.reverse();
        let backwards = reversed.iter().fold(base, |mark, event| mark.advanced_to(*event));

        let newest = times
            .iter()
            .map(EventTime::as_timestamp)
            .max()
            .expect("at least one event time");
        prop_assert_eq!(forwards.as_timestamp(), newest);
        prop_assert_eq!(backwards.as_timestamp(), newest);
    }

    /// A watermark never moves backwards, whatever it is shown. A run that
    /// re-reads old events after a retry must not rewind its own progress.
    #[test]
    fn a_watermark_never_retreats(start in 0i64..100_000, offset in 0i64..100_000) {
        let epoch = Watermark::parse("1970-01-01T00:00:00Z").expect("the epoch is valid");
        let mark = Watermark::new(
            epoch
                .as_timestamp()
                .checked_add(jiff::Span::new().seconds(start))
                .expect("in range"),
        );
        let event = EventTime::new(
            epoch
                .as_timestamp()
                .checked_add(jiff::Span::new().seconds(offset))
                .expect("in range"),
        );
        prop_assert!(mark.advanced_to(event).as_timestamp() >= mark.as_timestamp());
    }
}

#[test]
fn an_empty_payload_is_refused() {
    assert_eq!(RawPayload::new(Vec::new()), Err(InvalidPayload::Empty));
}

#[test]
fn the_two_meaningful_event_kinds_are_distinguishable() {
    assert_eq!(
        EventKind::from_source("updated").expect("readable"),
        EventKind::Updated
    );
    assert_eq!(
        EventKind::from_source("deleted").expect("readable"),
        EventKind::Deleted
    );
    assert!(EventKind::Deleted.is_deletion());
    assert!(!EventKind::Updated.is_deletion());
}

#[test]
fn an_empty_event_kind_is_refused() {
    let refused = matches!(
        EventKind::from_source(""),
        Err(InvalidIdentifier::Empty { .. })
    );
    assert!(refused);
}

#[test]
fn a_timestamp_must_be_rfc_3339() {
    assert!(FetchedAt::parse("2026-08-11T18:19:59Z").is_ok());
    // The source serves sub-second precision; it must survive the round trip.
    assert!(EventTime::parse("2026-08-10T19:29:47.199Z").is_ok());
    assert!(FetchedAt::parse("11/08/2026").is_err());
    assert!(FetchedAt::parse("2026-08-11 18:19:59").is_err());
}
