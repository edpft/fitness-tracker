//! Beyond The White Board's export, derived as gym sessions (#285).
//!
//! Each result below is a row of the operator's export of 2026-09-28, copied
//! as BTWB wrote it.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use application::{NormalisationError, Translation, ports::Translator};
use domain::{
    gym::{ManualExercise, ManualGymSession, ManualItem, ManualSet, Performed},
    landing::{
        FetchedAt, FilePath, FileProvenance, LandedRecord, LandingRecord, LandingRecordId,
        LandingStream, ModifiedAt, RawPayload, SourceRecordId,
    },
    normalised::OperatorZone,
};
use infrastructure::{BtwbTranslator, Exports};

type Failure = Box<dyn std::error::Error>;

const HEADER: &str = "Date,Formatted Result,Result,Performed,Workout,Description,Notes\n";

/// 26 February 2025: a back squat and an AMRAP, one class.
const SQUAT_AND_AMRAP: &str = "2025-02-26,285 kg | 95 kg,285.0,prescribed,Back Squat : 3 Rep Max,\"Sets
3 Back Squats | 95 kg\",\"\"
2025-02-26,3 rounds + 40 Single Unders | 412 reps,3.133,modified,\"AMRAP 12 mins: Double Unders, Dumbbell Snatches, and Toes-to-bars\",\"12:00 AMRAP:
100 Single Unders
16 Dumbbell Snatches
8 Toes-to-bars\",\"\"
";

/// 9 September 2024: a lift, and a result whose lines do not account for its score.
const LIFT_AND_AMREPS: &str = "2024-09-09,840 kg | 70 kg,840.0,prescribed,Romanian Deadlift : 12 Rep Max,\"Sets
12 Romanian Deadlifts | 70 kg\",\"\"
2024-09-09,168 reps | 24's,168,modified,\"AMReps 10 mins (4,8,12,...): Wall Balls and Push-ups\",\"AMReps in 10 mins:
4 Wall Balls
4x [ 6 Push-ups ]\",\"\"
";

fn landed(id: i64, path: &str, csv: &str) -> Result<LandedRecord, Failure> {
    let provenance = FileProvenance::new(
        FilePath::try_from(path)?,
        ModifiedAt::try_from("2026-09-28T09:00:00Z")?,
    );
    let landed = LandingRecord::land(
        LandingStream::try_from("btwb.exports")?,
        FetchedAt::try_from("2026-09-28T12:00:00Z")?,
        SourceRecordId::try_from(format!("digest-of-export-{id}"))?,
        provenance.into(),
        RawPayload::try_from(csv.as_bytes().to_vec())?,
    );
    Ok(LandedRecord::new(LandingRecordId::try_from(id)?, landed))
}

fn translate(
    exports: Vec<LandedRecord>,
) -> Result<Result<Translation<ManualGymSession>, NormalisationError>, Failure> {
    let zone = OperatorZone::try_from("Europe/London")?;
    let Some(exports) = Exports::gather(exports).pop() else {
        return Err("no exports".into());
    };
    Ok(BtwbTranslator.translate(&exports, &zone))
}

fn set<M: std::fmt::Display>(set: &ManualSet<M>) -> String {
    let load = set
        .load
        .map_or_else(|| "?".to_owned(), |load| load.to_string());
    let outcome = match &set.outcome {
        Performed::Completed(Some(measure)) => measure.to_string(),
        Performed::Completed(None) => "?".to_owned(),
        Performed::Failed => "failed".to_owned(),
    };
    format!("{load} × {outcome}")
}

fn exercise(exercise: &ManualExercise) -> String {
    let sets: Vec<String> = match exercise {
        ManualExercise::ForReps { sets, .. } => sets.iter().map(set).collect(),
        ManualExercise::ForDuration { sets, .. } => sets.iter().map(set).collect(),
        ManualExercise::ForDistance { sets, .. } => sets.iter().map(set).collect(),
    };
    format!("{}: {}", exercise.exercise_key(), sets.join("; "))
}

