//! The normalised layer through its real store, against a real SQLite file.
//!
//! The suites beside this one drive the use case with in-memory ports, because
//! what they assert is the translation. This one asserts the half they cannot:
//! that what the derivation produced survives a round trip through the store,
//! and that § 7's re-derivation holds at the file rather than only in memory.

mod support;

use application::{
    ExtractionRunLog as _, LandingStore as _, NormalisationSummary, RefusalReporter,
    WorkoutNormaliser,
    normalise::{Normalisation, NormalisationPorts, Refusals},
};
use infrastructure::{
    HevyWorkoutLandingReader, HevyWorkoutLandingStore, HevyWorkoutTranslator,
    SqliteExtractionRunLog, SqliteGymWorkoutStore, SqliteNormalisationRunLog, SqliteRefusalStore,
    connect,
};
use sqlx::SqlitePool;
use support::corpus;

/// A store holding the landed corpus, in a temporary file.
///
/// Returns `Result` and the test unwraps at the call site: `clippy.toml`'s
/// exemptions cover a `#[test]` body, not a helper defined beside one.
async fn landed() -> Result<(SqlitePool, tempfile::TempDir), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;

    let landing = HevyWorkoutLandingStore::new(pool.clone())?;
    let runs = SqliteExtractionRunLog::new(pool.clone());

    let run = runs
        .begin(landing.stream(), domain::landing::FetchedAt::EPOCH)
        .await?;

    // The fixture's records carry their own landing ids; appending them here
    // re-assigns from one, and the corpus is exported in that order, so the two
    // agree.
    let records = corpus::records()?
        .into_iter()
        .map(|landed| landed.record().clone())
        .collect();
    landing.append(run, records).await?;

    Ok((pool, directory))
}

async fn derive(pool: &SqlitePool) -> Result<NormalisationSummary, Box<dyn std::error::Error>> {
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
    Ok(normalisation.normalise().await?)
}

/// Everything the layer holds, in a form two derivations can be compared by.
///
/// Deliberately excludes `run_id`. That column records *which* derivation wrote
/// a row, which is provenance about the run rather than anything the entity
/// says — so it differs between two derivations and must, while the content
/// does not and must not.
async fn content(pool: &SqlitePool) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut rows = Vec::new();

    for row in sqlx::query!(
        r#"
        SELECT landing_record_id AS "id!: i64", source_record_id AS "source!: String",
               started_at_utc AS "started!: String", zone AS "zone!: String"
        FROM gym_workout ORDER BY landing_record_id
        "#
    )
    .fetch_all(pool)
    .await?
    {
        rows.push(format!(
            "{} {} {} {}",
            row.id, row.source, row.started, row.zone
        ));
    }

    for row in sqlx::query!(
        r#"
        SELECT workout AS "workout!: i64", item_position AS "item!: i64",
               exercise_position AS "exercise!: i64", position AS "position!: i64",
               load_kind AS "load_kind!: String", load_grams AS "load_grams!: i64",
               reps AS "reps: i64", duration_seconds AS "duration: i64",
               distance_mm AS "distance: i64", rir AS "rir: String",
               set_kind AS "set_kind!: String"
        FROM performed_set
        ORDER BY workout, item_position, exercise_position, position
        "#
    )
    .fetch_all(pool)
    .await?
    {
        rows.push(format!(
            "{} {} {} {} {} {} {:?} {:?} {:?} {:?} {}",
            row.workout,
            row.item,
            row.exercise,
            row.position,
            row.load_kind,
            row.load_grams,
            row.reps,
            row.duration,
            row.distance,
            row.rir,
            row.set_kind
        ));
    }

    Ok(rows)
}

