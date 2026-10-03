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
    measure::{BeatsPerMinute, HeartRateSummary, Kg, PositiveDuration, RepCount},
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
