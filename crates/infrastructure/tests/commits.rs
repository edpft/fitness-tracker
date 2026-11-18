//! What this process commits to the store is counted, and nothing else is
//! (#830).
#![expect(
    clippy::disallowed_methods,
    reason = "the count is the pool's, so the test opens one"
)]

use infrastructure::connect_counting;

/// **A committed write is counted at once, and settled once its connection
/// is back in the pool; a read, a rolled-back write and a drawing kept are
/// not counted**, so what a page reads is kept until something it could read
/// moves.
#[test]
fn a_write_is_counted_and_a_read_or_a_drawing_is_not() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(async {
            let directory = tempfile::tempdir().expect("a directory");
            let (pool, commits) = connect_counting(&directory.path().join("test.db"))
                .await
                .expect("a store");
            let settle = async || {
                for _ in 0..100 {
                    if commits.settled().is_some() {
                        return;
                    }
                    tokio::task::yield_now().await;
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                }
            };
            settle().await;
            let before = commits.committed();
            assert_eq!(commits.settled(), Some(before), "opened and settled");

            sqlx::query("SELECT count(*) FROM node_revision")
                .execute(&pool)
                .await
                .expect("read");
            settle().await;
            assert_eq!(commits.committed(), before, "a read is not counted");

            sqlx::query(
                "INSERT INTO kept_drawing (address, drawing, drawn_from) VALUES ('/body', 'svg', NULL)",
            )
            .execute(&pool)
            .await
            .expect("a drawing kept");
            settle().await;
            assert_eq!(commits.committed(), before, "a drawing is not counted");

            let mut rolled_back = pool.begin().await.expect("a transaction");
            sqlx::query("INSERT INTO node_revision (node, revision) VALUES ('plan', 1)")
                .execute(&mut *rolled_back)
                .await
                .expect("written");
            rolled_back.rollback().await.expect("rolled back");
            settle().await;
            assert_eq!(
                commits.committed(),
                before,
                "a write rolled back is not counted"
            );

            sqlx::query("INSERT INTO node_revision (node, revision) VALUES ('plan', 1)")
                .execute(&pool)
                .await
                .expect("written");
            assert_eq!(
                commits.committed(),
                before + 1,
                "a write is counted as soon as it is committed"
            );
            settle().await;
            assert_eq!(
                commits.settled(),
                Some(before + 1),
                "and settled once its connection is released"
            );

            let mut both = pool.begin().await.expect("a transaction");
            sqlx::query(
                "INSERT INTO kept_drawing (address, drawing, drawn_from) VALUES ('/sleep', 'svg', NULL)",
            )
            .execute(&mut *both)
            .await
            .expect("a drawing kept");
            sqlx::query(
                "INSERT INTO extraction_run (stream, started_at) VALUES ('hevy.workouts', '2026-11-18T06:00:00Z')",
            )
            .execute(&mut *both)
            .await
            .expect("written");
            both.commit().await.expect("committed");
            settle().await;
            assert_eq!(
                commits.settled(),
                Some(before + 2),
                "a drawing written with anything else is counted"
            );
        });
}

/// **A commit inside [`moving`] moves what it names, and nothing else**; one
/// outside moves everything.
#[test]
fn a_write_moves_what_its_step_names() {
    use application::Node;
    use domain::landing::LandingStream;
    use infrastructure::moving;

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(async {
            let directory = tempfile::tempdir().expect("a directory");
            let (pool, commits) = connect_counting(&directory.path().join("test.db"))
                .await
                .expect("a store");
            let landing = |name: &str| {
                Node::Landing(LandingStream::try_from(name).expect("a stream"))
            };
            let withings = [landing("withings.measurements")].into();
            let sleep = [landing("garmin.sleep")].into();
            let at = commits.committed();

            moving(
                [landing("withings.measurements")].into(),
                sqlx::query(
                    "INSERT INTO extraction_run (stream, started_at) VALUES ('withings.measurements', '2026-11-18T06:00:00Z')",
                )
                .execute(&pool),
            )
            .await
            .expect("written");
            assert!(commits.moved_since(at, &withings), "what it names moves");
            assert!(!commits.moved_since(at, &sleep), "and nothing else does");

            let at = commits.committed();
            sqlx::query(
                "INSERT INTO extraction_run (stream, started_at) VALUES ('garmin.sleep', '2026-11-18T06:00:00Z')",
            )
            .execute(&pool)
            .await
            .expect("written");
            assert!(
                commits.moved_since(at, &withings) && commits.moved_since(at, &sleep),
                "a write naming nothing moves everything"
            );
        });
}

/// **No connection is too old to settle what it committed.** sqlx closes a
/// connection past its lifetime as it is released, without `after_release`,
/// so a commit on one would never settle, and nothing read after it would be
/// kept for as long as the web service ran.
#[test]
fn a_connection_is_never_too_old_to_settle() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(async {
            let directory = tempfile::tempdir().expect("a directory");
            let (pool, _) = connect_counting(&directory.path().join("test.db"))
                .await
                .expect("a store");
            assert_eq!(pool.options().get_max_lifetime(), None);
        });
}
