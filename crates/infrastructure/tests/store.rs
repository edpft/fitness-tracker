//! The store, exercised through its ports against a real SQLite file.
//!
//! Two constraints shape how these are written.
//!
//! Helpers propagate with `?` rather than unwrapping: clippy's test exemption
//! for `unwrap` and `expect` reaches `#[test]` functions, and a free helper in
//! an integration test file is not one.
//!
//! And `#[tokio::test]` is not used. On a test that returns a `Result` its
//! expansion carries `#[allow(clippy::expect_used)]`, and an `allow` for a
//! forbidden lint is a compile error (E0453) — which is the whole point of
//! `forbid` over `deny`. Building the runtime by hand costs one helper and
//! keeps the rule intact.
//!
//! Tests therefore return `()` and panic on failure, rather than returning a
//! `Result` and propagating: `panic_in_result_fn` is forbidden too, and an
//! `assert!` in a function returning `Result` trips it. That is also the style
//! `clippy.toml` is configured for — `allow-unwrap-in-tests` and its siblings
//! exist precisely so a test can assert by panicking.

use std::error::Error;

use application::{ExtractionRunLog, LandingStore, ResumptionPointStore};
use domain::landing::{
    Endpoint, EventKind, EventProvenance, EventTime, FetchedAt, LandingRecord, LandingStream,
    RawPayload, RecordCount, RunId, RunOutcome, SourceRecordId, Watermark,
};
use infrastructure::{
    HevyWorkoutLandingStore, SqliteExtractionRunLog, SqliteResumptionPointStore, connect,
};
use sqlx::SqlitePool;
use tempfile::TempDir;

type Fallible<T> = Result<T, Box<dyn Error>>;

/// A runtime for one test. Free functions cannot panic, so this returns the
/// error and the caller — which is a `#[test]` function — unwraps it.
fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

// Free helpers propagate with `?`. Clippy's test exemption for `expect` covers
// a `#[test]` function and the async block inside it, but not a free function
// alongside them.

async fn store() -> Fallible<(TempDir, SqlitePool)> {
    let directory = TempDir::new()?;
    let pool = connect(&directory.path().join("fitness.db")).await?;
    Ok((directory, pool))
}

fn hevy_workouts() -> Fallible<LandingStream> {
    Ok(LandingStream::try_from("hevy.workouts")?)
}

fn record(id: &str, kind: EventKind, body: &[u8], at: &str) -> Fallible<LandingRecord> {
    Ok(LandingRecord::land(
        hevy_workouts()?,
        FetchedAt::try_from("2026-08-11T18:19:59Z")?,
        SourceRecordId::try_from(id)?,
        EventProvenance::new(
            Endpoint::try_from("/v1/workouts/events")?,
            kind,
            Some(EventTime::try_from(at)?),
        )
        .into(),
        RawPayload::try_from(body)?,
    ))
}

async fn a_run(pool: &SqlitePool) -> Fallible<RunId> {
    let log = SqliteExtractionRunLog::new(pool.clone());
    Ok(log
        .begin(
            &hevy_workouts()?,
            FetchedAt::try_from("2026-08-11T18:19:59Z")?,
        )
        .await?)
}

// --- Raw is append-only -----------------------------------------------------

/// SC-003, checked against the database rather than against our own code. The
/// guarantee holds for any writer, including a stray `sqlite3` session, which
/// is the point of enforcing it here.
#[test]
fn the_store_refuses_to_update_a_landing_record() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let run = a_run(&pool).await.expect("store operation");
        let landing = HevyWorkoutLandingStore::new(pool.clone()).expect("a wired stream");
        landing
            .append(
                run,
                vec![
                    record("w1", EventKind::Updated, b"{}", "2026-08-01T00:00:00Z")
                        .expect("valid test fixture"),
                ],
            )
            .await
            .expect("store operation");

        let refused = sqlx::query("UPDATE hevy_workout_landing SET payload = X'00' WHERE id = 1")
            .execute(&pool)
            .await;

        let error = refused.expect_err("an update must be refused").to_string();
        assert!(
            error.contains("append-only"),
            "the refusal should say why: {error}"
        );
    });
}