/// Each item as the operator would check it: an exercise, or a superset's
/// members in brackets.
fn described(session: &ManualGymSession) -> Vec<String> {
    session
        .items()
        .iter()
        .map(|item| match item {
            ManualItem::Exercise(one) => exercise(one),
            ManualItem::Superset(_) => format!(
                "[{}]",
                item.exercises()
                    .map(exercise)
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
        })
        .collect()
}

/// A day's results are one session: the lift, then the AMRAP as a superset of
/// its whole rounds, and the partial round its score names.
#[test]
fn a_day_is_one_session_and_an_amrap_its_rounds() {
    let csv = format!("{HEADER}{SQUAT_AND_AMRAP}");
    let translation = translate(vec![
        landed(1, "2026-09-28-workout_sessions.csv", &csv).expect("an export"),
    ])
    .expect("translated")
    .expect("no run failure");
    let Translation::Entities { entities, refusals } = translation else {
        panic!("sessions expected");
    };
    assert!(refusals.is_empty(), "{refusals:?}");
    let sessions: Vec<&ManualGymSession> = entities.iter().collect();
    assert_eq!(sessions.len(), 1);
    let session = sessions.first().expect("one session");
    assert_eq!(session.on().to_string(), "2025-02-26");
    assert_eq!(
        described(session),
        vec![
            "squat-barbell: 95 kg × 3".to_owned(),
            "[jump-rope: ? × ?; ? × ?; ? × ?; ? × ? | \
             dumbbell-snatch: ? × 16; ? × 16; ? × 16 | \
             toes-to-bar: ? × 8; ? × 8; ? × 8]"
                .to_owned(),
        ]
    );
}

/// What cannot be read is refused with its reason, and the rest of the day
/// still derives.
#[test]
fn a_result_it_cannot_read_is_refused_and_the_day_still_derives() {
    let csv = format!("{HEADER}{LIFT_AND_AMREPS}");
    let translation = translate(vec![
        landed(1, "2026-09-28-workout_sessions.csv", &csv).expect("an export"),
    ])
    .expect("translated")
    .expect("no run failure");
    let Translation::Entities { entities, refusals } = translation else {
        panic!("sessions expected");
    };
    let session = entities.first();
    assert_eq!(
        described(session),
        vec!["romanian-deadlift-barbell: 70 kg × 12".to_owned()]
    );
    assert_eq!(refusals.len(), 1);
    let reason = refusals.first().map(|refusal| refusal.reason.to_string());
    assert!(
        reason
            .as_deref()
            .is_some_and(|reason| reason.contains("row 3") && reason.contains("AMReps in 10 mins")),
        "{reason:?}"
    );
}

/// A day is read from the export landed last that holds it, so an export
/// landed twice counts once, and an earlier one adds only the days the later
/// lacks.
#[test]
fn each_day_comes_from_the_last_export_holding_it() {
    let early = format!("{HEADER}{LIFT_AND_AMREPS}{SQUAT_AND_AMRAP}");
    let late = format!("{HEADER}{SQUAT_AND_AMRAP}");
    let translation = translate(vec![
        landed(1, "20260804workout_sessions.csv", &early).expect("an export"),
        landed(2, "2026-09-28-workout_sessions.csv", &late).expect("an export"),
    ])
    .expect("translated")
    .expect("no run failure");
    let Translation::Entities { entities, .. } = translation else {
        panic!("sessions expected");
    };
    let from: Vec<(String, String)> = entities
        .iter()
        .map(|session| (session.on().to_string(), session.logged().file.to_string()))
        .collect();
    assert_eq!(
        from,
        vec![
            (
                "2024-09-09".to_owned(),
                "20260804workout_sessions.csv".to_owned()
            ),
            (
                "2025-02-26".to_owned(),
                "2026-09-28-workout_sessions.csv".to_owned()
            ),
        ]
    );
}

/// A movement the vocabulary does not map stops the run, as an unmapped Hevy
/// template does: a gap in the vocabulary is a defect here, not in the data.
#[test]
fn an_unmapped_movement_stops_the_run() {
    let csv = format!(
        "{HEADER}2024-09-09,Completed,,prescribed,Mystery,\"3 rounds of:\n5 Muscle-ups\",\"\"\n"
    );
    let outcome =
        translate(vec![landed(1, "export.csv", &csv).expect("an export")]).expect("translated");
    assert!(
        matches!(
            outcome,
            Err(NormalisationError::UnmappedExercise { ref template_id, .. }) if template_id == "Muscle-ups"
        ),
        "{outcome:?}"
    );
}

/// A file that is not BTWB's export is refused as such.
#[test]
fn a_file_that_is_not_an_export_is_refused() {
    let translation = translate(vec![landed(1, "notes.csv", "a,b\n1,2\n").expect("a file")])
        .expect("translated")
        .expect("no run failure");
    assert!(
        matches!(translation, Translation::Refused(_)),
        "{translation:?}"
    );
}
