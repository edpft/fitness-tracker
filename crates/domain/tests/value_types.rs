//! § 28: a randomly generated instance of a type must be valid.
//!
//! Every type here has exactly one way in, and that way validates. So the
//! property has two halves, and both matter: anything the conversion accepts
//! round-trips unchanged, and anything outside the invariant is refused rather
//! than quietly repaired. A conversion that trimmed or lowercased its input
//! would pass the first half and fail the second — and would be storing
//! something the source never said.
//!
//! `matches!` results are bound to a variable before asserting: `prop_assert!`
//! stringifies its expression into a format string, and a struct pattern's
//! braces are read as format directives.

use domain::landing::{
    Endpoint, EntityKind, EventKind, EventTime, FetchedAt, InvalidEndpoint, InvalidIdentifier,
    InvalidPayload, InvalidStream, LandingStream, RawPayload, SourceName, SourceRecordId,
    Watermark,
};
use proptest::prelude::*;

/// Non-empty, lowercase, no whitespace, no separator — what a source name and
/// an entity kind both require, and why is in `ids.rs`.
fn lowercase_token() -> impl Strategy<Value = String> {
    "[a-z0-9_]{1,32}"
}

/// Anything non-empty — the rule for an identifier whose format we do not own.
fn opaque_token() -> impl Strategy<Value = String> {
    "[!-~]{1,64}"
}

fn whitespace_bearing() -> impl Strategy<Value = String> {
    ("[a-z]{0,8}", "[ \t\n]", "[a-z]{0,8}").prop_map(|(a, w, b)| format!("{a}{w}{b}"))
}

