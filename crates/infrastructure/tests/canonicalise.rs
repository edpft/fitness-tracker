//! The canonical gym layer built through its real stores, against a real
//! SQLite file (#247).
//!
//! The suite in `domain` asserts the matching and the merge over accounts built
//! by hand. This one asserts the half it cannot: that
//! `SqliteNormalisedGymSessionReader` reads the normalised layer back in the
//! shape the merge expects, for both shapes it holds, and that what the merge
//! produced survives the write.
//!
//! **The Hevy corpus is the operator's own**, 164 landed records. The watch's
//! activity beside it is invented, in the shape his 478 landed gym activities
//! hold: one of them placed on a day the corpus already has a session for, so
//! the merge has two accounts of one visit to join.
//!
//! Tests return `()` and assert by panicking; the helpers return `Result` for
//! the test to unwrap, since the `clippy.toml` exemptions cover a `#[test]`
//! body and not a function beside one.

mod support;

use application::{
    CanonicalGymSessionStore as _, ExtractionRunLog as _, LandingStore as _,
    NormalisedGymSessionReader as _, WorkoutNormaliser as _, canonicalise,
    normalise::{Normalisation, NormalisationPorts},
};
use domain::{
    gym::CanonicalGymSession,
    landing::{
        Endpoint, EventKind, EventProvenance, FetchedAt, LandingRecord, LandingStream, RawPayload,
        SourceRecordId,
    },
    normalised::OperatorZone,
};
use infrastructure::{
    GarminActivityLandingStore, GarminGymAccountReader, HevySessionAccountReader,
    HevySessionTranslator, HevyWorkoutLandingStore, SqliteCanonicalGymSessionStore,
    SqliteExtractionRunLog, SqliteGymSessionStore, SqliteMeasuredGymSessionStore,
    SqliteNormalisationRunLog, SqliteNormalisedGymSessionReader, SqliteRefusalStore, connect,
    garmin::GarminGymTranslator,
};
use serde_json::json;
use sqlx::SqlitePool;
use support::corpus;

type Failure = Box<dyn std::error::Error>;

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

fn zone() -> Result<OperatorZone, Failure> {
    Ok(OperatorZone::try_from("Europe/London".to_owned())?)
}

/// A watch activity, in the shape Garmin's list endpoint serves one.
fn activity(id: &str, started: &str, seconds: u32) -> Result<LandingRecord, Failure> {
    let payload = json!({
        "activityId": id.parse::<i64>()?,
        "activityName": "Strength",
        "startTimeLocal": started,
        "startTimeGMT": started,
        "duration": f64::from(seconds),
        "elapsedDuration": f64::from(seconds),
        "averageHR": 118.0,
        "maxHR": 164.0,
        "activityType": {"typeKey": "strength_training"},
    });
    let provenance = EventProvenance::new(
        Endpoint::try_from("/activitylist-service/activities/search/activities")?,
        EventKind::try_from("updated")?,
        None,
    );
    let landed = LandingRecord::land(
        LandingStream::try_from("garmin.activities")?,
        FetchedAt::try_from("2026-09-18T08:00:00Z")?,
        SourceRecordId::try_from(id)?,
        provenance.into(),
        RawPayload::try_from(serde_json::to_vec(&payload)?)?,
    );
    Ok(landed)
}

/// The store, with the Hevy corpus and one watch activity landed and both
/// streams normalised.
///
/// Not `Send`, and it does not need to be: every caller drives it on the
/// current-thread runtime its own `#[test]` built, so the future never crosses
/// a thread.
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

    let garmin = GarminActivityLandingStore::new(pool.clone())?;
    let run = runs.begin(garmin.stream(), FetchedAt::EPOCH).await?;
    garmin
        .append(
            run,
            vec![
                // On a day the corpus holds a Hevy session: two accounts of
                // one visit.
                activity("24276489347", "2026-08-10T17:05:00", 3_600)?,
                // On a day it does not, and with no sets: a normalised session
                // and no canonical one.
                activity("24276489999", "2026-08-11T17:05:00", 1_800)?,
            ],
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
        zone()?,
    )
    .normalise()
    .await?;

    Normalisation::new(
        NormalisationPorts {
            raw: GarminGymAccountReader::new(pool.clone())?,
            translator: GarminGymTranslator,
            workouts: SqliteMeasuredGymSessionStore::new(pool.clone())?,
            refusals: SqliteRefusalStore::new(pool.clone(), GarminActivityLandingStore::STREAM)?,
            runs: SqliteNormalisationRunLog::new(pool.clone()),
            clock: corpus::FixedClock,
        },
        zone()?,
    )
    .normalise()
    .await?;

    Ok((pool, directory))
}

