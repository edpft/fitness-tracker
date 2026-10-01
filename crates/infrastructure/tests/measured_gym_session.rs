//! Garmin's activities into [`MeasuredGymSession`] (#172): the heart rate and
//! the sets as separate parts, and the operator's rulings on which of the
//! exercise data is his.
//!
//! Every payload here is invented; the shapes are the ones his 548 landed gym
//! activities hold — the bench ramp of 2019-03-14, the `UNKNOWN` classifier
//! output of a week earlier, the one unloaded set a watch emits while Hevy is
//! holding the real record, and the Peloton class a sync pushed in.

use application::{
    ExtractionRunLog as _, Translation, WorkoutNormaliser as _,
    normalise::{Normalisation, NormalisationPorts},
    ports::{SourceAccount, Translator},
};
use domain::{
    gym::{Guess, GuessedExercise, Load, MeasuredGymSession, RepsExercise},
    landing::{
        Endpoint, EventKind, EventProvenance, FetchedAt, LandedRecord, LandingRecord,
        LandingRecordId, LandingStream, RawPayload, SourceRecordId,
    },
    normalised::{OperatorZone, RefusalLocus, RefusalReason},
};
use infrastructure::{
    GarminActivityFileLandingStore, GarminActivityLandingStore, GarminExerciseSetLandingStore,
    GarminGymAccountReader, SqliteExtractionRunLog, SqliteMeasuredGymSessionStore,
    SqliteNormalisationRunLog, SqliteRefusalStore, connect,
    garmin::{GarminGymTranslator, account::activities},
};
use serde_json::{Value, json};
use sqlx::SqlitePool;

type Failure = Box<dyn std::error::Error>;

fn record(
    id: i64,
    activity: &str,
    stream: &str,
    path: &str,
    payload: &Value,
) -> Result<LandedRecord, Failure> {
    let provenance = EventProvenance::new(
        Endpoint::try_from(path)?,
        EventKind::try_from("updated")?,
        None,
    );
    let landed = LandingRecord::land(
        LandingStream::try_from(stream)?,
        FetchedAt::try_from("2026-09-18T08:00:00Z")?,
        SourceRecordId::try_from(activity)?,
        provenance.into(),
        RawPayload::try_from(serde_json::to_vec(payload)?)?,
    );
    Ok(LandedRecord::new(LandingRecordId::try_from(id)?, landed))
}

fn activity(id: &str, kind: &str, started: &str, overrides: &[(&str, Value)]) -> Value {
    let mut payload = json!({
        "activityId": id.parse::<i64>().unwrap_or_default(),
        "activityName": "Strength",
        "activityType": { "typeId": 13, "typeKey": kind },
        "startTimeGMT": started,
        "startTimeLocal": started,
        "duration": 2_709.291,
        "averageHR": 77.0,
        "maxHR": 133.0,
        "deviceId": 3_960_966_298_i64,
        "manufacturer": Value::Null,
        "isManualActivity": false,
    });
    if let Some(object) = payload.as_object_mut() {
        for (field, value) in overrides {
            object.insert((*field).to_owned(), value.clone());
        }
    }
    payload
}

/// One set as the exerciseSets endpoint serves it. `movement` is the
/// classifier's top candidate: its category, and the name where it narrowed to
/// one.
fn set(started: &str, reps: u32, grams: Option<f64>, movement: (&str, Option<&str>)) -> Value {
    json!({
        "exercises": [
            { "category": movement.0, "name": movement.1, "probability": 99.609_375 },
            { "category": "CURL", "name": Value::Null, "probability": 0.0 },
        ],
        "duration": 30_000.0,
        "repetitionCount": reps,
        "weight": grams,
        "setType": "ACTIVE",
        "startTime": started,
        "messageIndex": 0,
    })
}

fn rest(started: &str) -> Value {
    json!({
        "exercises": [],
        "duration": 60_000.0,
        "repetitionCount": 0,
        "weight": 0.0,
        "setType": "REST",
        "startTime": started,
    })
}

fn exercise_sets(id: &str, sets: &[Value]) -> Value {
    json!({ "activityId": id.parse::<i64>().unwrap_or_default(), "exerciseSets": sets })
}

fn translate(
    activity: &Value,
    sets: Option<&Value>,
) -> Result<Translation<MeasuredGymSession>, Failure> {
    translate_with(activity, sets, None)
}

/// As [`translate`], with the file the watch wrote where one landed.
fn translate_with(
    activity: &Value,
    sets: Option<&Value>,
    recording: Option<&[u8]>,
) -> Result<Translation<MeasuredGymSession>, Failure> {
    let id = activity
        .get("activityId")
        .and_then(Value::as_i64)
        .ok_or("an activity id")?
        .to_string();
    let landed = vec![record(
        1,
        &id,
        "garmin.activities",
        "/activitylist-service/activities/search/activities",
        activity,
    )?];
    let landed_sets = match sets {
        Some(sets) => vec![record(
            2,
            &id,
            "garmin.exercise_sets",
            "/activity-service/activity/exerciseSets",
            sets,
        )?],
        None => Vec::new(),
    };

    let landed_files = match recording {
        Some(archive) => vec![landed_recording(3, &id, archive)?],
        None => Vec::new(),
    };

    let accounts = activities(landed, landed_sets, landed_files);
    let [account] = accounts.as_slice() else {
        return Err(format!("{} accounts, not one", accounts.len()).into());
    };
    Ok(GarminGymTranslator.translate(account, &OperatorZone::try_from("Europe/London")?)?)
}