/// The counts, read back out of the file rather than out of the derivation
/// that produced them. If the store dropped a row this is what notices.
#[test]
fn the_stored_layer_holds_what_the_derivation_produced() {
    let outcome = corpus::block_on(async {
        let (pool, _directory) = landed().await?;
        let summary = derive(&pool).await?;

        let workouts = sqlx::query!(r#"SELECT count(*) AS "n!: i64" FROM gym_workout"#)
            .fetch_one(&pool)
            .await?
            .n;
        let entries = sqlx::query!(r#"SELECT count(*) AS "n!: i64" FROM performed_exercise"#)
            .fetch_one(&pool)
            .await?
            .n;
        let sets = sqlx::query!(r#"SELECT count(*) AS "n!: i64" FROM performed_set"#)
            .fetch_one(&pool)
            .await?
            .n;
        let supersets =
            sqlx::query!(r#"SELECT count(*) AS "n!: i64" FROM workout_item WHERE is_superset = 1"#)
                .fetch_one(&pool)
                .await?
                .n;
        let refusals = sqlx::query!(r#"SELECT count(*) AS "n!: i64" FROM normalisation_refusal"#)
            .fetch_one(&pool)
            .await?
            .n;

        Ok::<_, Box<dyn std::error::Error>>((summary, workouts, entries, sets, supersets, refusals))
    });

    let Ok(Ok((summary, workouts, entries, sets, supersets, refusals))) = outcome else {
        panic!("the corpus lands and derives")
    };

    assert_eq!(workouts, 163, "workouts");
    assert_eq!(entries, 1_135, "performed exercises");
    assert_eq!(sets, 3_778, "performed sets");
    assert_eq!(supersets, 334, "supersets");
    assert_eq!(refusals, 3, "refusals");
    assert_eq!(summary.workouts_written.as_usize(), 163);
    assert!(summary.reconciles(), "{summary:?}");
}

/// § 7 and SC-004, at the file. Derive, derive again, then discard the layer
/// entirely and derive a third time — the content is identical every time, and
/// no request is made to any source because the derivation holds no port that
/// could make one.
#[test]
fn re_derivation_restores_the_layer_identically() {
    let outcome = corpus::block_on(async {
        let (pool, _directory) = landed().await?;

        derive(&pool).await?;
        let first = content(&pool).await?;

        derive(&pool).await?;
        let second = content(&pool).await?;

        // Discard it entirely, as an operator would to force a rebuild.
        sqlx::query!("DELETE FROM performed_set")
            .execute(&pool)
            .await?;
        sqlx::query!("DELETE FROM performed_exercise")
            .execute(&pool)
            .await?;
        sqlx::query!("DELETE FROM workout_item")
            .execute(&pool)
            .await?;
        sqlx::query!("DELETE FROM gym_workout")
            .execute(&pool)
            .await?;

        derive(&pool).await?;
        let rebuilt = content(&pool).await?;

        Ok::<_, Box<dyn std::error::Error>>((first, second, rebuilt))
    });

    let Ok(Ok((first, second, rebuilt))) = outcome else {
        panic!("the corpus lands and derives three times")
    };

    assert!(!first.is_empty(), "the layer is not empty");
    assert_eq!(first, second, "re-deriving over unchanged raw");
    assert_eq!(first, rebuilt, "rebuilding after a discard");
}

/// FR-023: refusals read back, and read back as what was written.
#[test]
fn refusals_survive_the_round_trip() {
    let outcome = corpus::block_on(async {
        let (pool, _directory) = landed().await?;
        derive(&pool).await?;

        let reporter = Refusals::new(
            SqliteRefusalStore::new(pool.clone())?,
            SqliteNormalisationRunLog::new(pool.clone()),
        );
        Ok::<_, Box<dyn std::error::Error>>(reporter.refusals().await?)
    });

    let Ok(Ok(report)) = outcome else {
        panic!("the refusals read back")
    };

    assert_eq!(report.refusals.len(), 3);
    assert!(
        report.derived_at.is_some(),
        "a report says when the derivation ran, so a stale list reads as stale"
    );

    let mut by_reason: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for refusal in &report.refusals {
        *by_reason.entry(refusal.reason.as_str()).or_insert(0) += 1;
    }
    assert_eq!(by_reason.get("zero-reps"), Some(&1));
    assert_eq!(by_reason.get("non-contiguous-grouping"), Some(&1));
    assert_eq!(by_reason.get("single-member-grouping"), Some(&1));

    // The exercise survives, which is what makes a refused set actionable
    // without re-reading the payload.
    assert!(
        report
            .refusals
            .iter()
            .filter(|refusal| refusal.reason.as_str() == "zero-reps")
            .all(|refusal| refusal.exercise.is_some()),
        "a refused set names its exercise after a round trip"
    );
}
