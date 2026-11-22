//! What the store answered is kept until this process next writes it (#830).
//!
//! The operator, 2026-11-18: *"the fix is that the CLI and web server use the
//! same backend, until they do it's okay for the web server to be unaware of
//! changes made by the CLI."*
#![expect(
    clippy::disallowed_methods,
    reason = "a second pool stands for another process writing the store"
)]

use configurator::{catalogue, wiring};
use domain::normalised::OperatorZone;
use infrastructure::{SqliteOperatorSettingsStore, connect};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// What Hevy's layer is derived from: as first read, after another process
/// states a zone, after this one does, and from a store opened after.
async fn around_two_writers(database: &std::path::Path) -> Fallible<[application::DerivedFrom; 4]> {
    let zone = OperatorZone::try_from("Europe/London".to_owned())?;
    let hevy = catalogue::lookup("hevy.workouts").ok_or("hevy is catalogued")?;
    let store = configurator::store::Store::open(database).await?;

    // Opening commits the migrations, and a read is only kept once every
    // commit has settled, which happens as each connection goes back to the
    // pool, on a task of its own.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let first = wiring::inputs(hevy, &store, &zone).await?;

    SqliteOperatorSettingsStore::new(connect(database).await?)
        .author("2026-09-15T13:29:25Z".parse()?, "Pacific/Auckland")
        .await?;
    let elsewhere = wiring::inputs(hevy, &store, &zone).await?;

    store
        .state_timezone("2026-10-18T09:00:00Z".parse()?, "Europe/London")
        .await?;
    let here = wiring::inputs(hevy, &store, &zone).await?;

    let reopened = wiring::inputs(
        hevy,
        &configurator::store::Store::open(database).await?,
        &zone,
    )
    .await?;
    Ok([first, elsewhere, here, reopened])
}

/// **A write by another process is not seen until this one writes**, and
/// then everything it wrote is.
#[test]
fn a_read_is_kept_until_this_process_writes() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let [first, elsewhere, here, reopened] = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(around_two_writers(&directory.path().join("test.db")))
        .expect("the store answers");

    assert_eq!(first, elsewhere, "another process's write is not seen");
    assert_ne!(elsewhere, here, "this process's write is");
    assert_eq!(here, reopened, "and reads as a store opened afresh does");
}

/// What Withings' layer is derived from, and the diary: first, after another
/// process states a zone and this one writes the diary, and from a store
/// opened after.
async fn around_a_diary_write(
    database: &std::path::Path,
) -> Fallible<([application::DerivedFrom; 3], [domain::schedule::Diary; 2])> {
    use application::{DiaryAuthor as _, DiaryStore as _};
    let zone = OperatorZone::try_from("Europe/London".to_owned())?;
    let withings = catalogue::lookup("withings.measurements").ok_or("withings is catalogued")?;
    let store = configurator::store::Store::open(database).await?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let withings_first = wiring::inputs(withings, &store, &zone).await?;
    let diary_first = store.diary().diary().await?;

    SqliteOperatorSettingsStore::new(connect(database).await?)
        .author("2026-09-15T13:29:25Z".parse()?, "Pacific/Auckland")
        .await?;
    store
        .diary()
        .record_pattern(&domain::schedule::TrainingPattern::new(
            jiff::civil::Date::constant(2026, 1, 1),
            zone.clone(),
            std::collections::BTreeMap::from([(
                domain::schedule::TrainingSlot::new(
                    jiff::civil::Weekday::Monday,
                    domain::schedule::PartOfDay::Evening,
                ),
                domain::schedule::SlotLength::Shorter,
            )]),
        ))
        .await?;
    let withings_after = wiring::inputs(withings, &store, &zone).await?;
    let diary_after = store.diary().diary().await?;

    let reopened = configurator::store::Store::open(database).await?;
    Ok((
        [
            withings_first,
            withings_after,
            wiring::inputs(withings, &reopened, &zone).await?,
        ],
        [diary_first, diary_after],
    ))
}

/// **A write from a page moves what it authors, and nothing else**: a week
/// written on the schedule page is read again, and what Withings' layer reads
/// is still held.
#[test]
fn a_page_write_moves_only_what_it_authors() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let ([withings_first, withings_after, withings_reopened], [diary_first, diary_after]) =
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(around_a_diary_write(&directory.path().join("test.db")))
            .expect("the store answers");

    assert_ne!(diary_first, diary_after, "the week written is read");
    assert_eq!(withings_first, withings_after, "Withings is still held");
    assert_ne!(
        withings_after, withings_reopened,
        "though the zone another process stated would move it"
    );
}
