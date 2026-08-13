//! The acceptance scenarios, driven through the ports with fakes.
//!
//! This is the primary suite: it exercises the whole use case with no
//! database, no network and no credential, so it runs inside the nix sandbox
//! and gates merge.
//!
//! Tests return `()` and assert by panicking. Returning a `Result` would trip
//! `panic_in_result_fn`, and `#[tokio::test]` on a `Result`-returning body
//! expands with an `#[allow]` for a forbidden lint, which is a hard compile
//! error. Both are the `forbid` levels working as intended.
//!
//! Fixture builders return `Result` and are unwrapped here at the call site,
//! because clippy's test exemption does not reach a free function in a test
//! file — see `support`.

mod support;

use application::{
    ExtractionError, ExtractionStatusReporter, ResumptionPointResetter, SourceEvent,
    WorkoutExtractor,
    extract::{Extraction, ExtractionPorts},
    status::ExtractionStatus,
};
use domain::landing::{EventKind, LandingRecord, Provenance, RecordCount, RunOutcome, Watermark};

/// Whether the source's last word on a record was that it is gone.
///
/// A plain match rather than a method on the kind: the two variants that carry
/// meaning are distinguishable precisely so a caller can ask this itself.
fn is_deletion(record: &LandingRecord) -> bool {
    kind_of(record) == EventKind::Deleted
}

/// What the source said happened, which lives in the record's provenance
/// because it is true of an events feed rather than of every source.
fn kind_of(record: &LandingRecord) -> EventKind {
    let Provenance::Event(event) = record.provenance();
    event.kind().clone()
}
use support::{
    FakeLock, FakeSource, Fallible, FixedClock, InMemoryLanding, InMemoryResumption, InMemoryRuns,
    deleted, hevy_workouts, updated,
};

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

/// The fakes a test keeps hold of so it can look inside after the run. Cloning
/// a fake shares it rather than copying it.
struct Harness {
    landing: InMemoryLanding,
    resumption: InMemoryResumption,
    runs: InMemoryRuns,
    source: FakeSource,
}

type Subject =
    Extraction<FakeSource, InMemoryLanding, InMemoryResumption, InMemoryRuns, FakeLock, FixedClock>;

type Reader = ExtractionStatus<InMemoryLanding, InMemoryResumption, InMemoryRuns>;

fn harness(source: FakeSource, lock: FakeLock) -> Fallible<(Subject, Harness)> {
    let landing = InMemoryLanding::holding(hevy_workouts()?);
    let runs = InMemoryRuns::default();
    let resumption = InMemoryResumption::default();

    let extraction = Extraction::new(ExtractionPorts {
        source: source.clone(),
        landing: landing.clone(),
        resumption: resumption.clone(),
        runs: runs.clone(),
        lock,
        // Frozen on purpose. Extraction must never take its resumption
        // point from the clock, so a clock that does not move is the
        // assertion rather than a simplification.
        clock: FixedClock::at("2026-08-11T18:19:59Z")?,
    });

    Ok((
        extraction,
        Harness {
            landing,
            resumption,
            runs,
            source,
        },
    ))
}

fn status_reader(seen: &Harness) -> Reader {
    ExtractionStatus::new(
        seen.landing.clone(),
        seen.resumption.clone(),
        seen.runs.clone(),
    )
}

