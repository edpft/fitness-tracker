//! The edge cases the specification reasoned out, made executable.
//!
//! Each was a reasoned default at specification time rather than a
//! source-confirmed fact. One of them — a deletion for a workout that was
//! never landed — turned out to be present in the real account on the very
//! first run.

mod support;

use application::{ExtractWorkouts, Extraction, ExtractionPorts};
use domain::landing::{EventKind, RecordCount};
use support::{
    FakeLock, FakeSource, Fallible, FixedClock, InMemoryLanding, InMemoryResumption, InMemoryRuns,
    deleted, events_endpoint, hevy_workouts, updated,
};

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

type Subject =
    Extraction<FakeSource, InMemoryLanding, InMemoryResumption, InMemoryRuns, FakeLock, FixedClock>;

fn harness(source: FakeSource) -> Fallible<(Subject, InMemoryLanding, InMemoryRuns)> {
    let landing = InMemoryLanding::default();
    let runs = InMemoryRuns::default();

    let extraction = Extraction::new(
        hevy_workouts()?,
        events_endpoint()?,
        ExtractionPorts {
            source,
            landing: landing.clone(),
            resumption: InMemoryResumption::default(),
            runs: runs.clone(),
            lock: FakeLock::free(),
            clock: FixedClock::at("2026-08-11T18:19:59Z")?,
        },
    );

    Ok((extraction, landing, runs))
}

/// Landed anyway. Raw records what the source asserted, and suppressing it
/// would require raw to consult its own history. A deletion standing alone is
/// a fact about the source, resolved at the canonical layer rather than here.
#[test]
fn a_deletion_for_a_workout_never_landed_is_landed_anyway() {
    runtime().expect("a tokio runtime").block_on(async {
        let source = FakeSource::serving(vec![vec![
            deleted("never-seen", "2025-11-05T20:02:27.905Z").expect("a fixture"),
        ]]);
        let (extraction, landing, _runs) = harness(source).expect("a harness");

        let summary = extraction
            .extract(&hevy_workouts().expect("a valid stream"))
            .await
            .expect("a run");

        assert_eq!(summary.records_landed, RecordCount::new(1));
        let held = landing.for_id("never-seen");
        assert_eq!(held.len(), 1);
        assert_eq!(
            held.first().map(|record| record.event_kind().clone()),
            Some(EventKind::Deleted)
        );
    });
}

