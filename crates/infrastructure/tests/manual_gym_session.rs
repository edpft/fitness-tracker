//! Manual gym sessions from the historical spreadsheets (#274). The fixtures
//! are small invented workbooks laid out as the operator's are.

use application::{Translation, ports::Translator};
use domain::{
    gym::{ManualExercise, ManualGymSession, ManualSet},
    landing::{
        FetchedAt, FilePath, FileProvenance, LandedRecord, LandingRecord, LandingRecordId,
        LandingStream, ModifiedAt, RawPayload, SourceRecordId,
    },
    normalised::{OperatorZone, Refusal, RefusalReason},
};
use infrastructure::{SpreadsheetSessionTranslator, Workbook};

type Failure = Box<dyn std::error::Error>;

const SESSIONS: &[u8] = include_bytes!("fixtures/spreadsheets/gym-sessions.xlsx");
const CONDITIONING: &[u8] = include_bytes!("fixtures/spreadsheets/conditioning-2017.xlsx");
const BEGINNER: &[u8] = include_bytes!("fixtures/spreadsheets/beginner-2020.xlsx");
const FINAL_2017: &[u8] = include_bytes!("fixtures/spreadsheets/ct-2017-final.xlsx");
const EARLY_2017: &[u8] = include_bytes!("fixtures/spreadsheets/ct-2017-early.xlsx");

fn record(path: &str, bytes: &[u8]) -> Result<LandedRecord, Failure> {
    landed(7, path, bytes)
}

fn landed(id: i64, path: &str, bytes: &[u8]) -> Result<LandedRecord, Failure> {
    let provenance = FileProvenance::new(
        FilePath::try_from(path)?,
        ModifiedAt::try_from("2019-03-14T07:59:28Z")?,
    );
    let landed = LandingRecord::land(
        LandingStream::try_from("spreadsheets.files")?,
        FetchedAt::try_from("2026-09-27T12:00:00Z")?,
        SourceRecordId::try_from(format!("digest-of-file-{id}"))?,
        provenance.into(),
        RawPayload::try_from(bytes.to_vec())?,
    );
    Ok(LandedRecord::new(LandingRecordId::try_from(id)?, landed))
}

fn translate(bytes: &[u8]) -> Result<Translation<ManualGymSession>, Failure> {
    let zone = OperatorZone::try_from("Europe/London")?;
    let workbook = Workbook::of(record("Dropbox/Random/1RM.xlsx", bytes)?);
    Ok(SpreadsheetSessionTranslator.translate(&workbook, &zone)?)
}

fn set<M: std::fmt::Display>(set: &ManualSet<M>) -> String {
    let load = set
        .load
        .map_or_else(|| "?".to_owned(), |load| load.to_string());
    let intensity = set
        .intensity
        .map(|intensity| format!(" @ {intensity}"))
        .unwrap_or_default();
    let rest = set
        .rest_after
        .map(|rest| format!(" rest {}s", rest.as_seconds()))
        .unwrap_or_default();
    format!(
        "{load} × {}{intensity}{rest} [{}!{}]",
        set.outcome, set.written_in.sheet, set.written_in.cell
    )
}

/// Which file each set was taken from, in order.
fn files(session: &ManualGymSession) -> Vec<String> {
    session
        .exercises()
        .iter()
        .flat_map(|exercise| match exercise {
            ManualExercise::ForReps { sets, .. } => sets
                .iter()
                .map(|set| set.written_in.file.to_string())
                .collect::<Vec<_>>(),
            ManualExercise::ForDuration { sets, .. } => sets
                .iter()
                .map(|set| set.written_in.file.to_string())
                .collect(),
            ManualExercise::ForDistance { sets, .. } => sets
                .iter()
                .map(|set| set.written_in.file.to_string())
                .collect(),
        })
        .collect()
}

/// Each session as `day: exercise: set; set | exercise: …`, which is what the
/// operator would check against the sheet.
fn described(sessions: &[&ManualGymSession]) -> Vec<String> {
    sessions
        .iter()
        .map(|session| {
            let exercises: Vec<String> = session
                .exercises()
                .iter()
                .map(|exercise| {
                    let sets: Vec<String> = match exercise {
                        ManualExercise::ForReps { sets, .. } => sets.iter().map(set).collect(),
                        ManualExercise::ForDuration { sets, .. } => sets.iter().map(set).collect(),
                        ManualExercise::ForDistance { sets, .. } => sets.iter().map(set).collect(),
                    };
                    format!("{}: {}", exercise.exercise_key(), sets.join("; "))
                })
                .collect();
            format!("{}: {}", session.on(), exercises.join(" | "))
        })
        .collect()
}