fn session_for(activity: &Value, sets: Option<&Value>) -> Result<MeasuredGymSession, Failure> {
    match translate(activity, sets)? {
        Translation::Entity { entity, .. } => Ok(*entity),
        other => Err(format!("no session: {other:?}").into()),
    }
}

fn refusals(activity: &Value, sets: Option<&Value>) -> Result<Vec<RefusalReason>, Failure> {
    Ok(match translate(activity, sets)? {
        Translation::Entity { refusals, .. } => {
            refusals.into_iter().map(|refusal| refusal.reason).collect()
        }
        Translation::Refused(refusals) => refusals
            .iter()
            .map(|refusal| refusal.reason.clone())
            .collect(),
        other => return Err(format!("no refusals: {other:?}").into()),
    })
}

/// One landed activity file: the archive, as `garmin.activity_files` holds it.
fn landed_recording(id: i64, activity: &str, archive: &[u8]) -> Result<LandedRecord, Failure> {
    let provenance = EventProvenance::new(
        Endpoint::try_from("/download-service/files/activity")?,
        EventKind::try_from("updated")?,
        None,
    );
    let landed = LandingRecord::land(
        LandingStream::try_from("garmin.activity_files")?,
        FetchedAt::try_from("2026-09-18T08:00:00Z")?,
        SourceRecordId::try_from(activity)?,
        provenance.into(),
        RawPayload::try_from(archive.to_vec())?,
    );
    Ok(LandedRecord::new(LandingRecordId::try_from(id)?, landed))
}

/// A FIT recording, as a watch writes one: a `session` stating when it began,
/// then a `record` per moment carrying a heart rate.
///
/// `readings` are FIT timestamps — seconds from 1989-12-31 — each with the
/// reading the watch wrote at that moment, or [`None`] where the field says it
/// measured nothing.
fn recording(session_start: u32, readings: &[(u32, Option<u8>)]) -> Vec<u8> {
    let mut messages = Vec::new();

    // Local type 0 defines `session`: its own timestamp, then `start_time`.
    messages.extend_from_slice(&[0x40, 0x00, 0x00, 18, 0, 2]);
    messages.extend_from_slice(&[253, 4, 0x86, 2, 4, 0x86]);
    messages.push(0x00);
    messages.extend_from_slice(&session_start.to_le_bytes());
    messages.extend_from_slice(&session_start.to_le_bytes());

    // Local type 1 defines `record`: a timestamp, a heart rate, and a field
    // this build does not read, which it must still step over.
    messages.extend_from_slice(&[0x41, 0x00, 0x00, 20, 0, 3]);
    messages.extend_from_slice(&[253, 4, 0x86, 3, 1, 0x02, 13, 1, 0x01]);
    for (at, beats) in readings {
        messages.push(0x01);
        messages.extend_from_slice(&at.to_le_bytes());
        messages.push(beats.unwrap_or(0xFF));
        messages.push(30);
    }

    let mut file = vec![12, 0x10, 0x00, 0x00];
    file.extend_from_slice(
        &u32::try_from(messages.len())
            .unwrap_or_default()
            .to_le_bytes(),
    );
    file.extend_from_slice(b".FIT");
    file.extend_from_slice(&messages);
    // The trailing CRC, which nothing here reads.
    file.extend_from_slice(&[0x00, 0x00]);
    file
}

/// The archive Garmin serves a recording in: one deflated entry, named for its
/// activity.
fn archive(recording: &[u8]) -> Result<Vec<u8>, Failure> {
    use std::io::Write as _;

    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer.start_file("3461073244_ACTIVITY.fit", options)?;
    writer.write_all(recording)?;
    Ok(writer.finish()?.into_inner())
}

/// A clock that never moves, so a run log is comparable between derivations.
#[derive(Debug, Clone, Copy)]
struct FixedClock;

impl application::Clock for FixedClock {
    fn now(&self) -> FetchedAt {
        FetchedAt::EPOCH
    }
}

/// A runtime built by hand, because `#[tokio::test]` generates an
/// `#[allow(clippy::unwrap_used)]` that `forbid` refuses to compile.
fn runtime() -> Result<tokio::runtime::Runtime, Failure> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}

