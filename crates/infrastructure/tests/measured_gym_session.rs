//! Garmin's activities into [`MeasuredGymSession`] (#172): the heart rate and
//! the sets as separate parts, and the operator's rulings on which of the
//! exercise data is his.
//!
//! Every payload here is invented; the shapes are the ones his 548 landed gym
//! activities hold — the bench ramp of 2019-03-14, the `UNKNOWN` classifier
//! output of a week earlier, the one unloaded set a watch emits while Hevy is
//! holding the real record, and the Peloton class a sync pushed in.

use application::{Translation, ports::SourceAccount, ports::Translator};
use domain::{
    gym::{Guess, GuessedExercise, Load, MeasuredGymSession, RepsExercise},
    landing::{
        Endpoint, EventKind, EventProvenance, FetchedAt, LandedRecord, LandingRecord,
        LandingRecordId, LandingStream, RawPayload, SourceRecordId,
    },
    normalised::{OperatorZone, RefusalLocus, RefusalReason},
};
use infrastructure::garmin::{GarminGymTranslator, account::activities};
use serde_json::{Value, json};

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

    let accounts = activities(landed, landed_sets);
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
    assert_eq!(heart_rate.average().as_u32(), 77);
    assert_eq!(heart_rate.highest().as_u32(), 133);

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
    assert!(session.sets_landed_as().is_none());
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

    let accounts = activities(landed, sets);
    let [account] = accounts.as_slice() else {
        panic!("one account, not {}", accounts.len());
    };
    assert_eq!(account.records(), 2, "two responses about one session");
    assert_eq!(account.superseded(), 0);
    assert!(account.sets().is_some());
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

    let accounts = activities(landed, Vec::new());
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