#[test]
fn the_store_refuses_to_delete_a_landing_record() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let run = a_run(&pool).await.expect("store operation");
        let landing = HevyWorkoutLandingStore::new(pool.clone()).expect("a wired stream");
        landing
            .append(
                run,
                vec![
                    record("w1", EventKind::Updated, b"{}", "2026-08-01T00:00:00Z")
                        .expect("valid test fixture"),
                ],
            )
            .await
            .expect("store operation");

        let refused = sqlx::query("DELETE FROM hevy_workout_landing WHERE id = 1")
            .execute(&pool)
            .await;

        let error = refused.expect_err("a delete must be refused").to_string();
        assert!(
            error.contains("append-only"),
            "the refusal should say why: {error}"
        );
    });
}

// --- Change detection -------------------------------------------------------

/// The comparison is against the *most recent* record, not any record. A
/// workout edited to X, then Y, then back to X is the source serving three
/// payloads, and the third differs from the second even though it matches the
/// first.
#[test]
fn the_latest_digest_is_the_most_recent_not_any() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let run = a_run(&pool).await.expect("store operation");
        let landing = HevyWorkoutLandingStore::new(pool.clone()).expect("a wired stream");
        let id = SourceRecordId::try_from("w1").expect("valid test fixture");

        let first = record(
            "w1",
            EventKind::Updated,
            br#"{"v":1}"#,
            "2026-08-01T00:00:00Z",
        )
        .expect("valid test fixture");
        let second = record(
            "w1",
            EventKind::Updated,
            br#"{"v":2}"#,
            "2026-08-02T00:00:00Z",
        )
        .expect("valid test fixture");

        landing
            .append(run, vec![first.clone()])
            .await
            .expect("store operation");
        assert_eq!(
            landing.latest_digest(&id).await.expect("store operation"),
            Some(first.digest())
        );

        landing
            .append(run, vec![second.clone()])
            .await
            .expect("store operation");
        assert_eq!(
            landing.latest_digest(&id).await.expect("store operation"),
            Some(second.digest())
        );

        // Back to the original body: it differs from what is current, so it is a
        // change even though it is not new.
        landing
            .append(run, vec![first.clone()])
            .await
            .expect("store operation");
        assert_eq!(
            landing.latest_digest(&id).await.expect("store operation"),
            Some(first.digest())
        );
    });
}

#[test]
fn an_unseen_record_has_no_digest() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let landing = HevyWorkoutLandingStore::new(pool).expect("a wired stream");
        let digest = landing
            .latest_digest(&SourceRecordId::try_from("never-seen").expect("valid test fixture"))
            .await
            .expect("store operation");
        assert_eq!(digest, None);
    });
}

/// FR-002: the bytes come back exactly as they went in, including the
/// unrecognised field, which nothing along the way is allowed to drop.
#[test]
fn a_payload_round_trips_byte_for_byte() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let run = a_run(&pool).await.expect("store operation");
        let landing = HevyWorkoutLandingStore::new(pool.clone()).expect("a wired stream");
        let body = br#"{"type":"updated","workout":{"id":"w1","surprise_field":[1,2,3]}}"#;

        landing
            .append(
                run,
                vec![
                    record("w1", EventKind::Updated, body, "2026-08-01T00:00:00Z")
                        .expect("valid test fixture"),
                ],
            )
            .await
            .expect("store operation");

        let stored: Vec<u8> =
            sqlx::query_scalar("SELECT payload FROM hevy_workout_landing WHERE id = 1")
                .fetch_one(&pool)
                .await
                .expect("store operation");
        assert_eq!(stored, body.to_vec());
    });
}