/// A store holding one activity, its sets and its recording, in a temp file.
async fn landed(
    activity: &Value,
    sets: &Value,
    recording: &[u8],
) -> Result<(SqlitePool, tempfile::TempDir), Failure> {
    let id = activity
        .get("activityId")
        .and_then(Value::as_i64)
        .ok_or("an activity id")?
        .to_string();
    let landed_activity = record(
        1,
        &id,
        "garmin.activities",
        "/activitylist-service/activities/search/activities",
        activity,
    )?;
    let landed_sets = record(
        2,
        &id,
        "garmin.exercise_sets",
        "/activity-service/activity/exerciseSets",
        sets,
    )?;
    let landed_file = landed_recording(3, &id, recording)?;

    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;
    let runs = SqliteExtractionRunLog::new(pool.clone());

    append(
        &GarminActivityLandingStore::new(pool.clone())?,
        &runs,
        landed_activity,
    )
    .await?;
    append(
        &GarminExerciseSetLandingStore::new(pool.clone())?,
        &runs,
        landed_sets,
    )
    .await?;
    append(
        &GarminActivityFileLandingStore::new(pool.clone())?,
        &runs,
        landed_file,
    )
    .await?;

    Ok((pool, directory))
}

/// One record into its own landing table, under a run of its own.
async fn append<S: application::LandingStore + Sync>(
    store: &S,
    runs: &SqliteExtractionRunLog,
    landed: LandedRecord,
) -> Result<(), Failure> {
    let run = runs.begin(store.stream(), FetchedAt::EPOCH).await?;
    store.append(run, vec![landed.record().clone()]).await?;
    Ok(())
}

async fn derive(pool: &SqlitePool) -> Result<application::NormalisationSummary, Failure> {
    let normalisation = Normalisation::new(
        NormalisationPorts {
            raw: GarminGymAccountReader::new(pool.clone())?,
            translator: GarminGymTranslator,
            workouts: SqliteMeasuredGymSessionStore::new(pool.clone())?,
            refusals: SqliteRefusalStore::new(pool.clone(), "garmin.activities")?,
            runs: SqliteNormalisationRunLog::new(pool.clone()),
            clock: FixedClock,
        },
        OperatorZone::try_from("Europe/London")?,
    );
    Ok(normalisation.normalise().await?)
}

/// A whole session, end to end: the summary the list states on the session's own
/// row, and a row per reading the watch wrote.
#[test]
fn the_readings_reach_the_store() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let id = "3461073244";
        let activity = activity(id, "strength_training", "2019-03-14 07:43:22", &[]);
        let sets = exercise_sets(
            id,
            &[set(
                "2019-03-14T07:43:22.0",
                5,
                Some(48_000.0),
                ("BENCH_PRESS", None),
            )],
        );
        let file = archive(&recording(
            920_000_000,
            &[
                (920_000_000, Some(96)),
                (920_000_002, Some(0)),
                (920_000_009, Some(133)),
            ],
        ))
        .expect("an archive");

        let (pool, _directory) = landed(&activity, &sets, &file).await.expect("a corpus");
        let summary = derive(&pool).await.expect("a derivation");
        assert_eq!(
            summary.records_read.as_usize(),
            3,
            "the activity, its sets and its recording",
        );
        assert_eq!(summary.workouts_written.as_usize(), 1);
        assert!(summary.reconciles(), "every record has exactly one outcome");

        let session = sqlx::query!(
            r#"SELECT average_bpm AS "average!: i64", highest_bpm AS "highest!: i64",
                      recording_landing_record_id AS "recording!: i64",
                      (SELECT id FROM garmin_activity_file_landing) AS "landed!: i64"
               FROM measured_gym_session"#
        )
        .fetch_one(&pool)
        .await
        .expect("a measured session");
        assert_eq!((session.average, session.highest), (77, 133));
        assert_eq!(
            session.recording, session.landed,
            "the row names the record the readings came from",
        );

        let readings = sqlx::query!(
            r#"SELECT at_seconds AS "at!: i64", beats_per_minute AS "beats!: i64"
               FROM measured_gym_session_heart_rate ORDER BY at_seconds"#
        )
        .fetch_all(&pool)
        .await
        .expect("its readings");
        assert_eq!(
            readings
                .iter()
                .map(|row| (row.at, row.beats))
                .collect::<Vec<(i64, i64)>>(),
            vec![(0, 96), (9, 133)],
            "the gap is a gap, and a zero is not a reading",
        );

        // A second derivation replaces the stream's rows rather than adding to
        // them: the readings hang off a workout that is deleted and rebuilt.
        derive(&pool).await.expect("a second derivation");
        let again = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!: i64" FROM measured_gym_session_heart_rate"#
        )
        .fetch_one(&pool)
        .await
        .expect("a count");
        assert_eq!(again, 2);
    });
}