/// The real account's shape: 17 pages carrying 164 events — 163 updates, plus
/// one deletion for a workout that has no update at all.
fn full_history() -> Fallible<Vec<Vec<SourceEvent>>> {
    let mut pages = Vec::new();
    let mut made = 0;
    for page in 0..17 {
        let mut events = Vec::new();
        for slot in 0..10 {
            if made >= 163 {
                break;
            }
            let day = 1 + (made % 27);
            events.push(updated(
                &format!("w{made}"),
                &format!(r#"{{"type":"updated","workout":{{"id":"w{made}","slot":{slot}}}}}"#),
                &format!("2026-08-{day:02}T00:00:0{}Z", made % 10),
            )?);
            made += 1;
        }
        if page == 16 {
            // The delete-only workout: landed, and correctly absent from the
            // count the source reports.
            events.push(deleted("gone", "2025-11-05T20:02:27.905Z")?);
        }
        if !events.is_empty() {
            pages.push(events);
        }
    }
    Ok(pages)
}

/// How many workouts are live: those whose most recent landing record is an
/// update rather than a deletion.
///
/// This is SC-001 as revised, and it stays correct once a landed workout is
/// later deleted — where counting by event kind alone would not.
fn live_workouts(landing: &InMemoryLanding) -> usize {
    landing
        .distinct_ids()
        .into_iter()
        .filter(|id| {
            landing
                .for_id(id)
                .last()
                .is_some_and(|record| !is_deletion(record))
        })
        .count()
}

// --- Scenario 1: first run --------------------------------------------------

#[test]
fn a_first_run_lands_every_workout_the_source_holds() {
    runtime().expect("a tokio runtime").block_on(async {
        let (extraction, seen) = harness(
            FakeSource::serving(full_history().expect("fixtures")),
            FakeLock::free(),
        )
        .expect("a harness");

        let summary = extraction.extract().await.expect("a first run");

        assert_eq!(summary.events_seen.as_u64(), 164);
        assert_eq!(summary.records_landed, RecordCount::from(164));
        assert_eq!(seen.landing.records().len(), 164);

        // SC-001: the source independently reports 163, and the extra landed
        // identifier is the workout that exists only as a deletion.
        assert_eq!(live_workouts(&seen.landing), 163);
        assert_eq!(seen.landing.distinct_ids().len(), 164);

        // The resumption point is the newest event time *seen*, never the
        // clock — which is frozen at 2026-08-11 and would be wrong. Derived
        // from the fixture rather than written out, so the assertion cannot
        // drift from what the fixture actually serves.
        let newest = full_history()
            .expect("fixtures")
            .into_iter()
            .flatten()
            .filter_map(|event| event.provenance.occurred_at())
            .max()
            .expect("the fixture carries event times");
        assert!(summary.resumption_point_moved);
        assert_eq!(summary.resumption_point, Some(Watermark::from(newest)));
        assert!(
            newest.as_timestamp()
                > Watermark::try_from("2026-08-11T18:19:59Z")
                    .expect("valid")
                    .as_timestamp(),
            "the fixture must reach past the frozen clock, or this proves nothing"
        );
    });
}

// --- Scenario 2: repeat run, nothing changed (SC-002) -----------------------

#[test]
fn a_repeat_run_over_unchanged_data_lands_nothing() {
    runtime().expect("a tokio runtime").block_on(async {
        let (extraction, seen) = harness(
            FakeSource::serving(full_history().expect("fixtures")),
            FakeLock::free(),
        )
        .expect("a harness");

        extraction.extract().await.expect("the first run");
        let after_first = seen.landing.records().len();

        let second = extraction.extract().await.expect("the second run");

        assert_eq!(seen.landing.records().len(), after_first, "SC-002");
        assert_eq!(second.records_landed, RecordCount::from(0));
        // Finding nothing new is a success, not silence.
        assert!(
            seen.runs
                .last_outcome()
                .is_some_and(|outcome| outcome.is_success())
        );
    });
}

// --- Scenario 3: a workout is edited ---------------------------------------

#[test]
fn an_edited_workout_lands_again_and_the_earlier_record_survives() {
    runtime().expect("a tokio runtime").block_on(async {
        let before_edit = vec![vec![
            updated(
                "w1",
                r#"{"type":"updated","workout":{"id":"w1","title":"before"}}"#,
                "2026-08-01T00:00:00Z",
            )
            .expect("a fixture"),
        ]];
        let (extraction, seen) =
            harness(FakeSource::serving(before_edit), FakeLock::free()).expect("a harness");

        extraction.extract().await.expect("the first run");
        let first = seen
            .landing
            .for_id("w1")
            .first()
            .cloned()
            .expect("a landed record");

        seen.source.now_serving(vec![vec![
            updated(
                "w1",
                r#"{"type":"updated","workout":{"id":"w1","title":"after"}}"#,
                "2026-08-02T00:00:00Z",
            )
            .expect("a fixture"),
        ]]);

        extraction.extract().await.expect("the second run");

        let held = seen.landing.for_id("w1");
        assert_eq!(held.len(), 2, "an edit lands a second record");
        // The earlier record is byte-identical and still retrievable.
        assert_eq!(held.first(), Some(&first));
        assert!(
            String::from_utf8_lossy(held.last().expect("the newer record").payload().as_bytes())
                .contains("after")
        );
    });
}

// --- Scenario 4: a workout is deleted --------------------------------------

#[test]
fn a_deletion_lands_a_record_and_alters_nothing() {
    runtime().expect("a tokio runtime").block_on(async {
        let before = vec![vec![
            updated(
                "w1",
                r#"{"type":"updated","workout":{"id":"w1"}}"#,
                "2026-08-01T00:00:00Z",
            )
            .expect("a fixture"),
        ]];
        let (extraction, seen) =
            harness(FakeSource::serving(before), FakeLock::free()).expect("a harness");

        extraction.extract().await.expect("the first run");
        let landed = seen
            .landing
            .for_id("w1")
            .first()
            .cloned()
            .expect("a landed record");

        // A deletion replaces the workout's entry in the feed rather than
        // adding one beside it, which is what the live API does.
        seen.source.now_serving(vec![vec![
            deleted("w1", "2026-08-03T00:00:00Z").expect("a fixture"),
        ]]);
        extraction.extract().await.expect("the second run");

        let held = seen.landing.for_id("w1");
        assert_eq!(held.len(), 2);
        assert_eq!(held.first(), Some(&landed), "nothing is altered");
        assert_eq!(held.last().map(kind_of), Some(EventKind::Deleted));
        // And the workout is no longer live, by the SC-001 reading.
        assert_eq!(live_workouts(&seen.landing), 0);
    });
}

// --- Scenario 5: an interrupted run (SC-004) --------------------------------

#[test]
fn an_interrupted_run_leaves_the_resumption_point_and_is_made_good_by_the_next() {
    runtime().expect("a tokio runtime").block_on(async {
        let (extraction, seen) = harness(
            FakeSource::serving(full_history().expect("fixtures")),
            FakeLock::free(),
        )
        .expect("a harness");

        seen.source.fail_on_page(9);
        let failure = extraction
            .extract()
            .await
            .expect_err("a failing page fails the run");
        assert!(matches!(failure, ExtractionError::Source(_)));

        // Pages already read are durable — deleting them would be a mutation
        // of raw — but the resumption point has not moved.
        assert!(!seen.landing.records().is_empty(), "earlier pages are kept");
        assert_eq!(seen.resumption.get(), None, "FR-006");
        assert!(matches!(
            seen.runs.last_outcome(),
            Some(RunOutcome::Failed { .. })
        ));

        seen.source.heal();
        extraction.extract().await.expect("the retry");

        // SC-004: the same end state as one uninterrupted run, with what was
        // already landed deduplicated rather than doubled.
        assert_eq!(seen.landing.records().len(), 164);
        assert_eq!(live_workouts(&seen.landing), 163);
        assert!(seen.resumption.get().is_some());
    });
}

// --- Scenario 6: an operator-requested re-fetch -----------------------------

#[test]
fn a_reset_and_rerun_lands_nothing_when_payloads_are_identical() {
    runtime().expect("a tokio runtime").block_on(async {
        let (extraction, seen) = harness(
            FakeSource::serving(full_history().expect("fixtures")),
            FakeLock::free(),
        )
        .expect("a harness");

        extraction.extract().await.expect("the first run");
        let after_first = seen.landing.records().len();

        status_reader(&seen)
            .reset()
            .await
            .expect("a reset succeeds");
        assert_eq!(seen.resumption.get(), None);

        let rerun = extraction.extract().await.expect("the rerun");

        assert_eq!(rerun.events_seen.as_u64(), 164, "everything is re-served");
        assert_eq!(
            rerun.records_landed,
            RecordCount::from(0),
            "identical payloads land nothing"
        );
        assert_eq!(seen.landing.records().len(), after_first);
    });
}

#[test]
fn a_reset_and_rerun_lands_only_what_actually_differs() {
    runtime().expect("a tokio runtime").block_on(async {
        let original = vec![vec![
            updated("w1", r#"{"id":"w1","v":1}"#, "2026-08-01T00:00:00Z").expect("a fixture"),
            updated("w2", r#"{"id":"w2","v":1}"#, "2026-08-02T00:00:00Z").expect("a fixture"),
        ]];
        let (extraction, seen) =
            harness(FakeSource::serving(original), FakeLock::free()).expect("a harness");

        extraction.extract().await.expect("the first run");
        status_reader(&seen).reset().await.expect("a reset");

        seen.source.now_serving(vec![vec![
            updated("w1", r#"{"id":"w1","v":1}"#, "2026-08-01T00:00:00Z").expect("a fixture"),
            updated("w2", r#"{"id":"w2","v":2}"#, "2026-08-02T00:00:00Z").expect("a fixture"),
        ]]);

        let rerun = extraction.extract().await.expect("the rerun");
        assert_eq!(rerun.records_landed, RecordCount::from(1), "exactly one");
        assert_eq!(seen.landing.for_id("w1").len(), 1);
        assert_eq!(seen.landing.for_id("w2").len(), 2);
    });
}

// --- Scenario 7: the source is unavailable ---------------------------------

#[test]
fn an_unreachable_source_leaves_raw_untouched_and_fails_visibly() {
    runtime().expect("a tokio runtime").block_on(async {
        let source = FakeSource::serving(full_history().expect("fixtures"));
        source.go_unreachable();
        let (extraction, seen) = harness(source, FakeLock::free()).expect("a harness");

        let failure = extraction
            .extract()
            .await
            .expect_err("an unreachable source fails the run");

        assert!(matches!(failure, ExtractionError::Source(_)));
        assert!(seen.landing.records().is_empty(), "raw is unchanged");
        assert_eq!(seen.resumption.get(), None, "the position does not move");
        // The failure is recorded, so it is visible rather than silent.
        assert!(matches!(
            seen.runs.last_outcome(),
            Some(RunOutcome::Failed { .. })
        ));

        // § 36: capabilities that do not depend on the source keep working.
        let standing = status_reader(&seen)
            .status()
            .await
            .expect("status answers even when the source does not");
        assert_eq!(standing.records_held, RecordCount::from(0));
        assert!(standing.last_success.is_none());
    });
}

// --- FR-010: one run at a time ---------------------------------------------

#[test]
fn a_concurrent_run_is_refused_and_changes_nothing() {
    runtime().expect("a tokio runtime").block_on(async {
        let (extraction, seen) = harness(
            FakeSource::serving(full_history().expect("fixtures")),
            FakeLock::already_held(),
        )
        .expect("a harness");

        let refused = extraction
            .extract()
            .await
            .expect_err("a second run is refused");

        assert!(matches!(refused, ExtractionError::AlreadyRunning));
        assert!(seen.landing.records().is_empty());
        assert_eq!(seen.resumption.get(), None);
        // It fails before a run is begun, so there is nothing to record.
        assert!(seen.runs.outcomes().is_empty());
        assert_eq!(seen.source.request_count(), 0, "the source is not touched");
    });
}
