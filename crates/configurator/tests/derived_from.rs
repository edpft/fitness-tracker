//! What a normalised layer is derived from, as the composition root states it
//! (#468).
#![expect(
    clippy::disallowed_methods,
    reason = "what a derivation reads from is authored straight into the store it opens"
)]

use configurator::{catalogue, wiring};
use domain::normalised::OperatorZone;
use infrastructure::{SqliteOperatorSettingsStore, connect};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// The inputs of Hevy's layer declared from Auckland, before any zone is
/// stated, once London is, and once Auckland is stated too.
async fn around_a_move(database: &std::path::Path) -> Fallible<[application::DerivedFrom; 3]> {
    let auckland = OperatorZone::try_from("Pacific/Auckland".to_owned())?;
    let hevy = catalogue::lookup("hevy.workouts").ok_or("hevy is catalogued")?;
    let inputs = async || -> Fallible<application::DerivedFrom> {
        let store = configurator::store::Store::open(database).await?;
        Ok(wiring::inputs(hevy, &store, &auckland).await?)
    };

    let nothing_stated = inputs().await?;
    let settings = SqliteOperatorSettingsStore::new(connect(database).await?);
    settings
        .author("2026-09-15T13:29:25Z".parse()?, "Europe/London")
        .await?;
    let london = inputs().await?;
    settings
        .author("2027-03-01T09:00:00Z".parse()?, "Pacific/Auckland")
        .await?;
    let moved = inputs().await?;
    Ok([nothing_stated, london, moved])
}

/// #66: a zone declared for one invocation carries no date, so once any zone
/// is stated the derivation reads the stated ones, and a move is one more.
#[test]
fn a_derivation_reads_the_zones_stated_over_the_one_declared() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = directory.path().join("test.db");
    let [nothing_stated, london, moved] = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(around_a_move(&database))
        .expect("the store answers");

    assert!(nothing_stated.as_str().ends_with("zone Pacific/Auckland"));
    assert!(london.as_str().ends_with("zone Europe/London"));
    assert!(
        moved
            .as_str()
            .ends_with("zone Europe/London, Pacific/Auckland from 2027-03-01T09:00:00Z")
    );
}
