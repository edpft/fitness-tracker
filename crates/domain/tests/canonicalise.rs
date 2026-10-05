//! Building the canonical gym session from whichever normalised records hold
//! it (#247): which accounts are one visit, and what the merge makes of them.
//!
//! Every case here is one from the operator's own record, in the shape his
//! store holds it. The loads and counts are his; the normalised session ids
//! stand in for the rows.
//!
//! Tests return `()` and assert by panicking, and the builders beside them
//! return `Result` for the test to unwrap at the call site — the exemptions
//! that let a test panic do not reach a free function.

use domain::{
    canonical::{Attributed, NormalisedSessionId, Occurred},
    gym::{
        CanonicalExercise, CanonicalGymSession, CanonicalItem, CanonicalSet, Guess,
        GuessedExercise, Identified, Load, MeasuredHeartRate, NormalisedGymSession, Performed,
        Recorder, canonical_sessions, exercise::RepsExercise,
    },
    measure::{
        BeatsPerMinute, Duration, HeartRateSample, HeartRateSeries, HeartRateSummary, Kg,
        PositiveDuration, RepCount,
    },
    normalised::{OperatorZone, StartedAt},
    sequence::NonEmpty,
};
use jiff::civil::Date;

type Failure = Box<dyn std::error::Error>;

fn id(number: i64) -> Result<NormalisedSessionId, Failure> {
    Ok(NormalisedSessionId::try_from(number)?)
}

fn at(instant: &str) -> Result<Occurred, Failure> {
    Ok(Occurred::At(StartedAt::new(
        instant.parse()?,
        OperatorZone::try_from("Europe/London".to_owned())?,
    )))
}

fn on(day: &str) -> Result<Occurred, Failure> {
    Ok(Occurred::On(day.parse::<Date>()?))
}

/// One set: its count, and its load in grams where anything recorded one.
fn set(
    reps: u32,
    grams: Option<u64>,
    who: NormalisedSessionId,
) -> Result<CanonicalSet<RepCount>, Failure> {
    Ok(CanonicalSet {
        outcome: Attributed::new(Performed::Completed(Some(RepCount::new(reps)?)), who),
        load: grams.map(|grams| Attributed::new(Load::absolute(Kg::from_grams(grams)), who)),
        began: None,
        intensity: None,
        kind: None,
        rest_after: None,
    })
}

/// An exercise a log named, and its sets as `(reps, grams)`.
fn recorded(
    key: &str,
    sets: &[(u32, u64)],
    who: NormalisedSessionId,
) -> Result<CanonicalItem, Failure> {
    let built = sets
        .iter()
        .map(|(reps, grams)| set(*reps, Some(*grams), who))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CanonicalItem::Exercise(CanonicalExercise::ForReps {
        identified: Attributed::new(
            Identified::Recorded(RepsExercise::try_from(key.to_owned())?),
            who,
        ),
        sets: NonEmpty::new(built)?,
    }))
}

/// An exercise only a watch's classifier placed.
fn guessed(
    key: &str,
    sets: &[(u32, u64)],
    who: NormalisedSessionId,
) -> Result<CanonicalItem, Failure> {
    let built = sets
        .iter()
        .map(|(reps, grams)| set(*reps, Some(*grams), who))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CanonicalItem::Exercise(CanonicalExercise::ForReps {
        identified: Attributed::new(
            Identified::Guessed(GuessedExercise::Proposed(Guess::Exercise(
                RepsExercise::try_from(key.to_owned())?,
            ))),
            who,
        ),
        sets: NonEmpty::new(built)?,
    }))
}

fn log(
    number: i64,
    when: Occurred,
    items: Vec<CanonicalItem>,
) -> Result<NormalisedGymSession, Failure> {
    Ok(NormalisedGymSession::from_log(
        id(number)?,
        when,
        None,
        items,
    ))
}

fn watch(
    number: i64,
    when: Occurred,
    items: Vec<CanonicalItem>,
) -> Result<NormalisedGymSession, Failure> {
    Ok(NormalisedGymSession::new(
        id(number)?,
        Recorder::Watch,
        when,
        None,
        None,
        items,
    ))
}