fn details(refusals: &[Refusal]) -> Vec<String> {
    refusals
        .iter()
        .map(|refusal| match &refusal.reason {
            RefusalReason::Unmodelled { detail } => detail.clone(),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn a_dated_session_sheet_is_its_sets_in_order_and_a_plan_is_refused_as_one() {
    let translation = translate(SESSIONS).expect("translates");
    let Translation::Entities { entities, refusals } = translation else {
        panic!("expected sessions, got {translation:?}");
    };

    assert_eq!(
        described(&entities.iter().collect::<Vec<_>>()),
        [
            "2019-02-12: squat-barbell: 50 kg × 5 [Squat day (19-02-12)!C2]; \
             80 kg × 1 @ 1-2 rest 180s [Squat day (19-02-12)!C3]; \
             87.5 kg × failed rest 180s [Squat day (19-02-12)!C4] \
             | chest-dip: bodyweight × 5 rest 90s [Squat day (19-02-12)!C5] \
             | hip-thrust-barbell: 65 kg × 10 [Squat day (19-02-12)!C6]",
            "2016-04-04: deadlift-barbell: 37.5 kg × 6 [Weights!C3]; 72.5 kg × 5 [Weights!E3] \
             | squat-barbell: 20 kg × 6 [Weights!C8]; 40 kg × 3 [Weights!E8]",
        ],
        "the undated `Squat day` is the template, and 1 June 2016 is not recorded as done"
    );
    assert_eq!(
        details(&refusals),
        [
            "Bench day (2019-02-14) (2019-02-14) is a plan, with nothing to show it was performed",
            "Weights, 2016-06-01 (2016-06-01) has a working set with a load and no reps, so the \
             sheet does not record it performed",
        ]
    );
}

#[test]
fn a_ct_2017_week_is_dated_by_the_programme_and_two_pushes_run_push_pull_push() {
    let translation = translate(CONDITIONING).expect("translates");
    let Translation::Entities { entities, refusals } = translation else {
        panic!("expected sessions, got {translation:?}");
    };

    assert_eq!(
        described(&entities.iter().collect::<Vec<_>>()),
        [
            "2017-02-20: bench-press-barbell: 22.5 kg × 6 [Push!E2]; 45 kg × 5 [Push!G2]; \
             45 kg × 5 [Push!I2] | chest-dip: bodyweight × 3 [Push!Q2]",
            "2017-02-24: bench-press-barbell: 45 kg × 5 [Push!G3]; 45 kg × 4 [Push!I3] \
             | chest-dip: bodyweight × 4 [Push!Q3]",
            "2017-02-22: pull-up: bodyweight × 3 [Pull!E2]",
            "2017-01-11: squat-barbell: 20 kg × 6 [Legs!D2]; 40 kg × 10 [Legs!F2]",
        ],
        "week 8 is push Monday, pull Wednesday, push Friday; an empty-bar warm-up is a blank"
    );
    assert_eq!(
        details(&refusals),
        ["Push, workout 8 (2017-03-27) is a plan, with nothing to show it was performed"]
    );
}

#[test]
fn copies_of_ct_2017_merge_into_one_session_and_the_most_recent_wins() {
    const FINAL: &str = "Dropbox/Random/CT 2017.xlsx";
    const MARCH: &str = "Dropbox/Random/CT 2017 (Netbook's conflicted copy 2017-03-29).xlsx";
    let zone = OperatorZone::try_from("Europe/London").expect("zone");
    let workbooks = Workbook::gather(vec![
        landed(1, FINAL, FINAL_2017).expect("final"),
        landed(2, MARCH, EARLY_2017).expect("March copy"),
    ]);
    let [workbook] = workbooks.as_slice() else {
        panic!("the copies are one workbook, got {}", workbooks.len());
    };
    let translation = SpreadsheetSessionTranslator
        .translate(workbook, &zone)
        .expect("translates");
    let Translation::Entities { entities, refusals } = translation else {
        panic!("expected sessions, got {translation:?}");
    };

    assert_eq!(
        described(&entities.iter().collect::<Vec<_>>()),
        [
            "2017-01-11: squat-barbell: 20 kg × 6 [Legs!D2]; 40 kg × 10 [Legs!F2] \
             | deadlift-barbell: 42.5 kg × 2 [Deadlift!C2]; 47.5 kg × 10 [Legs!O2] \
             | suitcase-carry: 36 kg × 20m [Carry, Left!C2] \
             | dead-bug: 1 kg × 10 [Deadbugs!C2]",
            "2017-01-13: bicep-curl-barbell: 15 kg × 10 [Pull!E2]",
        ],
        "one session per workout; the March copy adds the deadlift warm-up the final left at \
         0 kg, a carry of one 20 m walk and the dead bugs, and the final's 15 kg curl beats the \
         March copy's 10 kg"
    );
    let legs = entities.first();
    assert_eq!(
        files(legs),
        [FINAL, FINAL, MARCH, FINAL, MARCH, MARCH],
        "each set names the copy it was taken from"
    );
    assert_eq!(
        legs.logged().file.to_string(),
        FINAL,
        "dated by the most recent copy"
    );
    assert_eq!(
        details(&refusals),
        [format!(
            "Landmines, workout 1 in {MARCH}: Push, workout 1 was not performed"
        )]
    );
}

#[test]
fn the_2020_log_has_no_loads_and_its_reps_in_reserve_are_the_evidence() {
    let translation = translate(BEGINNER).expect("translates");
    let Translation::Entities { entities, refusals } = translation else {
        panic!("expected sessions, got {translation:?}");
    };

    assert_eq!(
        described(&entities.iter().collect::<Vec<_>>()),
        [
            "2020-10-12: squat-barbell: ? × 4 [Sheet1!F2]; ? × 4 @ 4+ [Sheet1!F3] \
             | overhead-press-dumbbell: ? × 4 @ 2-3 [Sheet1!F4]",
        ],
        "day 1 of the week ending 18 October is Monday the 12th; its press is dumbbells"
    );
    assert_eq!(
        details(&refusals),
        [
            "Sheet1, week ending 2020-10-18, day 2 (2020-10-14) is a plan, with nothing to show it \
             was performed",
        ]
    );
}

mod store {
    use application::{
        ExtractionRunLog as _, LandingStore as _, NormalisationSummary, WorkoutNormaliser as _,
        normalise::{Normalisation, NormalisationPorts},
        ports::Clock,
    };
    use domain::{
        landing::{FetchedAt, LandingRecord},
        normalised::OperatorZone,
    };
    use infrastructure::{
        SpreadsheetFileAccountReader, SpreadsheetFileLandingStore, SpreadsheetTranslator,
        SqliteExtractionRunLog, SqliteNormalisationRunLog, SqliteRefusalStore,
        SqliteSpreadsheetStore, connect,
    };
    use sqlx::SqlitePool;

    use super::{Failure, SESSIONS, record};

    struct EpochClock;

    impl Clock for EpochClock {
        fn now(&self) -> FetchedAt {
            FetchedAt::EPOCH
        }
    }

    fn landing_record(path: &str, bytes: &[u8]) -> Result<LandingRecord, Failure> {
        Ok(record(path, bytes)?.record().clone())
    }

    async fn derive(pool: &SqlitePool) -> Result<NormalisationSummary, Failure> {
        let normalisation = Normalisation::new(
            NormalisationPorts {
                raw: SpreadsheetFileAccountReader::new(pool.clone())?,
                translator: SpreadsheetTranslator,
                workouts: SqliteSpreadsheetStore::new(pool.clone())?,
                refusals: SqliteRefusalStore::new(
                    pool.clone(),
                    SpreadsheetFileLandingStore::STREAM,
                )?,
                runs: SqliteNormalisationRunLog::new(pool.clone()),
                clock: EpochClock,
            },
            OperatorZone::try_from("Europe/London")?,
        );
        Ok(normalisation.normalise().await?)
    }

    async fn count(pool: &SqlitePool, sql: &'static str) -> Result<i64, Failure> {
        Ok(sqlx::query_scalar(sql).fetch_one(pool).await?)
    }

    fn block_on<T>(body: impl std::future::Future<Output = T>) -> Result<T, Failure> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        Ok(runtime.block_on(body))
    }

    #[test]
    fn sessions_share_the_gym_tables_and_a_re_derivation_replaces_only_its_own() {
        let outcome: Result<(), Failure> = block_on(async {
            let directory = tempfile::tempdir()?;
            let pool = connect(&directory.path().join("test.db")).await?;
            let landing = SpreadsheetFileLandingStore::new(pool.clone())?;
            let runs = SqliteExtractionRunLog::new(pool.clone());
            let run = runs.begin(landing.stream(), FetchedAt::EPOCH).await?;
            landing
                .append(
                    run,
                    vec![landing_record("Dropbox/Random/1RM.xlsx", SESSIONS)?],
                )
                .await?;

            let first = derive(&pool).await?;
            assert_eq!(first.workouts_written.as_usize(), 2);
            assert!(first.reconciles());

            // A Hevy session, as that derivation writes one.
            sqlx::query(
                "INSERT INTO gym_session (stream, landing_record_id, run_id)
                 SELECT 'hevy.workouts', 1, MAX(id) FROM normalisation_run",
            )
            .execute(&pool)
            .await?;

            let second = derive(&pool).await?;
            assert_eq!(second.workouts_written.as_usize(), 2);
            assert_eq!(
                count(
                    &pool,
                    "SELECT COUNT(*) FROM gym_session WHERE stream = 'spreadsheets.files'"
                )
                .await?,
                2,
                "the first derivation's sessions were replaced, not added to"
            );
            assert_eq!(
                count(
                    &pool,
                    "SELECT COUNT(*) FROM gym_session WHERE stream = 'hevy.workouts'"
                )
                .await?,
                1,
                "another source's session survives"
            );
            assert_eq!(
                count(
                    &pool,
                    "SELECT COUNT(*) FROM performed_set AS s
                     JOIN gym_workout AS w ON w.id = s.workout
                     WHERE w.stream = 'spreadsheets.files' AND s.sheet IS NOT NULL"
                )
                .await?,
                9,
                "every set names its sheet and cell"
            );
            Ok(())
        })
        .and_then(|result| result);
        outcome.expect("the derivation round-trips");
    }
}
