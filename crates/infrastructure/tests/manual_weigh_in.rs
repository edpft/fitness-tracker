//! Manual weigh-ins from the historical spreadsheets (#273). The fixtures are
//! small invented workbooks laid out as the operator's are.

use application::{Translation, ports::Translator};
use domain::{
    body::ManualWeighIn,
    landing::{
        FetchedAt, FilePath, FileProvenance, LandedRecord, LandingRecord, LandingRecordId,
        LandingStream, ModifiedAt, RawPayload, SourceRecordId,
    },
    normalised::{OperatorZone, Refusal, RefusalReason},
};
use infrastructure::SpreadsheetWeighInTranslator;

type Failure = Box<dyn std::error::Error>;

const WEIGH_INS: &[u8] = include_bytes!("fixtures/spreadsheets/weigh-ins.xlsx");
const TRAINING_LOG: &[u8] = include_bytes!("fixtures/spreadsheets/training-log.xlsx");
const COMPOSITION: &[u8] = include_bytes!("fixtures/spreadsheets/composition.xlsx");

fn record(path: &str, bytes: &[u8]) -> Result<LandedRecord, Failure> {
    let provenance = FileProvenance::new(
        FilePath::try_from(path)?,
        ModifiedAt::try_from("2018-05-19T09:00:00Z")?,
    );
    let landed = LandingRecord::land(
        LandingStream::try_from("spreadsheets.files")?,
        FetchedAt::try_from("2026-09-27T12:00:00Z")?,
        SourceRecordId::try_from("digest-of-the-file")?,
        provenance.into(),
        RawPayload::try_from(bytes.to_vec())?,
    );
    Ok(LandedRecord::new(LandingRecordId::try_from(7)?, landed))
}

fn translate(path: &str, bytes: &[u8]) -> Result<Translation<ManualWeighIn>, Failure> {
    let zone = OperatorZone::try_from("Europe/London")?;
    Ok(SpreadsheetWeighInTranslator.translate(&record(path, bytes)?, &zone)?)
}

/// Each weigh-in as `sheet!cell day kg`, which is what the operator would look
/// up.
fn described(weigh_ins: &[&ManualWeighIn]) -> Vec<String> {
    weigh_ins
        .iter()
        .map(|weigh_in| {
            let cell = weigh_in.written_in();
            format!(
                "{}!{} {} {}",
                cell.sheet,
                cell.cell,
                weigh_in.on(),
                weigh_in.mass()
            )
        })
        .collect()
}

fn refused_detail(refusal: &Refusal) -> String {
    match &refusal.reason {
        RefusalReason::UnreadableValue { detail, .. } | RefusalReason::Unmodelled { detail } => {
            detail.clone()
        }
        other => format!("{other:?}"),
    }
}

#[test]
fn every_filled_weight_cell_is_a_weigh_in_named_by_its_cell() {
    let translation = translate("Dropbox/Random/Body Weight.xlsx", WEIGH_INS).expect("translates");
    let Translation::Entities { entities, refusals } = translation else {
        panic!("expected weigh-ins, got {translation:?}");
    };

    assert_eq!(
        described(&entities.iter().collect::<Vec<_>>()),
        [
            "2016!C2 2016-01-01 67.5",
            "2016!C7 2016-01-06 66.3",
            "2018!C5 2018-01-01 73.9",
            "Weight, BMI, Fat%!B3 2014-12-29 68.3",
            "Weight, BMI, Fat%!C3 2014-12-30 68",
        ],
        "a blank, ??? and 0 are gaps; the summary block, the training log and \
         the 7-day average are not weigh-ins"
    );

    let refusals: Vec<String> = refusals.iter().map(refused_detail).collect();
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert!(
        refusals.iter().all(|detail| detail.starts_with("2016!C6:")),
        "{refusals:?}"
    );
}