#[test]
fn a_session_holds_its_heart_rate_and_its_sets_as_separate_parts() {
    // 2019-03-14: a bench ramp to a single at 70, then the accessory work.
    let id = "3461073244";
    let activity = activity(id, "strength_training", "2019-03-14 07:43:22", &[]);
    let sets = exercise_sets(
        id,
        &[
            set(
                "2019-03-14T07:43:22.0",
                5,
                Some(48_000.0),
                ("BENCH_PRESS", None),
            ),
            rest("2019-03-14T07:43:51.0"),
            set(
                "2019-03-14T07:50:58.0",
                1,
                Some(70_000.0),
                ("BENCH_PRESS", None),
            ),
            set(
                "2019-03-14T08:12:21.0",
                10,
                Some(32_000.0),
                ("ROW", Some("FACE_PULL")),
            ),
        ],
    );

    let session = session_for(&activity, Some(&sets)).expect("a session");
    let recorded = session.recorded();

    let heart_rate = recorded.heart_rate().expect("a heart rate");
    assert_eq!(heart_rate.stated().average().as_u32(), 77);
    assert_eq!(heart_rate.stated().highest().as_u32(), 133);
    assert!(
        heart_rate.series().is_none(),
        "no recording landed, so there are no readings",
    );

    let sets = recorded.sets().expect("sets");
    assert_eq!(sets.count(), 3, "the rest between them is not a set");
    assert_eq!(
        sets.first().load,
        Some(Load::Absolute(domain::measure::Kg::from_grams(48_000))),
        "grams as Garmin serves them, kilograms as we hold them",
    );
    assert_eq!(session.duration().as_seconds(), 2_709);
    assert_eq!(session.started_at().zone().id(), "Europe/London");
}

#[test]
fn the_exercise_is_the_watchs_guess_and_says_so() {
    let id = "3461073244";
    let activity = activity(id, "strength_training", "2019-03-14 07:43:22", &[]);
    let sets = exercise_sets(
        id,
        &[
            set(
                "2019-03-14T08:19:43.0",
                10,
                Some(45_000.0),
                ("SQUAT", Some("BARBELL_FRONT_SQUAT")),
            ),
            set(
                "2019-03-14T08:22:55.0",
                10,
                Some(45_000.0),
                ("UNKNOWN", None),
            ),
        ],
    );

    let session = session_for(&activity, Some(&sets)).expect("a session");
    let sets = session.recorded().sets().expect("sets");
    let mut sets = sets.iter();

    let named = sets.next().expect("the named set");
    assert!(
        named.guess.from_the_source(),
        "the classifier proposed this one itself",
    );
    assert_eq!(
        named.guess.exercise().map(RepsExercise::as_str),
        Some("front-squat-barbell"),
    );

    // The operator, 2026-09-28: an unknown between named sets at the same reps
    // and load is the same exercise.
    let carried = sets.next().expect("the unclassified set");
    assert_eq!(
        carried.guess,
        GuessedExercise::FromItsRun(Guess::Exercise(RepsExercise::FrontSquatBarbell)),
    );
    assert!(
        !carried.guess.from_the_source(),
        "carried from its run, which is not the source saying it",
    );
}

#[test]
fn a_run_naming_two_movements_names_none_of_the_sets_between_them() {
    let id = "3461073244";
    let activity = activity(id, "strength_training", "2019-03-14 07:43:22", &[]);
    let sets = exercise_sets(
        id,
        &[
            set(
                "2019-03-14T08:00:00.0",
                10,
                Some(70_000.0),
                ("DEADLIFT", Some("BARBELL_DEADLIFT")),
            ),
            set(
                "2019-03-14T08:03:00.0",
                10,
                Some(70_000.0),
                ("UNKNOWN", None),
            ),
            set(
                "2019-03-14T08:06:00.0",
                10,
                Some(70_000.0),
                ("BENCH_PRESS", Some("BARBELL_BENCH_PRESS")),
            ),
        ],
    );

    let session = session_for(&activity, Some(&sets)).expect("a session");
    let sets = session.recorded().sets().expect("sets");
    let middle = sets.iter().nth(1).expect("the middle set");
    assert_eq!(
        middle.guess,
        GuessedExercise::Undetermined,
        "two movements in one run is not an answer to pick between",
    );
}

#[test]
fn a_ramp_is_not_a_run_and_carries_nothing_along_it() {
    let id = "3461073244";
    let activity = activity(id, "strength_training", "2019-03-14 07:43:22", &[]);
    let sets = exercise_sets(
        id,
        &[
            set(
                "2019-03-14T07:43:22.0",
                5,
                Some(48_000.0),
                ("BENCH_PRESS", Some("BARBELL_BENCH_PRESS")),
            ),
            set(
                "2019-03-14T07:45:11.0",
                5,
                Some(53_000.0),
                ("UNKNOWN", None),
            ),
        ],
    );

    let session = session_for(&activity, Some(&sets)).expect("a session");
    let sets = session.recorded().sets().expect("sets");
    assert_eq!(
        sets.iter().nth(1).expect("the second set").guess,
        GuessedExercise::Undetermined,
        "a different load is a different exercise as far as this rule goes",
    );
}

#[test]
fn a_session_with_no_sets_is_still_a_session() {
    // Every gym activity from 2015 and 2016 is one of these: a heart rate and
    // nothing else.
    let activity = activity(
        "1051901298",
        "strength_training",
        "2015-02-23 18:02:11",
        &[],
    );
    let session = session_for(&activity, None).expect("a session");

    assert!(session.recorded().heart_rate().is_some());
    assert!(session.recorded().sets().is_none());
    assert_eq!(session.set_count(), 0);
    assert!(session.landed_as().sets.is_none());
}

