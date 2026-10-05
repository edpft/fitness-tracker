//! The local class catalogue, against a real SQLite file (#246).
//!
//! **What is pinned is the two states and the order they are reached in.** A
//! class is *listed* by the browse walk and *read* when its detail arrives, and
//! the gap between the two is the whole reason the catalogue converges rather
//! than downloading the library inside the operator's daily loop.
//!
//! Three things here would each look like a working catalogue and are not:
//!
//! - A re-list that discarded the detail. Every refresh lists every class, so
//!   this would undo all the reading the refreshes before it paid for and the
//!   catalogue would never converge.
//! - Unread classes handed back oldest first. The classes that get ridden are
//!   the newest; a library read from the far end would spend its first runs on
//!   classes a decade old.
//! - A class read but still offered as unread, which would make every refresh
//!   fetch the same fifty details forever.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use std::error::Error;

use infrastructure::{SqlitePelotonClassStore, connect, peloton::ClassSummary};
use tempfile::TempDir;

type Fallible<T> = Result<T, Box<dyn Error>>;

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

async fn catalogue() -> Fallible<(TempDir, SqlitePelotonClassStore)> {
    let directory = TempDir::new()?;
    let pool = connect(&directory.path().join("fitness.db")).await?;
    Ok((directory, SqlitePelotonClassStore::new(pool)))
}

fn listed(id: &str, series: &str, duration: u64, aired_at: i64) -> ClassSummary {
    ClassSummary {
        id: id.to_owned(),
        title: format!("{} min Power Zone Ride", duration / 60),
        duration_seconds: duration,
        series: Some(series.to_owned()),
        instructor: Some("an-instructor".to_owned()),
        aired_at: Some(aired_at),
    }
}

/// A class detail the reader can derive: a warm-up, a zoned ride, a cool-down.
fn detail(title: &str) -> String {
    serde_json::json!({
        "ride": { "title": title, "instructor": { "id": "an-instructor", "name": "A Teacher" } },
        "segments": { "segment_list": [
            { "name": "Warm Up", "length": 300 },
            { "name": "Power Zone", "length": 600 },
            { "name": "Cool Down", "length": 120 },
        ]},
        "target_metrics_data": { "target_metrics": [
            { "offsets": { "start": 300, "end": 899 }, "metrics": [{ "lower": 3 }] },
        ]},
        "is_ftp_test": false,
    })
    .to_string()
}

const ENDURANCE: &str = "0f63c48726fa4533a928cae5358d94d7";
const PLAIN: &str = "9fde039566054ea499130bed1c289eb3";

/// The first refresh: everything listed is new, and nothing is read yet.
#[test]
fn a_listed_class_is_held_and_not_yet_read() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        let added = store
            .record_listed(&[
                listed("a", ENDURANCE, 2_700, 100),
                listed("b", PLAIN, 2_700, 200),
            ])
            .await
            .expect("the listing records");
        assert_eq!(added, 2, "both were new");

        let held = store.held().await.expect("the counts read");
        assert_eq!(held.listed, 2);
        assert_eq!(held.read, 0);
        assert_eq!(held.outstanding(), 2);

        assert!(
            store.class("a").await.expect("a read answers").is_none(),
            "a listed class says nothing about what it prescribes"
        );
    });
}

/// **The one that would make the catalogue never converge.** Every refresh
/// lists every class, so a re-list must leave the detail alone.
#[test]
fn re_listing_a_class_keeps_what_was_read_of_it() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[listed("a", ENDURANCE, 2_700, 100)])
            .await
            .expect("the listing records");
        store
            .record_detail(
                "a",
                "45 min Power Zone Endurance Ride",
                1_020,
                &detail("45 min Power Zone Endurance Ride"),
            )
            .await
            .expect("the detail records");

        let added = store
            .record_listed(&[listed("a", ENDURANCE, 2_700, 100)])
            .await
            .expect("the listing records again");
        assert_eq!(added, 0, "a class Peloton re-lists is not a new class");

        let held = store.held().await.expect("the counts read");
        assert_eq!(held.listed, 1);
        assert_eq!(held.read, 1, "the second listing did not undo the reading");
        assert_eq!(held.outstanding(), 0);
        assert!(
            store.unread().await.expect("unread reads").is_empty(),
            "a read class is never offered as unread again"
        );
    });
}