/// Each composition figure as `sheet!cell component percentage`, beside the
/// weigh-in it is read against.
fn composition_of(weigh_ins: &[&ManualWeighIn]) -> Vec<String> {
    weigh_ins
        .iter()
        .flat_map(|weigh_in| {
            weigh_in.composition().iter().map(|written| {
                format!(
                    "{}!{} {} {} beside {}",
                    written.written_in.sheet,
                    written.written_in.cell,
                    written.component.as_str(),
                    written.percentage,
                    weigh_in.written_in().cell,
                )
            })
        })
        .collect()
}

#[test]
fn fat_and_water_are_read_beside_the_mass_on_their_row() {
    let translation =
        translate("Dropbox/Random/Body Weight 2018.xlsx", COMPOSITION).expect("translates");
    let Translation::Entities { entities, refusals } = translation else {
        panic!("expected weigh-ins, got {translation:?}");
    };
    let weigh_ins: Vec<&ManualWeighIn> = entities.iter().collect();

    assert_eq!(
        described(&weigh_ins),
        [
            "2018!C2 2018-01-01 73.9",
            "2018!C3 2018-01-02 73.9",
            "2018!C4 2018-01-03 73.6",
            "2018!C6 2018-01-05 73.4",
            "Weight, BMI, Fat%!B2 2014-12-29 68.3",
            "Weight, BMI, Fat%!C2 2014-12-30 68",
        ]
    );
    assert_eq!(
        composition_of(&weigh_ins),
        [
            "2018!E2 fat 24.7% beside C2",
            "2018!F2 water 56.6% beside C2",
            "2018!E3 fat 24.6% beside C3",
            "2018!F6 water 57% beside C6",
            "Weight, BMI, Fat%!B3 fat 20% beside B2",
            "Weight, BMI, Fat%!B4 water 60.3% beside B2",
            "Weight, BMI, Fat%!C3 fat 19.8% beside C2",
        ],
        "a blank is a gap, in a column or a row"
    );

    let refusals: Vec<String> = refusals.iter().map(refused_detail).collect();
    assert_eq!(refusals.len(), 2, "{refusals:?}");
    assert!(
        refusals
            .first()
            .is_some_and(|detail| detail.starts_with("2018!E5:")),
        "a fat percentage with no mass on its row stands alone: {refusals:?}"
    );
    assert!(
        refusals
            .get(1)
            .is_some_and(|detail| detail.starts_with("2018!E6:")),
        "150 % is more than the whole: {refusals:?}"
    );
}

#[test]
fn a_weigh_in_names_the_file_it_was_written_in() {
    let translation = translate("Dropbox/Random/Body Weight.xlsx", WEIGH_INS).expect("translates");
    let Translation::Entities { entities, .. } = translation else {
        panic!("expected weigh-ins, got {translation:?}");
    };
    let cell = entities.first().written_in();

    assert_eq!(
        cell.to_string(),
        "Dropbox/Random/Body Weight.xlsx › 2016!C2"
    );
    assert_eq!(cell.source_record_id.as_str(), "digest-of-the-file");
}

#[test]
fn a_training_log_with_a_weight_column_is_not_weigh_ins() {
    let translation = translate("Dropbox/Random/1RM.xlsx", TRAINING_LOG).expect("translates");
    let Translation::Refused(refusals) = translation else {
        panic!("a training log's load is not a body mass: {translation:?}");
    };

    assert!(matches!(
        refusals.first().reason,
        RefusalReason::Unmodelled { .. }
    ));
    assert_eq!(
        refused_detail(refusals.first()),
        "Dropbox/Random/1RM.xlsx, a spreadsheet of something other than weigh-ins,"
    );
}

#[test]
fn a_csv_export_is_one_sheet_named_after_the_file() {
    let csv = b"date,body_weight\nMon 01 Jan 18,73.9 \nTue 02 Jan 18,\nWed 03 Jan 18,73.5 \n";
    let translation =
        translate("Dropbox/Random/2018 bodyweight only.csv", csv).expect("translates");
    let Translation::Entities { entities, refusals } = translation else {
        panic!("expected weigh-ins, got {translation:?}");
    };

    assert_eq!(
        described(&entities.iter().collect::<Vec<_>>()),
        [
            "2018 bodyweight only!B2 2018-01-01 73.9",
            "2018 bodyweight only!B4 2018-01-03 73.5",
        ]
    );
    assert!(refusals.is_empty(), "{refusals:?}");
}