/// The serve ordinal continues across calls within a run, so the order the
/// source served events in survives a walk that commits per page.
#[test]
fn serve_ordinals_continue_across_pages_of_one_run() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let run = a_run(&pool).await.expect("store operation");
        let landing = HevyWorkoutLandingStore::new(pool.clone()).expect("a wired stream");

        landing
            .append(
                run,
                vec![
                    record(
                        "w1",
                        EventKind::Updated,
                        br#"{"v":1}"#,
                        "2026-08-01T00:00:00Z",
                    )
                    .expect("valid test fixture"),
                    record(
                        "w2",
                        EventKind::Updated,
                        br#"{"v":2}"#,
                        "2026-08-02T00:00:00Z",
                    )
                    .expect("valid test fixture"),
                ],
            )
            .await
            .expect("store operation");
        landing
            .append(
                run,
                vec![
                    record(
                        "w3",
                        EventKind::Updated,
                        br#"{"v":3}"#,
                        "2026-08-03T00:00:00Z",
                    )
                    .expect("valid test fixture"),
                ],
            )
            .await
            .expect("store operation");

        let ordinals: Vec<i64> =
            sqlx::query_scalar("SELECT serve_ordinal FROM hevy_workout_landing ORDER BY id")
                .fetch_all(&pool)
                .await
                .expect("store operation");
        assert_eq!(ordinals, vec![0, 1, 2]);
    });
}

/// A deletion is a record like any other, and an event without a timestamp is
/// recorded as having none rather than borrowing the fetch time.
#[test]
fn a_deletion_lands_and_a_missing_event_time_stays_missing() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let run = a_run(&pool).await.expect("store operation");
        let landing = HevyWorkoutLandingStore::new(pool.clone()).expect("a wired stream");

        let deletion = LandingRecord::land(
            hevy_workouts().expect("valid test fixture"),
            FetchedAt::try_from("2026-08-11T18:19:59Z").expect("valid test fixture"),
            SourceRecordId::try_from("gone").expect("valid test fixture"),
            EventProvenance::new(
                Endpoint::try_from("/v1/workouts/events").expect("valid test fixture"),
                EventKind::Deleted,
                None,
            )
            .into(),
            RawPayload::try_from(br#"{"type":"deleted","id":"gone"}"#.as_slice())
                .expect("valid test fixture"),
        );
        landing
            .append(run, vec![deletion])
            .await
            .expect("store operation");

        let (kind, event_time): (String, Option<String>) =
            sqlx::query_as("SELECT event_kind, event_time FROM hevy_workout_landing WHERE id = 1")
                .fetch_one(&pool)
                .await
                .expect("store operation");
        assert_eq!(kind, "deleted");
        assert_eq!(event_time, None);
    });
}

/// An unrecognised kind is stored verbatim: what the source called it, not
/// what we would have called it.
#[test]
fn an_unrecognised_event_kind_is_stored_verbatim() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let run = a_run(&pool).await.expect("store operation");
        let landing = HevyWorkoutLandingStore::new(pool.clone()).expect("a wired stream");

        landing
            .append(
                run,
                vec![
                    record(
                        "w1",
                        EventKind::try_from("archived").expect("valid test fixture"),
                        br#"{"type":"archived"}"#,
                        "2026-08-01T00:00:00Z",
                    )
                    .expect("valid test fixture"),
                ],
            )
            .await
            .expect("store operation");

        let kind: String =
            sqlx::query_scalar("SELECT event_kind FROM hevy_workout_landing WHERE id = 1")
                .fetch_one(&pool)
                .await
                .expect("store operation");
        assert_eq!(kind, "archived");
    });
}

#[test]
fn appending_nothing_lands_nothing() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let run = a_run(&pool).await.expect("store operation");
        let landing = HevyWorkoutLandingStore::new(pool).expect("a wired stream");
        assert_eq!(
            landing
                .append(run, Vec::new())
                .await
                .expect("store operation"),
            RecordCount::from(0)
        );
        assert_eq!(
            landing.count().await.expect("store operation"),
            RecordCount::from(0)
        );
    });
}