#[test]
fn a_session_with_no_heart_rate_is_still_a_session() {
    let id = "3461073244";
    let activity = activity(
        id,
        "strength_training",
        "2019-03-14 07:43:22",
        &[("averageHR", Value::Null), ("maxHR", Value::Null)],
    );
    let sets = exercise_sets(
        id,
        &[set(
            "2019-03-14T07:43:22.0",
            5,
            Some(48_000.0),
            ("BENCH_PRESS", None),
        )],
    );

    let session = session_for(&activity, Some(&sets)).expect("a session");
    assert!(session.recorded().heart_rate().is_none());
    assert_eq!(session.set_count(), 1);
}

#[test]
fn a_heart_rate_of_zero_is_a_sensor_saying_nothing() {
    let id = "3461073244";
    let activity = activity(
        id,
        "strength_training",
        "2019-03-14 07:43:22",
        &[("averageHR", json!(0.0)), ("maxHR", json!(0.0))],
    );
    let sets = exercise_sets(
        id,
        &[set(
            "2019-03-14T07:43:22.0",
            5,
            Some(48_000.0),
            ("BENCH_PRESS", None),
        )],
    );

    let session = session_for(&activity, Some(&sets)).expect("a session");
    assert!(session.recorded().heart_rate().is_none());
}

#[test]
fn one_unloaded_set_is_the_watch_classifying_on_its_own() {
    // 124 of the operator's gym activities are this shape, 115 of them on days
    // Hevy holds the session.
    let id = "20604812731";
    let activity = activity(id, "strength_training", "2025-06-30 18:12:43", &[]);
    let sets = exercise_sets(
        id,
        &[set("2025-06-30T18:31:04.0", 4, None, ("UNKNOWN", None))],
    );

    let session = session_for(&activity, Some(&sets)).expect("a session");
    assert!(
        session.recorded().sets().is_none(),
        "the set goes and the heart rate stays",
    );
    assert!(session.recorded().heart_rate().is_some());

    let refused = refusals(&activity, Some(&sets)).expect("refusals");
    assert!(refused.contains(&RefusalReason::OnlyTheWatchClassifying));
}

#[test]
fn a_single_set_carrying_a_load_is_a_set_he_recorded() {
    let id = "20604812731";
    let activity = activity(id, "strength_training", "2025-06-30 18:12:43", &[]);
    let sets = exercise_sets(
        id,
        &[set(
            "2025-06-30T18:31:04.0",
            4,
            Some(60_000.0),
            ("UNKNOWN", None),
        )],
    );

    let session = session_for(&activity, Some(&sets)).expect("a session");
    assert_eq!(session.set_count(), 1);
}

#[test]
fn a_peloton_class_pushed_in_by_a_sync_is_not_a_gym_session() {
    for (device, manufacturer) in [(1, "GARMIN"), (0, "PELOTON")] {
        let activity = activity(
            "19938182504",
            "strength_training",
            "2026-07-22 17:45:00",
            &[
                ("deviceId", json!(device)),
                ("manufacturer", json!(manufacturer)),
                (
                    "activityName",
                    json!("5 min Post-Ride Stretch with Ben Alldis"),
                ),
            ],
        );

        let refused = refusals(&activity, None).expect("refusals");
        assert!(
            matches!(refused.as_slice(), [RefusalReason::NotTheInstrument { .. }]),
            "{manufacturer} device {device}: {refused:?}",
        );
    }
}

#[test]
fn an_activity_that_is_not_a_gym_session_is_refused_rather_than_dropped() {
    let activity = activity("3460990594", "cycling", "2019-03-14 07:04:09", &[]);
    let refused = refusals(&activity, None).expect("refusals");

    assert!(
        matches!(
            refused.as_slice(),
            [RefusalReason::Unmodelled { detail }] if detail == "cycling",
        ),
        "{refused:?}",
    );
}

#[test]
fn a_movement_the_vocabulary_has_no_member_for_keeps_its_set_and_names_itself() {
    let id = "3461073244";
    let activity = activity(id, "strength_training", "2019-03-14 07:43:22", &[]);
    let sets = exercise_sets(
        id,
        &[set(
            "2019-03-14T08:00:00.0",
            10,
            Some(20_000.0),
            ("PLANK", Some("SIDE_PLANK_WITH_HIP_DIP")),
        )],
    );

    let session = session_for(&activity, Some(&sets)).expect("a session");
    let measured = session.recorded().sets().expect("sets");
    assert_eq!(
        measured.count(),
        1,
        "the set keeps its clock, reps and load"
    );
    assert_eq!(measured.first().guess, GuessedExercise::Undetermined);

    let refused = refusals(&activity, Some(&sets)).expect("refusals");
    assert!(
        refused.iter().any(|reason| matches!(
            reason,
            RefusalReason::UnguessableMovement { term }
                if term == "PLANK/SIDE_PLANK_WITH_HIP_DIP"
        )),
        "{refused:?}",
    );
}

/// One set of a movement the vocabulary has no member for.
fn unplaceable_sets(id: &str) -> Value {
    exercise_sets(
        id,
        &[set(
            "2019-03-14T08:00:00.0",
            10,
            Some(20_000.0),
            ("PLANK", Some("SIDE_PLANK_WITH_HIP_DIP")),
        )],
    )
}