#[test]
fn a_file_that_is_not_a_spreadsheet_is_refused_as_such() {
    let translation = translate(
        "OneDrive/Documents/2015-16 Training.docx",
        b"PK not a workbook",
    )
    .expect("translates");
    let Translation::Refused(refusals) = translation else {
        panic!("expected a refusal, got {translation:?}");
    };

    assert!(
        refused_detail(refusals.first()).starts_with(
            "OneDrive/Documents/2015-16 Training.docx, a file that is not a spreadsheet"
        ),
        "{refusals:?}"
    );
}

/// Through the store, beside another source's weigh-ins.
mod store {
    use application::CompositionHistory;
    use application::{
        ExtractionRunLog as _, LandingStore as _, NormalisationSummary, WeighInHistory as _,
        WorkoutNormaliser as _, canonicalise,
        normalise::{Normalisation, NormalisationPorts},
        ports::Clock,
    };
    use domain::{
        analytical::composition::Method,
        landing::{FetchedAt, LandingRecord},
        normalised::OperatorZone,
    };
    use infrastructure::{
        SpreadsheetFileAccountReader, SpreadsheetFileLandingStore, SpreadsheetTranslator,
        SqliteCanonicalWeighInStore, SqliteCompositionHistory, SqliteExtractionRunLog,
        SqliteNormalisationRunLog, SqliteNormalisedWeighInReader, SqliteRefusalStore,
        SqliteSpreadsheetStore, connect,
    };
    use sqlx::SqlitePool;

    use super::{COMPOSITION, Failure, TRAINING_LOG, WEIGH_INS, record};

    struct EpochClock;

    impl Clock for EpochClock {
        fn now(&self) -> FetchedAt {
            FetchedAt::EPOCH
        }
    }

    fn landing_record(path: &str, bytes: &[u8]) -> Result<LandingRecord, Failure> {
        Ok(record(path, bytes)?.record().clone())
    }

    async fn landed() -> Result<(SqlitePool, tempfile::TempDir), Failure> {
        let directory = tempfile::tempdir()?;
        let pool = connect(&directory.path().join("test.db")).await?;

        let landing = SpreadsheetFileLandingStore::new(pool.clone())?;
        let runs = SqliteExtractionRunLog::new(pool.clone());
        let run = runs.begin(landing.stream(), FetchedAt::EPOCH).await?;
        let records = vec![
            landing_record("Dropbox/Random/Body Weight.xlsx", WEIGH_INS)?,
            landing_record("Dropbox/Random/1RM.xlsx", TRAINING_LOG)?,
        ];
        landing.append(run, records).await?;
        Ok((pool, directory))
    }