/// The reader hands back every normalised session, whatever recorded it.
#[test]
fn the_reader_sees_both_shapes() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = derived().await.expect("a derived store");
        let reader = SqliteNormalisedGymSessionReader::new(pool.clone());
        let accounts = reader.all().await.expect("the normalised layer");

        let watched = accounts
            .iter()
            .filter(|account| account.recorder() == domain::gym::Recorder::Watch)
            .count();
        assert_eq!(watched, 2, "both watch activities");
        assert!(
            accounts.len() > watched,
            "and the Hevy sessions beside them: {}",
            accounts.len()
        );
        assert!(
            accounts
                .iter()
                .any(domain::gym::NormalisedGymSession::has_items),
            "a log's exercises came back"
        );
        // Oldest first, § 9: the order is the reader's and not SQLite's.
        let days: Vec<_> = accounts
            .iter()
            .map(|account| account.occurred().day())
            .collect();
        let mut sorted = days.clone();
        sorted.sort_unstable();
        assert_eq!(days, sorted, "oldest first");
    });
}

/// The whole use case: read, match, merge, write, and read back.
#[test]
fn a_visit_two_sources_recorded_is_one_canonical_session() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = derived().await.expect("a derived store");
        let reader = SqliteNormalisedGymSessionReader::new(pool.clone());
        let canonical = SqliteCanonicalGymSessionStore::new(pool.clone());

        let done = canonicalise::gym_sessions(&reader, &canonical)
            .await
            .expect("a canonicalising run");
        assert!(
            done.written.as_usize() < done.read.as_usize(),
            "accounts merged and the sessionless recording wrote nothing: \
             read {}, wrote {}",
            done.read,
            done.written
        );
        assert_eq!(
            canonical.count().await.expect("a count"),
            done.written,
            "what it reported is what it holds"
        );

        let sessions = canonical.all().await.expect("the canonical layer");
        let day = "2026-08-10".parse().expect("a date");
        let merged = sessions
            .iter()
            .find(|session| session.occurred().day() == day)
            .expect("the visit both sources recorded");
        assert!(
            merged.heart_rate().is_some(),
            "the watch's heart rate reached it"
        );
        assert!(
            merged.stands_on().len() > 1,
            "and it names both accounts: {:?}",
            merged.stands_on()
        );

        // The watch's 11 August recording holds no exercise, so § II.4 gives it
        // no entry: the operator, 2026-10-01, "Heart rate only isn't a
        // meaningful gym session."
        let alone = "2026-08-11".parse().expect("a date");
        assert!(
            !sessions
                .iter()
                .any(|session: &CanonicalGymSession| session.occurred().day() == alone),
            "a heart rate and nothing else is no canonical session"
        );
    });
}

/// Replacing the layer twice leaves what it left the first time (§ II).
#[test]
fn the_layer_is_replaced_not_appended() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = derived().await.expect("a derived store");
        let reader = SqliteNormalisedGymSessionReader::new(pool.clone());
        let canonical = SqliteCanonicalGymSessionStore::new(pool.clone());

        let first = canonicalise::gym_sessions(&reader, &canonical)
            .await
            .expect("a first run");
        let again = canonicalise::gym_sessions(&reader, &canonical)
            .await
            .expect("a second run");
        assert_eq!(first, again, "the same inputs give the same layer");
        assert_eq!(
            canonical.count().await.expect("a count"),
            first.written,
            "and nothing accumulated"
        );
    });
}

/// Re-deriving a normalised gym stream leaves the canonical layer rebuildable.
///
/// **The defect this pins.** Every field of a canonical session names the
/// `gym_session` row it came from, and re-deriving a stream deletes those rows
/// and writes new ones under new ids — so a canonical layer left standing is a
/// layer holding foreign keys to rows that no longer exist. SQLite said so,
/// `FOREIGN KEY constraint failed`, the second time a store that had been
/// canonicalised was normalised; and `fitness next` canonicalises every run, so
/// the second run would have been every run (#350).
///
/// So re-deriving clears the whole layer, and canonicalising builds it again.
#[test]
fn re_deriving_a_stream_leaves_the_layer_rebuildable() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let (pool, _directory) = derived().await.expect("a derived store");
        let reader = SqliteNormalisedGymSessionReader::new(pool.clone());
        let canonical = SqliteCanonicalGymSessionStore::new(pool.clone());

        let first = canonicalise::gym_sessions(&reader, &canonical)
            .await
            .expect("a first run");
        assert!(first.written.as_usize() > 0, "the layer was built");

        // The same derivation again, over the same raw: the normalised rows are
        // replaced, so the layer standing on them cannot survive.
        Normalisation::new(
            NormalisationPorts {
                raw: HevySessionAccountReader::new(pool.clone()).expect("a reader"),
                translator: HevySessionTranslator::default(),
                workouts: SqliteGymSessionStore::new(pool.clone()).expect("a store"),
                refusals: SqliteRefusalStore::new(pool.clone(), HevyWorkoutLandingStore::STREAM)
                    .expect("a refusal store"),
                runs: SqliteNormalisationRunLog::new(pool.clone()),
                clock: corpus::FixedClock,
            },
            zone().expect("a zone"),
        )
        .normalise()
        .await
        .expect("re-deriving succeeds with a canonical layer in the store");

        assert_eq!(
            canonical.count().await.expect("a count").as_usize(),
            0,
            "the stale layer is gone rather than left pointing at deleted rows"
        );

        let again = canonicalise::gym_sessions(&reader, &canonical)
            .await
            .expect("a second canonicalising");
        assert_eq!(again, first, "and it rebuilds to the same layer");
    });
}