// --- The resumption point ---------------------------------------------------

#[test]
fn a_resumption_point_is_absent_until_a_run_sets_it() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let points = SqliteResumptionPointStore::new(pool);
        assert_eq!(
            points
                .read(&hevy_workouts().expect("valid test fixture"))
                .await
                .expect("store operation"),
            None
        );
    });
}

/// FR-007: resetting is deleting the row, and it costs a re-fetch rather than
/// a fact. Nothing in raw is touched.
#[test]
fn a_resumption_point_advances_and_clears() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let points = SqliteResumptionPointStore::new(pool);
        let stream = hevy_workouts().expect("valid test fixture");
        let at = FetchedAt::try_from("2026-08-11T18:19:59Z").expect("valid test fixture");

        let first = Watermark::try_from("2026-08-01T00:00:00Z").expect("valid test fixture");
        points
            .advance(&stream, first, at)
            .await
            .expect("store operation");
        assert_eq!(
            points.read(&stream).await.expect("store operation"),
            Some(first)
        );

        let later = Watermark::try_from("2026-08-10T19:29:47.199Z").expect("valid test fixture");
        points
            .advance(&stream, later, at)
            .await
            .expect("store operation");
        assert_eq!(
            points.read(&stream).await.expect("store operation"),
            Some(later)
        );

        points.clear(&stream).await.expect("store operation");
        assert_eq!(points.read(&stream).await.expect("store operation"), None);
    });
}

/// Sub-second precision must survive: the source serves it, and a watermark
/// rounded to the second would re-fetch or skip.
#[test]
fn a_resumption_point_keeps_sub_second_precision() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let points = SqliteResumptionPointStore::new(pool);
        let stream = hevy_workouts().expect("valid test fixture");
        let precise = Watermark::try_from("2026-08-10T19:29:47.199Z").expect("valid test fixture");

        points
            .advance(
                &stream,
                precise,
                FetchedAt::try_from("2026-08-11T18:19:59Z").expect("valid test fixture"),
            )
            .await
            .expect("store operation");
        assert_eq!(
            points.read(&stream).await.expect("store operation"),
            Some(precise)
        );
    });
}

/// Two streams of the same source resume independently. A watermark belongs to
/// one landing table, not to a source.
#[test]
fn streams_of_one_source_resume_independently() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let points = SqliteResumptionPointStore::new(pool);
        let workouts = hevy_workouts().expect("valid test fixture");
        let measurements =
            LandingStream::try_from("hevy.measurements").expect("valid test fixture");
        let at = FetchedAt::try_from("2026-08-11T18:19:59Z").expect("valid test fixture");

        points
            .advance(
                &workouts,
                Watermark::try_from("2026-08-01T00:00:00Z").expect("valid test fixture"),
                at,
            )
            .await
            .expect("store operation");

        assert!(
            points
                .read(&workouts)
                .await
                .expect("store operation")
                .is_some()
        );
        assert_eq!(
            points.read(&measurements).await.expect("store operation"),
            None
        );
    });
}

// --- The run log ------------------------------------------------------------

/// FR-008 and FR-011 together: a run that found nothing is a success with zero
/// counts, and it is distinguishable from never having run and from failing.
#[test]
fn a_successful_run_that_found_nothing_is_still_a_success() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let log = SqliteExtractionRunLog::new(pool);
        let stream = hevy_workouts().expect("valid test fixture");
        let at = FetchedAt::try_from("2026-08-11T18:19:59Z").expect("valid test fixture");

        assert_eq!(
            log.latest_success(&stream).await.expect("store operation"),
            None
        );

        let run = log.begin(&stream, at).await.expect("store operation");
        // In flight is not yet a success.
        assert_eq!(
            log.latest_success(&stream).await.expect("store operation"),
            None
        );

        log.finish(
            run,
            RunOutcome::Succeeded {
                finished_at: at,
                events_seen: domain::landing::EventCount::from(0),
                records_landed: RecordCount::from(0),
            },
        )
        .await
        .expect("store operation");

        let latest = log
            .latest_success(&stream)
            .await
            .expect("store operation")
            .expect("a success");
        assert_eq!(latest.id(), run);
        assert!(latest.outcome().is_success());
    });
}