#[test]
fn a_set_the_watch_counted_no_reps_for_is_refused_and_the_rest_stand() {
    let id = "3461073244";
    let activity = activity(id, "strength_training", "2019-03-14 07:43:22", &[]);
    let sets = exercise_sets(
        id,
        &[
            set(
                "2019-03-14T07:43:22.0",
                0,
                Some(48_000.0),
                ("BENCH_PRESS", None),
            ),
            set(
                "2019-03-14T07:45:11.0",
                5,
                Some(53_000.0),
                ("BENCH_PRESS", None),
            ),
        ],
    );

    let session = session_for(&activity, Some(&sets)).expect("a session");
    assert_eq!(session.set_count(), 1);

    let refused = refusals(&activity, Some(&sets)).expect("refusals");
    assert!(
        refused
            .iter()
            .any(|reason| matches!(reason, RefusalReason::UnreadableValue { field, .. } if *field == "repetitionCount")),
        "{refused:?}",
    );
}

#[test]
fn a_refusal_below_a_session_names_the_set_and_no_exercise_above_it() {
    let id = "3461073244";
    let activity = activity(id, "strength_training", "2019-03-14 07:43:22", &[]);
    let sets = exercise_sets(
        id,
        &[
            set(
                "2019-03-14T07:43:22.0",
                5,
                Some(48_000.0),
                ("BENCH_PRESS", None),
            ),
            set(
                "2019-03-14T07:45:11.0",
                0,
                Some(53_000.0),
                ("BENCH_PRESS", None),
            ),
        ],
    );

    let Translation::Entity { refusals, .. } =
        translate(&activity, Some(&sets)).expect("a session")
    else {
        panic!("no session");
    };
    let [refusal] = refusals.as_slice() else {
        panic!("one refusal, not {}", refusals.len());
    };
    assert_eq!(refusal.locus, RefusalLocus::Ungrouped { set: 1 });
    assert_eq!(refusal.locus.to_string(), "set 1");
}

#[test]
fn a_session_holds_the_readings_the_watch_wrote() {
    // The operator, 2026-09-18: "heart rate for the canonical session needs
    // per-second samples, not averageHR/maxHR". The stated summary stays
    // beside them.
    //
    // The spacing is his: the watch writes on its own judgement, and his
    // 2026-09-25 session holds 2,785 readings across 5,935 seconds.
    let id = "3461073244";
    let activity = activity(id, "strength_training", "2019-03-14 07:43:22", &[]);
    let file = archive(&recording(
        920_000_000,
        &[
            (920_000_000, Some(96)),
            (920_000_001, Some(98)),
            // Nothing measured, and a strap that said nothing: neither is a
            // reading, and neither becomes a row.
            (920_000_003, None),
            (920_000_004, Some(0)),
            (920_000_010, Some(133)),
        ],
    ))
    .expect("an archive");

    let Translation::Entity { entity, refusals } =
        translate_with(&activity, None, Some(&file)).expect("a translation")
    else {
        panic!("no session");
    };
    assert!(
        refusals.is_empty(),
        "a recording that reads refuses nothing"
    );

    let heart_rate = entity.recorded().heart_rate().expect("a heart rate");
    assert_eq!(
        heart_rate.stated().average().as_u32(),
        77,
        "what the list said is what the list said",
    );
    let series = heart_rate.series().expect("its readings");
    let readings: Vec<(u64, u32)> = series
        .samples()
        .iter()
        .map(|reading| (reading.at.as_seconds(), reading.beats_per_minute.as_u32()))
        .collect();
    assert_eq!(readings, vec![(0, 96), (1, 98), (10, 133)]);
    assert_eq!(entity.reading_count(), 3);
}

#[test]
fn a_recording_whose_clock_is_wrong_still_places_its_readings() {
    // 16 of the operator's 2015 and 2016 sessions were recorded by a 310XT with
    // no time fix, and their files are stamped 2007-04-01 — nine years before
    // the session the activity list dates. The offsets between the readings are
    // sound, which is what a series is, so they are counted from the file's own
    // start and the session keeps the list's clock.
    let id = "3461073244";
    let activity = activity(id, "strength_training", "2016-07-11 08:00:00", &[]);
    let stamped_2007 = 544_147_200;
    let file = archive(&recording(
        stamped_2007,
        &[(stamped_2007, Some(88)), (stamped_2007 + 4, Some(91))],
    ))
    .expect("an archive");

    let session = match translate_with(&activity, None, Some(&file)).expect("a translation") {
        Translation::Entity { entity, .. } => *entity,
        other => panic!("no session: {other:?}"),
    };

    let series = session
        .recorded()
        .heart_rate()
        .and_then(|heart_rate| heart_rate.series())
        .expect("its readings");
    assert_eq!(
        series
            .samples()
            .iter()
            .map(|reading| reading.at.as_seconds())
            .collect::<Vec<u64>>(),
        vec![0, 4],
    );
    assert_eq!(
        session.started_at().instant().to_string(),
        "2016-07-11T08:00:00Z",
        "the session's clock is the activity list's",
    );
}

