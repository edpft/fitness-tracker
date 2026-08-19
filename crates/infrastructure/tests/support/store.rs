//! A store holding the corpus, derived, with the fixture programme authored.
//!
//! One assembly rather than one per suite: landing, deriving and authoring is
//! forty lines of setup that says nothing about what any single test asserts, and
//! a second copy is a second thing to keep in step with the ports.

use application::{
    ExtractionRunLog as _, LandingStore as _, NormalisationSummary, ProgrammeAuthor as _,
    WorkoutNormaliser,
    normalise::{Normalisation, NormalisationPorts},
    prescribe::Authoring,
};
use infrastructure::{
    HevyWorkoutLandingReader, HevyWorkoutLandingStore, HevyWorkoutTranslator,
    SqliteExtractionRunLog, SqliteGenerationParameterStore, SqliteGymWorkoutStore,
    SqliteNormalisationRunLog, SqliteProgrammeStore, SqliteRefusalStore, connect,
};
use sqlx::SqlitePool;

use super::{corpus, programme};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// The corpus landed and derived, and the fixture programme in force.
///
/// The directory comes back with the pool because dropping it deletes the file
/// the pool is talking to.
pub async fn derived_and_authored() -> Fallible<(tempfile::TempDir, SqlitePool)> {
    with_programme(programme::programme()?).await
}

/// The same, with a programme the caller chose.
pub async fn with_programme(
    programme: domain::prescription::Programme,
) -> Fallible<(tempfile::TempDir, SqlitePool)> {
    let directory = tempfile::tempdir()?;
    let pool: SqlitePool = connect(&directory.path().join("test.db")).await?;

    let landing = HevyWorkoutLandingStore::new(pool.clone())?;
    let runs = SqliteExtractionRunLog::new(pool.clone());
    let run = runs
        .begin(landing.stream(), domain::landing::FetchedAt::EPOCH)
        .await?;
    let records = corpus::records()?
        .into_iter()
        .map(|landed| landed.record().clone())
        .collect();
    landing.append(run, records).await?;

    let normalisation = Normalisation::new(
        NormalisationPorts {
            raw: HevyWorkoutLandingReader::new(pool.clone())?,
            translator: HevyWorkoutTranslator,
            workouts: SqliteGymWorkoutStore::new(pool.clone())?,
            refusals: SqliteRefusalStore::new(pool.clone())?,
            runs: SqliteNormalisationRunLog::new(pool.clone()),
            clock: corpus::FixedClock,
        },
        corpus::zone()?,
    );
    let _summary: NormalisationSummary = normalisation.normalise().await?;

    Authoring::new(
        SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(&programme, &programme::parameters()?)
    .await?;

    Ok((directory, pool))
}