/// Every distinct payload the source serves is landed, in the order served.
/// Where the source collapses repeated edits into a single current state, one
/// record results — and no edit is lost that the source still holds, because
/// raw cannot land what it was never served.
#[test]
fn repeated_edits_between_runs_land_each_payload_the_source_actually_served() {
    runtime().expect("a tokio runtime").block_on(async {
        let source = FakeSource::serving(vec![vec![
            updated("w1", r#"{"id":"w1","v":1}"#, "2026-08-01T00:00:00Z").expect("a fixture"),
        ]]);
        let (extraction, landing, _runs) = harness(source.clone()).expect("a harness");
        let stream = hevy_workouts().expect("a valid stream");

        extraction.extract(&stream).await.expect("the first run");

        // Two edits happen between runs; the source serves only the latest.
        source.now_serving(vec![vec![
            updated("w1", r#"{"id":"w1","v":3}"#, "2026-08-03T00:00:00Z").expect("a fixture"),
        ]]);
        extraction.extract(&stream).await.expect("the second run");

        let held = landing.for_id("w1");
        assert_eq!(held.len(), 2, "one record per payload actually served");
        assert!(
            String::from_utf8_lossy(held.last().expect("a record").payload().as_bytes())
                .contains(r#""v":3"#)
        );
    });
}

/// A workout edited to X, then Y, then back to X is the source contradicting
/// itself three times, and all three are landed: the third differs from what
/// is current even though it matches what came first.
#[test]
fn a_payload_reverted_to_an_earlier_state_lands_again() {
    runtime().expect("a tokio runtime").block_on(async {
        let source = FakeSource::serving(vec![vec![
            updated("w1", r#"{"id":"w1","v":"x"}"#, "2026-08-01T00:00:00Z").expect("a fixture"),
        ]]);
        let (extraction, landing, _runs) = harness(source.clone()).expect("a harness");
        let stream = hevy_workouts().expect("a valid stream");

        extraction.extract(&stream).await.expect("run one");

        source.now_serving(vec![vec![
            updated("w1", r#"{"id":"w1","v":"y"}"#, "2026-08-02T00:00:00Z").expect("a fixture"),
        ]]);
        extraction.extract(&stream).await.expect("run two");

        source.now_serving(vec![vec![
            updated("w1", r#"{"id":"w1","v":"x"}"#, "2026-08-03T00:00:00Z").expect("a fixture"),
        ]]);
        extraction.extract(&stream).await.expect("run three");

        assert_eq!(landing.for_id("w1").len(), 3);
    });
}

/// An empty account completes successfully, lands nothing, and is not reported
/// as a failure. This is also the steady state of a caught-up extraction.
#[test]
fn an_empty_account_is_a_success_rather_than_a_failure() {
    runtime().expect("a tokio runtime").block_on(async {
        let (extraction, landing, runs) = harness(FakeSource::empty()).expect("a harness");

        let summary = extraction
            .extract(&hevy_workouts().expect("a valid stream"))
            .await
            .expect("an empty account is not a failure");

        assert_eq!(summary.events_seen.as_u64(), 0);
        assert_eq!(summary.records_landed, RecordCount::new(0));
        assert!(landing.records().is_empty());
        assert!(
            runs.last_outcome()
                .is_some_and(|outcome| outcome.is_success()),
            "silence and success must read differently"
        );
    });
}

/// A kind the source adds later is unknown, not illegal. It is landed with the
/// kind recorded exactly as served, because raw retains what it does not
/// recognise rather than discarding it.
#[test]
fn an_unrecognised_event_kind_is_landed_with_its_kind_verbatim() {
    runtime().expect("a tokio runtime").block_on(async {
        let mut event = updated(
            "w1",
            r#"{"type":"archived","id":"w1"}"#,
            "2026-08-01T00:00:00Z",
        )
        .expect("a fixture");
        event.kind = EventKind::from_source("archived").expect("a readable kind");

        let (extraction, landing, _runs) =
            harness(FakeSource::serving(vec![vec![event]])).expect("a harness");

        extraction
            .extract(&hevy_workouts().expect("a valid stream"))
            .await
            .expect("an unknown kind is still an event");

        let held = landing.for_id("w1");
        assert_eq!(held.len(), 1);
        assert_eq!(
            held.first()
                .map(|record| record.event_kind().as_source_str()),
            Some("archived")
        );
    });
}

/// Every landed record names the source and the endpoint it came from, so a
/// reader can answer what it came from and when without consulting anything
/// else.
#[test]
fn every_landed_record_carries_its_provenance() {
    runtime().expect("a tokio runtime").block_on(async {
        let source = FakeSource::serving(vec![vec![
            updated("w1", r#"{"id":"w1"}"#, "2026-08-01T00:00:00Z").expect("a fixture"),
        ]]);
        let (extraction, landing, _runs) = harness(source).expect("a harness");

        extraction
            .extract(&hevy_workouts().expect("a valid stream"))
            .await
            .expect("a run");

        let record = landing.records().first().cloned().expect("a record");
        assert_eq!(record.source().as_str(), "hevy");
        assert_eq!(record.endpoint().as_str(), "/v1/workouts/events");
        assert_eq!(record.source_record_id().as_str(), "w1");
        assert_eq!(record.fetched_at().to_string(), "2026-08-11T18:19:59Z");
        assert_eq!(record.digest(), record.payload().digest());
    });
}