/// A re-list replaces the listing fields, because § 14 holds the current value.
#[test]
fn re_listing_a_class_replaces_what_the_listing_says() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[listed("a", ENDURANCE, 2_700, 100)])
            .await
            .expect("the listing records");
        store
            .record_listed(&[listed("a", PLAIN, 1_800, 300)])
            .await
            .expect("the listing records again");

        let under_old = store
            .in_series(ENDURANCE, 2_700)
            .await
            .expect("a series reads");
        assert!(under_old.is_empty(), "the old listing no longer answers");
        let under_new = store.in_series(PLAIN, 1_800).await.expect("a series reads");
        assert_eq!(under_new.len(), 1);
    });
}

/// **Newest first, because that is what gets ridden.** Unbounded since #369,
/// so the order no longer decides what gets left out — it decides what a
/// refresh that is interrupted part-way has already read.
#[test]
fn the_unread_are_offered_newest_first() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[
                listed("oldest", ENDURANCE, 2_700, 100),
                listed("newest", ENDURANCE, 2_700, 300),
                listed("middle", ENDURANCE, 2_700, 200),
            ])
            .await
            .expect("the listing records");

        let first = store.unread().await.expect("unread reads");
        assert_eq!(
            first,
            vec![
                "newest".to_owned(),
                "middle".to_owned(),
                "oldest".to_owned()
            ],
            "newest first, and every one of them"
        );
    });
}

/// A class with no air date sorts last rather than first, so it never crowds
/// out a class the operator might actually ride.
#[test]
fn a_class_with_no_air_date_is_read_last() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        let mut undated = listed("undated", ENDURANCE, 2_700, 0);
        undated.aired_at = None;
        store
            .record_listed(&[undated, listed("dated", ENDURANCE, 2_700, 100)])
            .await
            .expect("the listing records");

        let order = store.unread().await.expect("unread reads");
        assert_eq!(order, vec!["dated".to_owned(), "undated".to_owned()]);
    });
}

/// What was stored is what the reader derives, which is the point of keeping
/// the response rather than only the reading of it.
#[test]
fn a_read_class_prescribes_what_its_stored_detail_says() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[listed("a", ENDURANCE, 2_700, 100)])
            .await
            .expect("the listing records");
        store
            .record_detail(
                "a",
                "45 min Power Zone Endurance Ride",
                1_020,
                &detail("45 min Power Zone Endurance Ride"),
            )
            .await
            .expect("the detail records");

        let class = store
            .class("a")
            .await
            .expect("a read answers")
            .expect("a read class is there");
        assert_eq!(class.title, "45 min Power Zone Endurance Ride");
        assert_eq!(class.warm_up_seconds, 300);
        assert_eq!(class.cool_down_seconds, 120);
        assert_eq!(class.ride_seconds, 600);
        assert!(
            class.tiles(),
            "the zone plan accounts for the whole ride, clipped to it"
        );
    });
}

/// **A class the browse listing never carried is held anyway**, which is the
/// Peak case: `mapping` records one class of *Peak Your Power Zones* as
/// unavailable to the operator's account, and a catalogue that refused it would
/// leave `plan` with a hole in Peak.
#[test]
fn a_class_the_listing_never_carried_is_held_from_its_detail_alone() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_detail(
                "never-listed",
                "45 min Power Zone Ride",
                1_020,
                &detail("45 min Power Zone Ride"),
            )
            .await
            .expect("a class with no listing is still a class Peloton serves");

        let held = store.held().await.expect("the counts read");
        assert_eq!(held.listed, 1);
        assert_eq!(held.read, 1);
        assert!(
            store
                .is_accounted_for("never-listed")
                .await
                .expect("is_accounted_for answers")
        );

        let class = store
            .class("never-listed")
            .await
            .expect("a read answers")
            .expect("it is there");
        assert_eq!(class.title, "45 min Power Zone Ride");
        assert!(
            store
                .in_series(ENDURANCE, 2_700)
                .await
                .expect("a series reads")
                .is_empty(),
            "a detail states no series, so it is offered for no series"
        );
    });
}

/// **A detail must not overwrite the series a walk recorded.** The detail
/// carries no `series_id`, so a write that replaced the listing fields would
/// wipe the series every candidate search turns on.
#[test]
fn reading_a_class_keeps_the_series_the_listing_gave_it() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[listed("a", ENDURANCE, 2_700, 100)])
            .await
            .expect("the listing records");
        store
            .record_detail("a", "a different title", 1_020, &detail("a class"))
            .await
            .expect("the detail records");

        let found = store
            .in_series(ENDURANCE, 2_700)
            .await
            .expect("a series reads");
        assert_eq!(found.len(), 1, "the series survived being read");
        assert_eq!(
            found[0].duration_seconds, 2_700,
            "and so did the listing's own length"
        );
        assert_eq!(
            found[0].title, "45 min Power Zone Ride",
            "and its title: the listing's is the current value, not the detail's"
        );
    });
}

