//! The invariant the whole feature rests on.
//!
//! > The resumption point advances to the newest event time the run **saw**,
//! > and never to the clock.
//!
//! The feed serves newest first, so a workout edited while a run is walking it
//! gets promoted above pages the run has already read, and that run can miss
//! it. Because the point never passes an event the run observed, and the edit
//! is by definition newer than anything it observed, the *next* run collects
//! it. Taking the point from the clock instead would step over that edit
//! permanently, and nothing would look wrong.
//!
//! These tests fail loudly if that rule is ever relaxed.

mod support;

use application::{
    WorkoutExtractor,
    extract::{Extraction, ExtractionPorts},
};
use domain::landing::Watermark;
use support::{
    FakeLock, FakeSource, Fallible, FixedClock, InMemoryLanding, InMemoryResumption, InMemoryRuns,
    hevy_workouts, untimed, updated,
};

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

type Subject =
    Extraction<FakeSource, InMemoryLanding, InMemoryResumption, InMemoryRuns, FakeLock, FixedClock>;

/// The clock is frozen well after every event time in these fixtures, so any
/// resumption point taken from it would be visibly wrong.
const FROZEN_CLOCK: &str = "2030-01-01T00:00:00Z";

fn harness(
    source: FakeSource,
) -> Fallible<(Subject, InMemoryLanding, InMemoryResumption, InMemoryRuns)> {
    let landing = InMemoryLanding::holding(hevy_workouts()?);
    let resumption = InMemoryResumption::default();
    let runs = InMemoryRuns::default();

    let extraction = Extraction::new(ExtractionPorts {
        source,
        landing: landing.clone(),
        resumption: resumption.clone(),
        runs: runs.clone(),
        lock: FakeLock::free(),
        clock: FixedClock::at(FROZEN_CLOCK)?,
    });

    Ok((extraction, landing, resumption, runs))
}