/// Every load of one exercise of the session, in order, as whole grams.
fn loads(session: &CanonicalGymSession, key: &str) -> Vec<Option<u64>> {
    session
        .exercises()
        .filter(|exercise| exercise.exercise_key() == Some(key))
        .flat_map(sets_of)
        .collect()
}

fn sets_of(exercise: &CanonicalExercise) -> Vec<Option<u64>> {
    match exercise {
        CanonicalExercise::ForReps { sets, .. } => sets
            .iter()
            .map(|set| {
                set.load.as_ref().map(|load| match load.copied() {
                    Load::Absolute(mass) => mass.as_grams(),
                    Load::Relative(_) => 0,
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn keys(session: &CanonicalGymSession) -> Vec<&'static str> {
    session
        .exercises()
        .map(|exercise| exercise.exercise_key().unwrap_or("(undetermined)"))
        .collect()
}

/// 2019-03-14: `1RM.xlsx` is the block's template and `Strength training
/// 2019.xlsx` is what he performed. Nothing about either file says which — what
/// says it is that the watch holds one of them and not the other.
#[test]
fn a_third_account_settles_which_of_two_sheets_was_performed() {
    let template = log(
        1525,
        on("2019-03-14").unwrap(),
        vec![
            recorded(
                "face-pull-cable",
                &[(10, 27_000), (10, 27_000), (10, 27_000)],
                id(1525).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let performed = log(
        1588,
        on("2019-03-14").unwrap(),
        vec![
            recorded(
                "face-pull-cable",
                &[(10, 32_000), (10, 32_000), (10, 27_000)],
                id(1588).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let recording = watch(
        1237,
        at("2019-03-14T07:43:22Z").unwrap(),
        vec![
            guessed(
                "face-pull-cable",
                &[(10, 32_000), (10, 32_000), (10, 27_000)],
                id(1237).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();

    let sessions = canonical_sessions(vec![template, performed, recording]);
    assert_eq!(sessions.len(), 1, "three accounts of one visit");
    let session = sessions.first().expect("one session");
    assert_eq!(
        loads(session, "face-pull-cable"),
        vec![Some(32_000), Some(32_000), Some(27_000)],
        "the sheet the watch agrees with, not the template"
    );
}

/// The operator, 2026-10-03: *"43kg Vs 42.5kg is no disagreement, it's the same
/// value at different levels of resolution."* 2018-05-02's back squat, whose
/// watch file declares 43000, 43000 and 42500 grams.
#[test]
fn a_whole_kilogramme_on_the_dial_is_the_half_the_sheet_states() {
    let sheet = log(
        1,
        on("2018-05-02").unwrap(),
        vec![
            recorded(
                "back-squat-barbell",
                &[(10, 42_500), (10, 42_500), (12, 42_500)],
                id(1).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let recording = watch(
        2,
        at("2018-05-02T12:00:00Z").unwrap(),
        vec![
            guessed(
                "back-squat-barbell",
                &[(10, 43_000), (10, 43_000), (12, 42_500)],
                id(2).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();

    let sessions = canonical_sessions(vec![sheet, recording]);
    let session = sessions.first().expect("one session");
    assert_eq!(
        loads(session, "back-squat-barbell"),
        vec![Some(42_500), Some(42_500), Some(42_500)],
        "three sets, and the figure the bar actually held"
    );
}

/// 2018-04-28: the sheet kept the bench press's last set and the watch has all
/// three. The sheet's one set is the watch's third, not its first.
#[test]
fn a_sheets_top_set_aligns_with_the_set_it_was() {
    let sheet = log(
        1,
        on("2018-04-28").unwrap(),
        vec![recorded("bench-press-barbell", &[(4, 36_000)], id(1).unwrap()).unwrap()],
    )
    .unwrap();
    let recording = watch(
        2,
        at("2018-04-28T13:28:13Z").unwrap(),
        vec![
            guessed(
                "bench-press-barbell",
                &[(10, 40_000), (6, 40_000), (4, 36_000)],
                id(2).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();

    let sessions = canonical_sessions(vec![sheet, recording]);
    let session = sessions.first().expect("one session");
    assert_eq!(
        loads(session, "bench-press-barbell"),
        vec![Some(40_000), Some(40_000), Some(36_000)],
        "three sets, not four"
    );
}

/// 2019-03-14 again: the watch called three sets of 10 × 70 `BARBELL_DEADLIFT`
/// and the operator's sheet for that block calls them Romanian deadlifts. One
/// exercise, and the log names it.
#[test]
fn a_guess_does_not_duplicate_the_exercise_the_log_named() {
    let sheet = log(
        1525,
        on("2019-03-14").unwrap(),
        vec![
            recorded(
                "romanian-deadlift-barbell",
                &[(10, 70_000), (10, 70_000), (10, 70_000)],
                id(1525).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let recording = watch(
        1237,
        at("2019-03-14T08:04:16Z").unwrap(),
        vec![
            guessed(
                "deadlift-barbell",
                &[(10, 70_000), (10, 70_000), (10, 70_000)],
                id(1237).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();

    let sessions = canonical_sessions(vec![sheet, recording]);
    let session = sessions.first().expect("one session");
    assert_eq!(
        keys(session),
        vec!["romanian-deadlift-barbell"],
        "one exercise, under the name the operator gave it"
    );
    assert_eq!(session.set_count(), 3, "three sets, not six");
}

/// The operator, 2026-10-01: *"A canonical gym session must have exercises and
/// may have heart rate data. Heart rate only isn't a meaningful gym session."*
/// 160 of his 551 gym days are this.
#[test]
fn a_recording_with_no_sets_is_no_canonical_session() {
    let summary = HeartRateSummary::new(
        BeatsPerMinute::new(77).unwrap(),
        BeatsPerMinute::new(133).unwrap(),
    );
    let recording = NormalisedGymSession::new(
        id(9).unwrap(),
        Recorder::Watch,
        at("2026-09-25T17:00:00Z").unwrap(),
        PositiveDuration::from_seconds(2_700)
            .ok()
            .map(|length| Attributed::new(length, id(9).unwrap())),
        Some(Attributed::new(
            MeasuredHeartRate::new(summary, None),
            id(9).unwrap(),
        )),
        Vec::new(),
    );

    assert!(
        canonical_sessions(vec![recording]).is_empty(),
        "a heart rate and nothing else"
    );
}

/// `Bench day (2019-03-06)` is the watch's activity on the 7th. A group of
/// accounts holding no clock joins an adjacent day where exactly one adjacent
/// day holds one.
#[test]
fn a_sheet_dated_a_day_early_joins_the_day_it_was_trained() {
    let sheet = log(
        1522,
        on("2019-03-06").unwrap(),
        vec![recorded("bench-press-barbell", &[(5, 60_000)], id(1522).unwrap()).unwrap()],
    )
    .unwrap();
    let recording = watch(
        1240,
        at("2019-03-07T07:40:00Z").unwrap(),
        vec![guessed("bench-press-barbell", &[(5, 60_000)], id(1240).unwrap()).unwrap()],
    )
    .unwrap();

    let sessions = canonical_sessions(vec![sheet, recording]);
    assert_eq!(sessions.len(), 1, "one visit, not two");
    let session = sessions.first().expect("one session");
    assert_eq!(
        session.occurred().day(),
        "2019-03-07".parse::<Date>().unwrap(),
        "the day the watch recorded it"
    );
}

/// A session only one source recorded is still one session, and the degenerate
/// case is not a different kind of thing.
#[test]
fn one_account_is_one_session() {
    let sheet = log(
        1,
        on("2017-01-09").unwrap(),
        vec![
            recorded(
                "back-squat-barbell",
                &[(5, 50_000), (5, 50_000)],
                id(1).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();

    let sessions = canonical_sessions(vec![sheet]);
    assert_eq!(sessions.len(), 1);
    assert_eq!(
        sessions.first().expect("one session").set_count(),
        2,
        "both sets, unchanged"
    );
}

/// 2019-03-20's front squat: 47.5 × 3 in the sheet and 50 × 3 on the watch,
/// whose last counts differ too. Nothing pairs by value, and aligning that way
/// gave four sets for a three-set exercise.
#[test]
fn two_accounts_of_the_same_length_align_by_order() {
    let sheet = log(
        1,
        on("2019-03-20").unwrap(),
        vec![
            recorded(
                "front-squat-barbell",
                &[(10, 47_500), (10, 47_500), (12, 47_500)],
                id(1).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let recording = watch(
        2,
        at("2019-03-20T07:50:00Z").unwrap(),
        vec![
            guessed(
                "front-squat-barbell",
                &[(10, 50_000), (10, 50_000), (8, 50_000)],
                id(2).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();

    let sessions = canonical_sessions(vec![sheet, recording]);
    let session = sessions.first().expect("one session");
    assert_eq!(session.set_count(), 3, "three sets, not four");
}

/// A recording, its readings at `(minute, bpm)`, one a minute.
fn readings(beats: &[(u64, u32)]) -> Result<MeasuredHeartRate, Failure> {
    let samples = beats
        .iter()
        .map(|(minute, bpm)| {
            Ok(HeartRateSample {
                at: Duration::from_seconds(minute * 60),
                beats_per_minute: BeatsPerMinute::new(*bpm)?,
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    let highest = beats.iter().map(|(_, bpm)| *bpm).max().unwrap_or(1);
    Ok(MeasuredHeartRate::new(
        HeartRateSummary::new(BeatsPerMinute::new(110)?, BeatsPerMinute::new(highest)?),
        Some(HeartRateSeries::new(NonEmpty::new(samples)?)),
    ))
}

fn length(minutes: u64, who: NormalisedSessionId) -> Option<Attributed<PositiveDuration>> {
    PositiveDuration::from_seconds(minutes * 60)
        .ok()
        .map(|stated| Attributed::new(stated, who))
}

/// A watch's account of a session, with the readings it wrote.
fn recording(
    number: i64,
    occurred: Occurred,
    minutes: u64,
    beats: &[(u64, u32)],
) -> Result<NormalisedGymSession, Failure> {
    let who = id(number)?;
    Ok(NormalisedGymSession::new(
        who,
        Recorder::Watch,
        occurred,
        length(minutes, who),
        Some(Attributed::new(readings(beats)?, who)),
        vec![guessed("back-squat-barbell", &[(5, 60_000)], who)?],
    ))
}

/// Readings a minute apart from `from` to `to`, all at `bpm`.
fn flat(from: u64, to: u64, bpm: u32) -> Vec<(u64, u32)> {
    (from..=to).map(|minute| (minute, bpm)).collect()
}

/// 2025-07-25: the watch recorded 119 minutes, Hevy's last set is at 57, and
/// the readings hold 75 to 80 from minute 90 to the end.
///
/// #321, and the clearest case in the operator's record. The visit is neither
/// account's figure: cutting at Hevy's end would throw away the 29 minutes of
/// elevated heart rate that follow it, and the watch's 119 includes the drive
/// home.
#[test]
fn the_watch_left_running_states_the_visit_not_the_recording() {
    let mut beats = flat(0, 89, 120);
    beats.extend(flat(90, 118, 78));
    let logged = NormalisedGymSession::from_log(
        id(1).unwrap(),
        at("2025-07-25T17:00:00Z").unwrap(),
        length(57, id(1).unwrap()),
        vec![recorded("back-squat-barbell", &[(5, 60_000)], id(1).unwrap()).unwrap()],
    );
    let watched = recording(2, at("2025-07-25T17:00:00Z").unwrap(), 119, &beats).unwrap();

    let sessions = canonical_sessions(vec![logged, watched]);
    let session = sessions.first().expect("one session");
    let stated = session.duration().expect("a length");
    assert_eq!(
        stated.copied().as_seconds(),
        90 * 60,
        "where the readings went quiet, not 119 minutes and not Hevy's 57"
    );
    assert_eq!(
        stated.normalised_session(),
        id(2).unwrap(),
        "the watch's recording is what it was read off"
    );
}

/// 2025-08-08: Hevy started seven minutes before the watch did.
///
/// The watch recorded 104 minutes from 17:07, Hevy's last set is at 17:54, and
/// the readings fall to 95 from minute 85. The log's offset from the watch's
/// start is negative, and an end worked out as unsigned would discard the only
/// account that brackets the visit.
#[test]
fn a_log_that_started_before_the_watch_still_bounds_the_visit() {
    let mut beats = flat(0, 84, 140);
    beats.extend(flat(85, 103, 95));
    let logged = NormalisedGymSession::from_log(
        id(1).unwrap(),
        at("2025-08-08T17:00:00Z").unwrap(),
        length(54, id(1).unwrap()),
        vec![recorded("back-squat-barbell", &[(5, 60_000)], id(1).unwrap()).unwrap()],
    );
    let watched = recording(2, at("2025-08-08T17:07:00Z").unwrap(), 104, &beats).unwrap();

    let sessions = canonical_sessions(vec![logged, watched]);
    let stated = sessions
        .first()
        .and_then(CanonicalGymSession::duration)
        .expect("a length");
    assert_eq!(
        stated.copied().as_seconds(),
        85 * 60,
        "trimmed where the readings went quiet, not left at 104 minutes"
    );
}

/// 2026-07-20: the watch recorded 98 minutes and Hevy 98, and the readings dip
/// between sets from minute 71.
///
/// Two sources state the same end, so there is nothing to trim however quiet
/// the readings go — his seven 2026 long sessions were genuinely 94 to 104
/// minutes, and a between-sets dip is not a drive home.
#[test]
fn two_accounts_agreeing_on_the_end_are_not_trimmed() {
    let mut beats = flat(0, 70, 117);
    beats.extend(flat(71, 97, 90));
    let logged = NormalisedGymSession::from_log(
        id(1).unwrap(),
        at("2026-07-20T17:00:00Z").unwrap(),
        length(98, id(1).unwrap()),
        vec![recorded("back-squat-barbell", &[(5, 60_000)], id(1).unwrap()).unwrap()],
    );
    let watched = recording(2, at("2026-07-20T17:00:00Z").unwrap(), 98, &beats).unwrap();

    let sessions = canonical_sessions(vec![logged, watched]);
    assert_eq!(
        sessions
            .first()
            .and_then(CanonicalGymSession::duration)
            .map(|stated| stated.copied().as_seconds()),
        Some(98 * 60),
        "the whole recording"
    );
}

/// 2025-02-12: Hevy holds 5 minutes of a 57-minute recording.
///
/// An account covering less of the visit than the trim would discard is a
/// fragment, not evidence about where the visit ended — and a floor read off
/// five minutes is not a floor.
#[test]
fn a_fragment_of_a_log_does_not_bound_the_visit() {
    let mut beats = flat(0, 15, 128);
    beats.extend(flat(16, 56, 100));
    let logged = NormalisedGymSession::from_log(
        id(1).unwrap(),
        at("2025-02-12T17:00:00Z").unwrap(),
        length(5, id(1).unwrap()),
        vec![recorded("back-squat-barbell", &[(5, 60_000)], id(1).unwrap()).unwrap()],
    );
    let watched = recording(2, at("2025-02-12T17:00:00Z").unwrap(), 57, &beats).unwrap();

    let sessions = canonical_sessions(vec![logged, watched]);
    assert_eq!(
        sessions
            .first()
            .and_then(CanonicalGymSession::duration)
            .map(|stated| stated.copied().as_seconds()),
        Some(57 * 60),
        "the whole recording, because the log says nothing about its end"
    );
}

/// 2025-03-31: Hevy ran 18:00 to 19:00 and the watch 18:47 to 19:00.
///
/// He was logging 47 minutes before he started the watch, and both accounts end
/// within a minute of each other. Each is a floor on the visit, so the longest
/// stands — the watch's 12 minutes is what it recorded, not how long he was
/// there.
#[test]
fn a_log_that_started_before_the_watch_states_the_visit() {
    let logged = NormalisedGymSession::from_log(
        id(1).unwrap(),
        at("2025-03-31T18:00:53Z").unwrap(),
        length(60, id(1).unwrap()),
        vec![recorded("back-squat-barbell", &[(5, 60_000)], id(1).unwrap()).unwrap()],
    );
    let watched = recording(
        2,
        at("2025-03-31T18:47:33Z").unwrap(),
        12,
        &flat(0, 11, 120),
    )
    .unwrap();

    let sessions = canonical_sessions(vec![logged, watched]);
    let stated = sessions
        .first()
        .and_then(CanonicalGymSession::duration)
        .expect("a length");
    assert_eq!(stated.copied().as_seconds(), 60 * 60, "the hour he logged");
    assert_eq!(
        stated.normalised_session(),
        id(1).unwrap(),
        "and it says so"
    );
}

/// 2025-06-23: the watch recorded 104 minutes, Hevy's last set is at 70, and
/// the readings never fall below the 80bpm he reached between sets.
///
/// #321's honest answer: he dipped lower while demonstrably training than he
/// ever does afterwards, so nothing in the record separates a long session
/// from one the watch was left running through. The measurement stands.
#[test]
fn a_tail_no_lower_than_his_own_rest_is_not_a_drive_home() {
    let mut beats = flat(0, 69, 118);
    beats.push((40, 80));
    beats.extend(flat(70, 103, 88));
    let logged = NormalisedGymSession::from_log(
        id(1).unwrap(),
        at("2025-06-23T17:00:00Z").unwrap(),
        length(70, id(1).unwrap()),
        vec![recorded("back-squat-barbell", &[(5, 60_000)], id(1).unwrap()).unwrap()],
    );
    let watched = recording(2, at("2025-06-23T17:00:00Z").unwrap(), 104, &beats).unwrap();

    let sessions = canonical_sessions(vec![logged, watched]);
    assert_eq!(
        sessions
            .first()
            .and_then(CanonicalGymSession::duration)
            .map(|stated| stated.copied().as_seconds()),
        Some(104 * 60),
        "the whole recording, undeterminable rather than trimmed"
    );
}

/// A sixteen-minute silence does not break a quiet stretch.
///
/// 2025-07-25's watch wrote nothing at all from minute 102 to 118, then one
/// reading of 74. Treating a minute it wrote nothing in as training would miss
/// the clearest case in the record.
#[test]
fn a_minute_the_watch_wrote_nothing_in_does_not_break_the_stretch() {
    let mut beats = flat(0, 89, 120);
    beats.extend(flat(90, 101, 78));
    beats.push((118, 74));
    let logged = NormalisedGymSession::from_log(
        id(1).unwrap(),
        at("2025-07-25T17:00:00Z").unwrap(),
        length(57, id(1).unwrap()),
        vec![recorded("back-squat-barbell", &[(5, 60_000)], id(1).unwrap()).unwrap()],
    );
    let watched = recording(2, at("2025-07-25T17:00:00Z").unwrap(), 119, &beats).unwrap();

    let sessions = canonical_sessions(vec![logged, watched]);
    assert_eq!(
        sessions
            .first()
            .and_then(CanonicalGymSession::duration)
            .map(|stated| stated.copied().as_seconds()),
        Some(90 * 60),
        "one stretch from minute 90, silence and all"
    );
}

/// A session's length where only one account states one.
#[test]
fn one_account_states_the_length_alone() {
    let alone = NormalisedGymSession::new(
        id(3).unwrap(),
        Recorder::Watch,
        at("2025-03-19T17:00:00Z").unwrap(),
        length(47, id(3).unwrap()),
        None,
        vec![guessed("bench-press-barbell", &[(5, 60_000)], id(3).unwrap()).unwrap()],
    );
    let sessions = canonical_sessions(vec![alone]);
    assert_eq!(
        sessions
            .first()
            .and_then(CanonicalGymSession::duration)
            .map(|stated| stated.copied().as_seconds()),
        Some(47 * 60),
        "the watch's length where nothing else states one"
    );

    // A sheet states a day and a length and no clock, so nothing brackets it.
    let sheet = NormalisedGymSession::from_log(
        id(4).unwrap(),
        on("2019-03-20").unwrap(),
        length(52, id(4).unwrap()),
        vec![recorded("back-squat-barbell", &[(5, 60_000)], id(4).unwrap()).unwrap()],
    );
    let sessions = canonical_sessions(vec![sheet]);
    assert_eq!(
        sessions
            .first()
            .and_then(CanonicalGymSession::duration)
            .map(|stated| stated.copied().as_seconds()),
        Some(52 * 60),
        "the operator's own record where no watch recorded the visit"
    );
}
