//! Which of Hevy's entries a change of exercise reaches (#754).
//!
//! - an entry reads as what the derivation reads, so one already corrected
//!   away from an exercise is not found as it, and is found as what it was
//!   corrected to;
//! - a search can be held to the day of the session a change is made from.

use crate::support;

use application::{
    ExtractionRunLog as _, LandingStore as _, WorkoutNormaliser as _,
    normalise::{Normalisation, NormalisationPorts},
};
use domain::{
    gym::exercise::{Exercise, RepsExercise},
    landing::FetchedAt,
    normalised::{
        Corrected, CorrectedTerm, Correction, CorrectionId, CorrectionReason, EditOverlay,
        OperatorZone, StatedZones,
    },
    sequence::NonEmpty,
};
use infrastructure::{
    HevySessionAccountReader, HevySessionTranslator, HevyStandIns, HevyWorkoutLandingStore,
    SqliteExtractionRunLog, SqliteGymSessionStore, SqliteNormalisationRunLog, SqliteRefusalStore,
    connect, hevy::Within,
};
use sqlx::SqlitePool;
use support::corpus;

type Failure = Box<dyn std::error::Error>;

const PULL_UP: Exercise = Exercise::Reps(RepsExercise::PullUp);
const NEUTRAL: Exercise = Exercise::Reps(RepsExercise::NeutralGripPullUp);

fn zones() -> Result<StatedZones, Failure> {
    Ok(StatedZones::throughout(OperatorZone::try_from(
        "Europe/London".to_owned(),
    )?))
}

/// The store, with the Hevy corpus landed and normalised.
#[expect(
    clippy::future_not_send,
    reason = "driven on a current-thread runtime inside one test"
)]
async fn derived() -> Result<(SqlitePool, tempfile::TempDir), Failure> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;
    let runs = SqliteExtractionRunLog::new(pool.clone());
    let hevy = HevyWorkoutLandingStore::new(pool.clone())?;
    let run = runs.begin(hevy.stream(), FetchedAt::EPOCH).await?;
    hevy.append(
        run,
        corpus::records()?
            .into_iter()
            .map(|landed| landed.record().clone())
            .collect(),
    )
    .await?;
    Normalisation::new(
        NormalisationPorts {
            raw: HevySessionAccountReader::new(pool.clone())?,
            translator: HevySessionTranslator::default(),
            workouts: SqliteGymSessionStore::new(pool.clone())?,
            refusals: SqliteRefusalStore::new(pool.clone(), HevyWorkoutLandingStore::STREAM)?,
            runs: SqliteNormalisationRunLog::new(pool.clone()),
            clock: corpus::FixedClock,
        },
        zones()?,
    )
    .normalise(application::Rederivation::Whole)
    .await?;
    Ok((pool, directory))
}

fn corrected(to: Exercise, entry: &CorrectedTerm) -> Result<EditOverlay, Failure> {
    Ok(EditOverlay::of(&[Correction::new(
        CorrectionId::from(1),
        "2026-11-20T09:00:00Z".parse()?,
        CorrectionReason::try_from("Hevy had no neutral grip pull up")?,
        Corrected::Exercise {
            exercise: to,
            entries: NonEmpty::new(vec![entry.clone()])?,
        },
    )]))
}

/// **An entry already corrected reads as what it was corrected to**, so it
/// is not offered again as the exercise it was corrected away from.
#[test]
fn an_entry_reads_as_its_correction() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(async {
            let (pool, _directory) = derived().await.expect("a derived store");
            let stand_ins = HevyStandIns::new(pool);
            let zones = zones().expect("a zone");
            let everywhere = Within::default();

            let pull_ups = stand_ins
                .reading_as(PULL_UP, &zones, &EditOverlay::default(), &everywhere)
                .await
                .expect("the landed workouts");
            let first = pull_ups.first().expect("the corpus holds pull-ups").clone();
            let overlay = corrected(NEUTRAL, &first.term).expect("a correction");

            let still = stand_ins
                .reading_as(PULL_UP, &zones, &overlay, &everywhere)
                .await
                .expect("the landed workouts");
            assert_eq!(still.len(), pull_ups.len() - 1);
            assert!(!still.contains(&first));

            let neutral = stand_ins
                .reading_as(NEUTRAL, &zones, &overlay, &everywhere)
                .await
                .expect("the landed workouts");
            assert!(neutral.contains(&first), "{neutral:?}");
        });
}

/// **A search held to a day finds that day's entries and no other's**: what
/// a change made from one session reaches when it is that session alone.
#[test]
fn a_search_is_held_to_the_day_it_is_made_from() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(async {
            let (pool, _directory) = derived().await.expect("a derived store");
            let stand_ins = HevyStandIns::new(pool);
            let zones = zones().expect("a zone");

            let pull_ups = stand_ins
                .reading_as(PULL_UP, &zones, &EditOverlay::default(), &Within::default())
                .await
                .expect("the landed workouts");
            let first = pull_ups.first().expect("the corpus holds pull-ups");
            let within = Within {
                from: Some(first.day),
                until: Some(first.day),
            };

            let held = stand_ins
                .reading_as(PULL_UP, &zones, &EditOverlay::default(), &within)
                .await
                .expect("the landed workouts");
            assert!(held.contains(first), "{held:?}");
            assert!(held.iter().all(|entry| entry.day == first.day), "{held:?}");
            assert!(held.len() < pull_ups.len(), "{held:?}");
        });
}