proptest! {
    #[test]
    fn a_source_name_round_trips(token in lowercase_token()) {
        let name = SourceName::try_from(token.clone()).expect("a lowercase token is a source name");
        prop_assert_eq!(name.as_str(), token);
    }

    #[test]
    fn a_source_name_rejects_whitespace(token in whitespace_bearing()) {
        let refused = matches!(
            SourceName::try_from(token),
            Err(InvalidIdentifier::ContainsWhitespace { .. } | InvalidIdentifier::Empty { .. })
        );
        prop_assert!(refused, "whitespace must be refused, not trimmed away");
    }

    #[test]
    fn a_source_name_rejects_uppercase(token in "[A-Z][a-z]{0,8}") {
        let refused = matches!(
            SourceName::try_from(token),
            Err(InvalidIdentifier::NotLowercase { .. })
        );
        prop_assert!(refused, "case must be refused, not silently lowered");
    }

    /// The separator belongs to the stream name, so neither half may contain
    /// one: `hevy.workouts.extra` must not read as a stream at all.
    #[test]
    fn a_name_rejects_the_stream_separator(
        left in lowercase_token(),
        right in lowercase_token(),
    ) {
        let refused = matches!(
            SourceName::try_from(format!("{left}.{right}")),
            Err(InvalidIdentifier::ContainsSeparator { .. })
        );
        prop_assert!(refused);
    }

    #[test]
    fn an_entity_kind_round_trips(token in lowercase_token()) {
        let kind = EntityKind::try_from(token.clone()).expect("a lowercase token is an entity kind");
        prop_assert_eq!(kind.as_str(), token);
    }

    /// The name an operator types is the name the system prints back. Both
    /// directions, because a CLI argument is read with one and every message
    /// is written with the other.
    #[test]
    fn a_landing_stream_round_trips_through_its_text_form(
        source in lowercase_token(),
        entity in lowercase_token(),
    ) {
        let stream = LandingStream::new(
            SourceName::try_from(source.clone()).expect("valid source"),
            EntityKind::try_from(entity.clone()).expect("valid entity"),
        );
        prop_assert_eq!(stream.to_string(), format!("{source}.{entity}"));
        prop_assert_eq!(
            LandingStream::try_from(stream.to_string().as_str()).expect("its own text form"),
            stream
        );
    }

    #[test]
    fn a_stream_needs_both_halves(token in lowercase_token()) {
        let refused = matches!(
            LandingStream::try_from(token.as_str()),
            Err(InvalidStream::Malformed { .. })
        );
        prop_assert!(refused, "a stream is a source and an entity");
    }

    #[test]
    fn an_endpoint_round_trips(path in "(/[a-z0-9_]{1,12}){1,4}") {
        let endpoint = Endpoint::try_from(path.clone()).expect("an absolute path is an endpoint");
        prop_assert_eq!(endpoint.as_str(), path);
    }

    #[test]
    fn an_endpoint_must_be_absolute(path in "[a-z0-9_]{1,12}") {
        let refused = matches!(
            Endpoint::try_from(path),
            Err(InvalidEndpoint::NotAbsolutePath)
        );
        prop_assert!(refused, "a relative path is not an endpoint");
    }

    /// The identifier is not parsed as a UUID even though Hevy serves UUIDs,
    /// and nothing beyond non-empty is required of it: validating a source's
    /// format is interpretation, and would fail extraction to defend a
    /// constraint we do not own.
    #[test]
    fn a_source_record_id_accepts_any_non_empty_token(token in opaque_token()) {
        let id = SourceRecordId::try_from(token.clone()).expect("a non-empty token is an id");
        prop_assert_eq!(id.as_str(), token);
    }

    /// Including one with whitespace in it. A source that names a record
    /// `a b` has named it that, and refusing to land it would lose the record
    /// to defend a rule we invented.
    #[test]
    fn a_source_record_id_keeps_whatever_the_source_said(token in whitespace_bearing()) {
        prop_assume!(!token.is_empty());
        let id = SourceRecordId::try_from(token.clone()).expect("a non-empty token is an id");
        prop_assert_eq!(id.as_str(), token);
    }

    #[test]
    fn a_payload_round_trips(bytes in prop::collection::vec(any::<u8>(), 1..512)) {
        let payload = RawPayload::try_from(bytes.clone()).expect("non-empty bytes are a payload");
        prop_assert_eq!(payload.as_bytes(), bytes.as_slice());
    }

    /// An unrecognised kind is kept verbatim rather than normalised or
    /// rejected: a kind the source adds later is unknown, not illegal.
    #[test]
    fn an_unrecognised_event_kind_survives_verbatim(kind in "[a-z]{1,16}") {
        prop_assume!(kind != "updated" && kind != "deleted");
        let event = EventKind::try_from(kind.as_str()).expect("a non-empty kind is readable");
        prop_assert_eq!(event.as_str(), kind);
        let unrecognised = matches!(event, EventKind::Unrecognised(_));
        prop_assert!(unrecognised);
    }

    /// The fold a run uses to accumulate its resumption point. Order of
    /// observation must not matter: the feed serves newest-first, but a batch
    /// that fails and is retried can present the same events in any order.
    #[test]
    fn a_watermark_keeps_the_newest_event_time(
        offsets in prop::collection::vec(0i64..100_000, 1..32),
    ) {
        let base = Watermark::try_from("1970-01-01T00:00:00Z").expect("the epoch is valid");
        let times: Vec<EventTime> = offsets
            .iter()
            .map(|seconds| {
                EventTime::from(
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
            .map(|event| event.as_timestamp())
            .max()
            .expect("at least one event time");
        prop_assert_eq!(forwards.as_timestamp(), newest);
        prop_assert_eq!(backwards.as_timestamp(), newest);
    }

    /// A watermark never moves backwards, whatever it is shown. A run that
    /// re-reads old events after a retry must not rewind its own progress.
    #[test]
    fn a_watermark_never_retreats(start in 0i64..100_000, offset in 0i64..100_000) {
        let epoch = Watermark::EPOCH;
        let mark = Watermark::from(
            epoch
                .as_timestamp()
                .checked_add(jiff::Span::new().seconds(start))
                .expect("in range"),
        );
        let event = EventTime::from(
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
    assert_eq!(RawPayload::try_from(Vec::new()), Err(InvalidPayload::Empty));
}

#[test]
fn the_two_meaningful_event_kinds_are_distinguishable() {
    assert_eq!(
        EventKind::try_from("updated").expect("readable"),
        EventKind::Updated
    );
    assert_eq!(
        EventKind::try_from("deleted").expect("readable"),
        EventKind::Deleted
    );
}

#[test]
fn an_empty_event_kind_is_refused() {
    let refused = matches!(
        EventKind::try_from(""),
        Err(InvalidIdentifier::Empty { .. })
    );
    assert!(refused);
}

#[test]
fn a_timestamp_must_be_rfc_3339() {
    assert!(FetchedAt::try_from("2026-08-11T18:19:59Z").is_ok());
    // The source serves sub-second precision; it must survive the round trip.
    assert!(EventTime::try_from("2026-08-10T19:29:47.199Z").is_ok());
    assert!(FetchedAt::try_from("11/08/2026").is_err());
    assert!(FetchedAt::try_from("2026-08-11 18:19:59").is_err());
}

/// `str::parse` is what most callers reach for, and it must agree with the
/// conversion it delegates to.
#[test]
fn parsing_agrees_with_converting() {
    assert_eq!(
        "hevy.workouts".parse::<LandingStream>().expect("valid"),
        LandingStream::try_from("hevy.workouts").expect("valid")
    );
    assert_eq!(
        "2026-08-11T18:19:59Z".parse::<FetchedAt>().expect("valid"),
        FetchedAt::try_from("2026-08-11T18:19:59Z").expect("valid")
    );
}

/// A run id is a position in the store's sequence, and a negative one means
/// the file holds something this program did not write.
#[test]
fn a_run_id_cannot_be_negative() {
    use domain::landing::RunId;

    assert_eq!(RunId::try_from(7_i64).expect("positive").as_u64(), 7);
    assert!(RunId::try_from(-1_i64).is_err());
}