#[test]
fn a_recording_that_will_not_read_costs_the_series_and_not_the_summary() {
    let id = "3461073244";
    let activity = activity(id, "strength_training", "2019-03-14 07:43:22", &[]);

    let Translation::Entity { entity, refusals } =
        translate_with(&activity, None, Some(b"PK\x03\x04 and then nonsense"))
            .expect("a translation")
    else {
        panic!("no session");
    };

    let heart_rate = entity.recorded().heart_rate().expect("a heart rate");
    assert_eq!(heart_rate.stated().highest().as_u32(), 133);
    assert!(heart_rate.series().is_none());

    let [refusal] = refusals.as_slice() else {
        panic!("one refusal, not {}", refusals.len());
    };
    assert_eq!(refusal.locus, RefusalLocus::Record);
    assert!(
        matches!(
            &refusal.reason,
            RefusalReason::UnreadablePayload { detail } if detail.starts_with("its recording:")
        ),
        "{:?}",
        refusal.reason,
    );
}

#[test]
fn a_recording_holding_no_reading_is_a_session_with_none() {
    // 20 of the operator's 551 strength activities, every one of which states no
    // summary either: the watch recorded a clock and nothing else.
    let id = "3461073244";
    let activity = activity(
        id,
        "strength_training",
        "2019-03-14 07:43:22",
        &[("averageHR", json!(0.0)), ("maxHR", json!(0.0))],
    );
    let sets = exercise_sets(
        id,
        &[
            set(
                "2019-03-14T07:43:22.0",
                5,
                Some(48_000.0),
                ("BENCH_PRESS", None),
            ),
            set(
                "2019-03-14T07:50:58.0",
                5,
                Some(48_000.0),
                ("BENCH_PRESS", None),
            ),
        ],
    );
    let file = archive(&recording(920_000_000, &[(920_000_000, None)])).expect("an archive");

    let session = match translate_with(&activity, Some(&sets), Some(&file)).expect("a translation")
    {
        Translation::Entity { entity, .. } => *entity,
        other => panic!("no session: {other:?}"),
    };
    assert!(session.recorded().heart_rate().is_none());
    assert_eq!(session.reading_count(), 0);
    assert_eq!(session.set_count(), 2);
}

#[test]
fn an_account_composes_the_activity_and_its_sets() {
    let id = "3461073244";
    let landed = vec![
        record(
            1,
            id,
            "garmin.activities",
            "/activitylist-service/activities/search/activities",
            &activity(id, "strength_training", "2019-03-14 07:43:22", &[]),
        )
        .expect("an activity"),
    ];
    let sets = vec![
        record(
            2,
            id,
            "garmin.exercise_sets",
            "/activity-service/activity/exerciseSets",
            &unplaceable_sets(id),
        )
        .expect("its sets"),
    ];

    let recording = vec![
        landed_recording(
            3,
            id,
            &archive(&recording(100, &[(100, Some(96))])).expect("an archive"),
        )
        .expect("its file"),
    ];

    let accounts = activities(landed, sets, recording);
    let [account] = accounts.as_slice() else {
        panic!("one account, not {}", accounts.len());
    };
    assert_eq!(account.records(), 3, "three responses about one session");
    assert_eq!(account.superseded(), 0);
    assert!(account.sets().is_some());
    assert!(account.recording().is_some());
}

#[test]
fn a_later_serving_of_an_activity_supersedes_the_earlier() {
    let id = "3461073244";
    let landed = vec![
        record(
            1,
            id,
            "garmin.activities",
            "/activitylist-service/activities/search/activities",
            &activity(id, "strength_training", "2019-03-14 07:43:22", &[]),
        )
        .expect("an activity"),
        record(
            5,
            id,
            "garmin.activities",
            "/activitylist-service/activities/search/activities",
            &activity(
                id,
                "strength_training",
                "2019-03-14 07:43:22",
                &[("maxHR", json!(141.0))],
            ),
        )
        .expect("a later serving"),
    ];

    let accounts = activities(landed, Vec::new(), Vec::new());
    let [account] = accounts.as_slice() else {
        panic!("one account, not {}", accounts.len());
    };
    assert_eq!(account.superseded(), 1);
    assert_eq!(
        account.activity().id(),
        LandingRecordId::try_from(5).expect("an id")
    );
}

#[test]
fn a_withdrawn_activity_withdraws_its_session() {
    let id = "3461073244";
    let provenance = EventProvenance::new(
        Endpoint::try_from("/activitylist-service/activities/search/activities").expect("a path"),
        EventKind::try_from("deleted").expect("a kind"),
        None,
    );
    let landed = LandingRecord::land(
        LandingStream::try_from("garmin.activities").expect("a stream"),
        FetchedAt::try_from("2026-09-18T08:00:00Z").expect("a time"),
        SourceRecordId::try_from(id).expect("an id"),
        provenance.into(),
        RawPayload::try_from(
            serde_json::to_vec(&activity(
                id,
                "strength_training",
                "2019-03-14 07:43:22",
                &[],
            ))
            .expect("a payload"),
        )
        .expect("a payload"),
    );
    let accounts = activities(
        vec![LandedRecord::new(
            LandingRecordId::try_from(1).expect("an id"),
            landed,
        )],
        Vec::new(),
        Vec::new(),
    );
    let [account] = accounts.as_slice() else {
        panic!("one account, not {}", accounts.len());
    };

    let zone = OperatorZone::try_from("Europe/London").expect("a zone");
    match GarminGymTranslator
        .translate(account, &zone)
        .expect("a translation")
    {
        Translation::Retraction { of } => assert_eq!(of.as_str(), id),
        other => panic!("not a retraction: {other:?}"),
    }
}