/// The selection a holding week makes, answered from the store in the order the
/// source would have given it.
#[test]
fn a_series_and_a_length_answer_newest_first_from_the_store() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[
                listed("older", ENDURANCE, 2_700, 100),
                listed("newer", ENDURANCE, 2_700, 200),
                listed("wrong-length", ENDURANCE, 1_800, 300),
                listed("wrong-series", PLAIN, 2_700, 400),
            ])
            .await
            .expect("the listing records");

        let found = store
            .in_series(ENDURANCE, 2_700)
            .await
            .expect("a series reads");
        let ids: Vec<&str> = found.iter().map(|one| one.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["newer", "older"],
            "one series, one length, newest first"
        );
    });
}

/// An empty catalogue answers empty rather than erroring, and the counts are
/// what tell an unfilled catalogue from a series Peloton has nothing in.
#[test]
fn an_unfilled_catalogue_answers_nothing_and_says_so() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        assert!(
            store
                .in_series(ENDURANCE, 2_700)
                .await
                .expect("a series reads")
                .is_empty()
        );
        let held = store.held().await.expect("the counts read");
        assert_eq!(held.listed, 0);
        assert_eq!(held.outstanding(), 0);
    });
}

/// **The thing that makes "all read" reachable** (#369). Peloton will not
/// serve one *Peak Your Power Zones* class to the operator's account, and an
/// unbounded refresh that left it unread would ask for it again on every single
/// `fitness next` — and the count would read "1 still to read" for ever.
#[test]
fn a_class_peloton_will_not_serve_is_accounted_for_and_never_asked_again() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[
                listed("served", ENDURANCE, 2_700, 200),
                listed("unserved", ENDURANCE, 2_700, 100),
            ])
            .await
            .expect("the listing records");
        store
            .record_not_served("unserved")
            .await
            .expect("what the source does not serve records");

        assert_eq!(
            store.unread().await.expect("unread reads"),
            vec!["served".to_owned()],
            "a class the source does not serve is not offered for reading again"
        );
        assert!(
            store
                .is_accounted_for("unserved")
                .await
                .expect("is_accounted_for answers"),
            "there is nothing left to ask Peloton about it"
        );

        store
            .record_detail(
                "served",
                "45 min Power Zone Ride",
                1_020,
                &detail("45 min Power Zone Ride"),
            )
            .await
            .expect("a detail records");

        let held = store.held().await.expect("the counts read");
        assert_eq!(held.listed, 2);
        assert_eq!(held.read, 1);
        assert_eq!(held.not_served, 1);
        assert_eq!(
            held.outstanding(),
            0,
            "a catalogue that has asked all it can is read"
        );
    });
}

/// A class the listing never carried and Peloton has no detail for: the fact
/// needs somewhere to live, or the class is asked for on every run for ever.
#[test]
fn a_class_with_no_listing_row_still_records_that_it_is_not_served() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_not_served("never-listed")
            .await
            .expect("what the source does not serve records");

        assert!(
            store
                .is_accounted_for("never-listed")
                .await
                .expect("is_accounted_for answers")
        );
        assert!(store.unread().await.expect("unread reads").is_empty());
        assert!(
            store
                .in_series(ENDURANCE, 2_700)
                .await
                .expect("a series reads")
                .is_empty(),
            "a placeholder row is never a candidate"
        );
        assert!(
            store
                .class("never-listed")
                .await
                .expect("a read answers")
                .is_none(),
            "and it prescribes nothing"
        );
    });
}

/// **Not served is not the last word where Peloton later serves the class.**
/// Nothing re-asks on a schedule, but a detail that does arrive — because the
/// class was placed by a skeleton and fetched by name — clears it.
#[test]
fn a_detail_clears_what_was_not_served() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        store
            .record_listed(&[listed("a", ENDURANCE, 2_700, 100)])
            .await
            .expect("the listing records");
        store
            .record_not_served("a")
            .await
            .expect("what the source does not serve records");
        store
            .record_detail(
                "a",
                "45 min Power Zone Ride",
                1_020,
                &detail("45 min Power Zone Ride"),
            )
            .await
            .expect("a detail records");

        let held = store.held().await.expect("the counts read");
        assert_eq!(held.read, 1);
        assert_eq!(held.not_served, 0);
    });
}