/// A workout edited mid-run is missed by that run and collected by the next.
///
/// Pages of two. The run reads page 1 = [a, b]. Then `c` is edited, which
/// promotes it to the head of the feed and pushes everything down, so page 2
/// is now [b, d] — and `c`, which the run never saw, sits on page 1 where the
/// run has already been.
#[test]
fn an_edit_promoted_past_a_read_page_is_collected_by_the_next_run() {
    runtime().expect("a tokio runtime").block_on(async {
        let before = vec![
            vec![
                updated("a", r#"{"id":"a"}"#, "2026-08-04T00:00:00Z").expect("a fixture"),
                updated("b", r#"{"id":"b"}"#, "2026-08-03T00:00:00Z").expect("a fixture"),
            ],
            vec![
                updated("c", r#"{"id":"c","v":1}"#, "2026-08-02T00:00:00Z").expect("a fixture"),
                updated("d", r#"{"id":"d"}"#, "2026-08-01T00:00:00Z").expect("a fixture"),
            ],
        ];
        // `c` is edited during the run and jumps to the front.
        let after = vec![
            vec![
                updated("c", r#"{"id":"c","v":2}"#, "2026-08-09T00:00:00Z").expect("a fixture"),
                updated("a", r#"{"id":"a"}"#, "2026-08-04T00:00:00Z").expect("a fixture"),
            ],
            vec![
                updated("b", r#"{"id":"b"}"#, "2026-08-03T00:00:00Z").expect("a fixture"),
                updated("d", r#"{"id":"d"}"#, "2026-08-01T00:00:00Z").expect("a fixture"),
            ],
        ];

        let source = FakeSource::serving(before);
        source.swap_after_page(1, after);
        let (extraction, landing, resumption, _runs) = harness(source).expect("a harness");

        extraction.extract().await.expect("the first run");

        // The edit was missed, exactly as expected — this is a real race, not
        // something the design pretends away.
        assert!(landing.for_id("c").is_empty(), "the edit was missed");

        // And here is why that is survivable: the resumption point sits at the
        // newest event the run actually saw, which is older than the edit.
        let mark = resumption.get().expect("a resumption point");
        assert_eq!(
            mark,
            Watermark::try_from("2026-08-04T00:00:00Z").expect("valid"),
            "the point is the newest event seen, not the newest that exists"
        );
        assert!(
            mark.as_timestamp()
                < Watermark::try_from("2026-08-09T00:00:00Z")
                    .expect("valid")
                    .as_timestamp(),
            "the point must not have passed the edit"
        );

        // So the next run collects it.
        extraction.extract().await.expect("the second run");
        assert_eq!(landing.for_id("c").len(), 1, "the next run collects it");
    });
}

/// The clock is never the source of the resumption point.
///
/// Frozen decades ahead of every event: if the point were taken from it, this
/// would be obvious, and the previous test's edit would be lost for good.
#[test]
fn the_resumption_point_never_comes_from_the_clock() {
    runtime().expect("a tokio runtime").block_on(async {
        let source = FakeSource::serving(vec![vec![
            updated("a", r#"{"id":"a"}"#, "2026-08-04T00:00:00Z").expect("a fixture"),
        ]]);
        let (extraction, _landing, resumption, _runs) = harness(source).expect("a harness");

        extraction.extract().await.expect("a run");

        let mark = resumption.get().expect("a resumption point");
        assert_eq!(
            mark,
            Watermark::try_from("2026-08-04T00:00:00Z").expect("valid")
        );
        assert!(
            mark.as_timestamp()
                < Watermark::try_from(FROZEN_CLOCK)
                    .expect("valid")
                    .as_timestamp(),
            "the clock is not a resumption point"
        );
    });
}

/// A run that is served nothing leaves the point where it was. With `since`
/// inclusive and the feed newest-first, "nothing since the point" means there
/// is nothing to advance to.
#[test]
fn a_run_that_sees_nothing_leaves_the_point_alone() {
    runtime().expect("a tokio runtime").block_on(async {
        let (extraction, landing, resumption, _runs) =
            harness(FakeSource::empty()).expect("a harness");

        let summary = extraction
            .extract()
            .await
            .expect("an empty account is not a failure");

        assert_eq!(summary.events_seen.as_u64(), 0);
        assert!(!summary.resumption_point_moved);
        assert_eq!(resumption.get(), None);
        assert!(landing.records().is_empty());
    });
}

/// An event with no timestamp contributes nothing to the point rather than
/// borrowing the fetch time — which would risk stepping over events this run
/// never saw.
#[test]
fn an_event_without_a_timestamp_does_not_move_the_point() {
    runtime().expect("a tokio runtime").block_on(async {
        let source = FakeSource::serving(vec![vec![
            untimed("a", r#"{"id":"a"}"#).expect("a fixture"),
        ]]);
        let (extraction, landing, resumption, _runs) = harness(source).expect("a harness");

        let summary = extraction.extract().await.expect("a run");

        // It is landed — § 37 records partial data as partial rather than
        // discarding it.
        assert_eq!(landing.for_id("a").len(), 1);
        assert_eq!(
            landing
                .for_id("a")
                .first()
                .and_then(|record| record.provenance().occurred_at()),
            None
        );
        // But it moves nothing.
        assert!(!summary.resumption_point_moved);
        assert_eq!(resumption.get(), None);
    });
}

/// The point never retreats, even when the source re-serves older events after
/// it has advanced.
#[test]
fn the_point_never_retreats() {
    runtime().expect("a tokio runtime").block_on(async {
        let source = FakeSource::serving(vec![vec![
            updated("a", r#"{"id":"a"}"#, "2026-08-09T00:00:00Z").expect("a fixture"),
        ]]);
        let (extraction, _landing, resumption, _runs) = harness(source.clone()).expect("a harness");

        extraction.extract().await.expect("the first run");
        let advanced = resumption.get().expect("a resumption point");

        // The source now serves only something older. `since` is inclusive, so
        // a correct implementation would not even be shown this — but if it
        // were, the point must hold.
        source.now_serving(vec![vec![
            updated("b", r#"{"id":"b"}"#, "2026-08-01T00:00:00Z").expect("a fixture"),
        ]]);
        extraction.extract().await.expect("the second run");

        assert_eq!(
            resumption.get(),
            Some(advanced),
            "the point does not retreat"
        );
    });
}