#[test]
fn a_bare_category_is_the_exercise_it_names_unqualified() {
    // The operator, 2026-09-28: "I said you could use category when there was
    // only category." Where the movement has a conventional referent he has
    // stated, the category is that exercise: an unqualified squat is a barbell
    // back squat.
    let id = "3461073244";
    let activity = activity(id, "strength_training", "2019-03-14 07:43:22", &[]);
    let sets = exercise_sets(
        id,
        &[set(
            "2019-03-14T07:43:22.0",
            5,
            Some(60_000.0),
            ("SQUAT", None),
        )],
    );

    let session = session_for(&activity, Some(&sets)).expect("a session");
    let measured = session.recorded().sets().expect("sets");

    assert_eq!(
        measured.first().guess.exercise().map(RepsExercise::as_str),
        Some("back-squat-barbell"),
    );
    assert!(
        refusals(&activity, Some(&sets))
            .expect("refusals")
            .is_empty(),
    );
}

#[test]
fn a_bare_category_with_no_conventional_exercise_describes_the_movement() {
    // There is no `row` in the vocabulary and no default for a bare one, so
    // what the watch stated is the movement and the weight says the rest: the
    // operator's loaded bare rows are bimodal with one empty interval, 20 to
    // 28 kg. Before this the whole set was refused and its load, reps and
    // clock went with it.
    let id = "3461073245";
    let activity = activity(id, "strength_training", "2019-03-14 07:43:22", &[]);
    let sets = exercise_sets(
        id,
        &[
            set("2019-03-14T07:43:22.0", 10, Some(32_500.0), ("ROW", None)),
            set("2019-03-14T07:50:00.0", 10, Some(15_000.0), ("ROW", None)),
            set("2019-03-14T07:57:00.0", 12, None, ("CALF_RAISE", None)),
            set("2019-03-14T08:04:00.0", 15, None, ("LEG_RAISE", None)),
        ],
    );

    let session = session_for(&activity, Some(&sets)).expect("a session");
    let measured = session.recorded().sets().expect("sets");
    let described: Vec<String> = measured.iter().map(|set| set.guess.to_string()).collect();

    assert_eq!(
        described,
        vec![
            "row (barbell)?".to_owned(),
            "row (dumbbell)?".to_owned(),
            // Seated and standing are 81 sets against 78 and their loads are
            // indistinguishable, so no implement is recoverable and none is
            // invented.
            "calf-raise?".to_owned(),
            // None of the operator's carries a weight at all, and a leg raise
            // with nothing added is loaded by the lifter.
            "leg-raise (bodyweight)?".to_owned(),
        ],
    );
    assert!(
        refusals(&activity, Some(&sets))
            .expect("refusals")
            .is_empty(),
    );
}

/// A stated duration of zero is refused, and not as a missing figure.
///
/// The operator, 2026-09-30: *"A 0 duration activity isn't an activity, by
/// definition, nothing happened."* Garmin serves one — a yoga activity on
/// 2026-08-19 whose `duration` and `elapsedDuration` are both `0.0` — so the
/// case is the source's rather than hypothetical, and it reaches the gym path the
/// moment a strength activity is stopped the same way.
///
/// **The two refusals are kept apart** because what an operator does about them
/// differs. Silence is wrong data to fix at source; a stated zero is the truth
/// about a non-event and there is nothing to fix, which is why it is a declared
/// limitation.
#[test]
fn a_stated_zero_duration_is_nothing_happening_and_an_absent_one_is_not() {
    let zero = activity(
        "24041473622",
        "strength_training",
        "2026-08-19 20:34:44",
        &[("duration", json!(0.0)), ("elapsedDuration", json!(0.0))],
    );
    let refused = refusals(&zero, None).expect("refusals");
    assert_eq!(
        refused.as_slice(),
        [RefusalReason::NothingHappened],
        "a stated zero says nothing happened"
    );
    assert_eq!(
        RefusalReason::NothingHappened.kind(),
        domain::normalised::RefusalKind::DeclaredLimitation,
        "there is nothing to fix at either end"
    );

    let absent = activity(
        "24041473623",
        "strength_training",
        "2026-08-19 20:34:44",
        &[("duration", Value::Null)],
    );
    let refused = refusals(&absent, None).expect("refusals");
    assert_eq!(
        refused.as_slice(),
        [RefusalReason::MissingFigure {
            figure: "a duration"
        }],
        "silence is a missing figure, not a non-event"
    );
}