/// A failed run is never reported as the latest success, which is what stops a
/// broken extraction reading as a quiet one.
#[test]
fn a_failed_run_is_not_a_success() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let log = SqliteExtractionRunLog::new(pool);
        let stream = hevy_workouts().expect("valid test fixture");
        let at = FetchedAt::try_from("2026-08-11T18:19:59Z").expect("valid test fixture");

        let run = log.begin(&stream, at).await.expect("store operation");
        log.finish(
            run,
            RunOutcome::Failed {
                finished_at: at,
                reason: domain::landing::FailureReason::SourceUnavailable,
            },
        )
        .await
        .expect("store operation");

        assert_eq!(
            log.latest_success(&stream).await.expect("store operation"),
            None
        );
    });
}

#[test]
fn the_latest_success_is_the_most_recent_one() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let log = SqliteExtractionRunLog::new(pool);
        let stream = hevy_workouts().expect("valid test fixture");

        for finished in [
            "2026-08-09T00:00:00Z",
            "2026-08-11T00:00:00Z",
            "2026-08-10T00:00:00Z",
        ] {
            let at = FetchedAt::try_from(finished).expect("valid test fixture");
            let run = log.begin(&stream, at).await.expect("store operation");
            log.finish(
                run,
                RunOutcome::Succeeded {
                    finished_at: at,
                    events_seen: domain::landing::EventCount::from(1),
                    records_landed: RecordCount::from(1),
                },
            )
            .await
            .expect("store operation");
        }

        let latest = log
            .latest_success(&stream)
            .await
            .expect("store operation")
            .expect("a success");
        assert_eq!(
            latest.outcome().finished_at(),
            Some(FetchedAt::try_from("2026-08-11T00:00:00Z").expect("valid test fixture"))
        );
    });
}

// --- The schema's own guarantees --------------------------------------------

/// The CHECK constraints mirror the `RunOutcome` sum type. The type makes these
/// combinations unrepresentable in Rust; these assert they are unrepresentable
/// in the file too, for a writer that is not this program.
#[test]
fn the_schema_refuses_a_finished_run_with_no_outcome() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let refused = sqlx::query(
            "INSERT INTO extraction_run (stream, started_at, finished_at)
             VALUES ('hevy.workouts', '2026-08-11T18:19:59Z', '2026-08-11T18:20:00Z')",
        )
        .execute(&pool)
        .await;
        assert!(refused.is_err(), "a finished run must say how it finished");
    });
}

#[test]
fn the_schema_refuses_a_failure_with_no_reason() {
    runtime()
        .expect("a tokio runtime")
        .block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let refused = sqlx::query(
            "INSERT INTO extraction_run
                (stream, started_at, finished_at, outcome, events_seen, records_landed)
             VALUES ('hevy.workouts', '2026-08-11T18:19:59Z', '2026-08-11T18:20:00Z', 'failed', 0, 0)",
        )
        .execute(&pool)
        .await;
        assert!(refused.is_err(), "a failure must say why");
        });
}

#[test]
fn the_schema_refuses_a_success_that_reports_no_counts() {
    runtime().expect("a tokio runtime").block_on(async {
        let (_dir, pool) = store().await.expect("store operation");
        let refused = sqlx::query(
            "INSERT INTO extraction_run (stream, started_at, finished_at, outcome)
             VALUES ('hevy.workouts', '2026-08-11T18:19:59Z', '2026-08-11T18:20:00Z', 'succeeded')",
        )
        .execute(&pool)
        .await;
        assert!(
            refused.is_err(),
            "a finished run always reports both counts"
        );
    });
}
