//! What a published programme's ride can be swapped for, from the catalogue
//! (#436).
//!
//! **A class of the same length, in no programme, and read.** A programme's own
//! classes are kept for the next cycle's first round; a class of another length
//! would change the week's volume; and a class only listed has no zone plan to
//! be compared by.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use std::error::Error;

use application::StandaloneClasses as _;
use domain::cycling::RideVenue;
use infrastructure::{
    SqlitePelotonClassStore, connect,
    peloton::{ClassSummary, PelotonHoldingRides, catalogue::ClassCatalogue},
};
use tempfile::TempDir;

type Fallible<T> = Result<T, Box<dyn Error>>;

/// Build's µ1 session 1, and its µ1 session 2: both placed by a programme.
const BUILD_FIRST: &str = "9f8f3af689cc4f0db9afa013d4676ed6";
const BUILD_SECOND: &str = "44867a5486184a09b8ca135a6d8c7494";
const ENDURANCE: &str = "0f63c48726fa4533a928cae5358d94d7";

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

fn listed(id: &str, duration: u64) -> ClassSummary {
    ClassSummary {
        id: id.to_owned(),
        title: format!("{} min Power Zone Endurance Ride", duration / 60),
        duration_seconds: duration,
        series: Some(ENDURANCE.to_owned()),
        instructor: Some("an-instructor".to_owned()),
        aired_at: Some(100),
    }
}

/// A class detail the reader can derive: a warm-up, a zoned ride, a cool-down.
fn detail(title: &str, zone: u8) -> String {
    serde_json::json!({
        "ride": { "title": title, "instructor": { "id": "an-instructor", "name": "A Teacher" } },
        "segments": { "segment_list": [
            { "name": "Warm Up", "length": 300 },
            { "name": "Power Zone", "length": 600 },
            { "name": "Cool Down", "length": 120 },
        ]},
        "target_metrics_data": { "target_metrics": [
            { "offsets": { "start": 300, "end": 899 }, "metrics": [{ "lower": zone }] },
        ]},
        "is_ftp_test": false,
    })
    .to_string()
}

#[test]
fn only_a_read_standalone_class_of_the_same_length_is_offered() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        let classes = [
            (BUILD_FIRST, 2_700, true),
            (BUILD_SECOND, 2_700, true),
            ("standalone-45", 2_700, true),
            ("standalone-60", 3_600, true),
            ("listed-only-45", 2_700, false),
        ];
        let summaries: Vec<ClassSummary> = classes
            .iter()
            .map(|(id, duration, _)| listed(id, *duration))
            .collect();
        store
            .record_listed(&summaries)
            .await
            .expect("the listing records");
        for (id, duration, read) in classes {
            if read {
                let title = format!("{} min Power Zone Endurance Ride", duration / 60);
                store
                    .record_detail(id, &title, 1_020, &detail(&title, 3))
                    .await
                    .expect("the detail records");
            }
        }

        let offering = PelotonHoldingRides::new(ClassCatalogue::stored(&store));
        let venue =
            RideVenue::new(BUILD_FIRST, "45 min Power Zone Endurance Ride").expect("a venue");
        let offered = offering.like(&venue).await.expect("the catalogue answers");

        let references: Vec<&str> = offered
            .iter()
            .map(|standalone| standalone.at.reference())
            .collect();
        assert_eq!(
            references,
            vec!["standalone-45"],
            "Build's other class is a programme's, the 60-minute one is another \
             length, and the listed one has no zone plan"
        );
    });
}

#[test]
fn a_class_the_catalogue_does_not_hold_is_offered_nothing() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let (_directory, store) = catalogue().await.expect("a catalogue opens");
        let offering = PelotonHoldingRides::new(ClassCatalogue::stored(&store));
        let venue = RideVenue::new("not-held", "a class").expect("a venue");

        let offered = offering.like(&venue).await.expect("the catalogue answers");

        assert!(
            offered.is_empty(),
            "no length to match, so the ride stays as published"
        );
    });
}
