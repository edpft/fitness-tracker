//! A write of what a writer authors moves its revision, and nothing else does
//! (#805).

use std::collections::BTreeSet;

use application::{Authors, Node, Revisions};
use infrastructure::{Authoring, SqliteRevisions, connect};

struct Planner;
impl Authors for Planner {
    fn authors() -> BTreeSet<Node> {
        [Node::PLAN].into()
    }
}

/// **A committed write moves the revision of what its writer authors; a read,
/// and a write not committed, keep nothing**, so a page drawn from the plan
/// sees every edit to it and no other.
#[test]
fn a_write_moves_the_revision_and_a_read_keeps_nothing() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(async {
            let directory = tempfile::tempdir().expect("a directory");
            let pool = connect(&directory.path().join("test.db"))
                .await
                .expect("a store");
            let authoring = Authoring::<Planner>::new(pool.clone());
            let revisions = SqliteRevisions::new(pool);
            let at = async |node: &Node| revisions.revision(node).await.expect("read").value();
            assert_eq!(at(&Node::PLAN).await, 0, "never written");

            let mut reading = authoring.reading().await.expect("a read");
            sqlx::query("INSERT INTO node_revision (node, revision) VALUES ('plan', 7)")
                .execute(&mut *reading)
                .await
                .expect("written inside the read");
            drop(reading);
            assert_eq!(at(&Node::PLAN).await, 0, "a read keeps nothing");

            for _ in 0..2 {
                authoring
                    .writing()
                    .await
                    .expect("a write")
                    .commit()
                    .await
                    .expect("committed");
            }
            assert_eq!(at(&Node::PLAN).await, 2);
            assert_eq!(at(&Node::DIARY).await, 0, "the diary was not written");

            drop(authoring.writing().await.expect("a write"));
            assert_eq!(
                at(&Node::PLAN).await,
                2,
                "a write not committed keeps nothing"
            );
        });
}