    async fn landed_with_composition() -> Result<(SqlitePool, tempfile::TempDir), Failure> {
        let directory = tempfile::tempdir()?;
        let pool = connect(&directory.path().join("test.db")).await?;

        let landing = SpreadsheetFileLandingStore::new(pool.clone())?;
        let runs = SqliteExtractionRunLog::new(pool.clone());
        let run = runs.begin(landing.stream(), FetchedAt::EPOCH).await?;
        let records = vec![landing_record(
            "Dropbox/Random/Body Weight 2018.xlsx",
            COMPOSITION,
        )?];
        landing.append(run, records).await?;
        Ok((pool, directory))
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
        Ok(normalisation
            .normalise(application::Rederivation::Whole)
            .await?)
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
    fn a_re_derivation_replaces_its_own_weigh_ins_and_no_other_sources() {
        let outcome: Result<(), Failure> = block_on(async {
            let (pool, _directory) = landed().await?;

            let first = derive(&pool).await?;
            assert_eq!(first.records_read.as_usize(), 2);
            assert_eq!(first.workouts_written.as_usize(), 5);
            assert_eq!(first.records_refused.as_usize(), 1, "the training log");
            assert!(first.reconciles());

            // A Body Scan weigh-in, as the Withings derivation writes one.
            sqlx::query(
                "INSERT INTO weigh_in (stream, measured_at_utc, zone, mass_grams, run_id)
                 SELECT 'withings.measurements', '2026-09-17T06:12:05Z', 'Europe/London',
                        84619, MAX(id)
                 FROM normalisation_run",
            )
            .execute(&pool)
            .await?;
            let canonical = SqliteCanonicalWeighInStore::new(pool.clone());
            let normalised = SqliteNormalisedWeighInReader::new(pool.clone());
            canonicalise::weigh_ins(&normalised, &canonical).await?;

            // **The canonical rows name the ones this deletes**, so a
            // re-derivation that did not clear them first would fail here.
            let second = derive(&pool).await?;
            assert_eq!(second.workouts_written.as_usize(), 5);
            assert_eq!(
                count(&pool, "SELECT COUNT(*) FROM manual_weigh_in").await?,
                5
            );
            assert_eq!(
                count(
                    &pool,
                    "SELECT COUNT(*) FROM weigh_in WHERE stream = 'spreadsheets.files'"
                )
                .await?,
                5,
                "the first derivation's rows were replaced, not added to"
            );
            assert_eq!(
                count(
                    &pool,
                    "SELECT COUNT(*) FROM weigh_in WHERE stream = 'withings.measurements'"
                )
                .await?,
                1,
                "another source's weigh-in survives"
            );

            assert_eq!(
                count(
                    &pool,
                    "SELECT COUNT(*) FROM manual_weigh_in
                     WHERE last_saved_by = 'document'
                       AND last_saved_at_utc = '2026-09-27T16:09:22Z'"
                )
                .await?,
                5,
                "every copy says when its workbook was last saved, by the workbook's own dcterms:modified"
            );

            let done = canonicalise::weigh_ins(&normalised, &canonical).await?;
            assert_eq!(done.read.as_usize(), 6);
            assert!(done.left_out.is_empty());
            let history = canonical.weigh_ins().await?;
            assert_eq!(
                history.len(),
                6,
                "relative strength reads every weighing, the spreadsheets' included"
            );
            Ok(())
        })
        .and_then(|result| result);
        outcome.expect("the derivation round-trips");
    }

    #[test]
    fn composition_is_written_against_its_weigh_in_and_replaced_with_it() {
        let outcome: Result<(), Failure> = block_on(async {
            let (pool, _directory) = landed_with_composition().await?;

            derive(&pool).await?;
            let second = derive(&pool).await?;
            assert_eq!(second.workouts_written.as_usize(), 6);
            assert_eq!(
                count(&pool, "SELECT COUNT(*) FROM body_composition").await?,
                7,
                "the first derivation's rows were replaced, not added to"
            );
            assert_eq!(
                count(&pool, "SELECT COUNT(*) FROM manual_composition").await?,
                7,
                "every figure names the cell it was typed into"
            );

            let fat: (i64, String, i64) = sqlx::query_as(
                "SELECT c.percentage_tenths, m.cell, w.mass_grams
                 FROM body_composition c
                 JOIN manual_composition m USING (weigh_in, component)
                 JOIN weigh_in w ON w.id = c.weigh_in
                 WHERE w.on_day = '2018-01-01' AND c.component = 'fat'",
            )
            .fetch_one(&pool)
            .await?;
            assert_eq!(fat, (247, "E2".to_owned(), 73_900));

            let measured = SqliteCompositionHistory::new(pool.clone())
                .measured()
                .await?;
            assert_eq!(measured.len(), 7, "every figure reads back");
            assert!(
                measured
                    .iter()
                    .all(|one| one.method == Method::Spreadsheets)
            );
            Ok(())
        })
        .and_then(|result| result);
        outcome.expect("the derivation round-trips");
    }
}
