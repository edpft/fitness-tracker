//! Manual gym sessions, read out of the operator's historical spreadsheets
//! (#274).
//!
//! **Which sheets are training logs is declared, not guessed**, as with the
//! weigh-ins: each layout below is recognised by its header, word for word,
//! and each was read off the files and settled with the operator on
//! 2026-09-27. A sheet that matches none of them is not a training log.
//!
//! **Only what was performed** (operator, 2026-09-27: *"if there isn't a
//! positive sign that the session was performed … we must assume that it was
//! just a plan"*). A session derives only where the sheet shows it was done: a
//! note, an RPE or reps in reserve (the 2019 session sheets, the 2020 log), a
//! performed column beside the planned one (`CT 2017`), or a sheet that is a
//! log with no plan in it, whose numbers are outcomes (2016's `Weights`, the
//! 2018 `1RM` sheets and `Training Log`, 2023's e1RM sets). A dated plan is
//! refused as a plan, so it shows up among the refusals rather than vanishing.
//!
//! **A working set with a load and no reps is a session the sheet does not
//! record** (1 June 2016, 13 June 2018). The whole session is refused, not just
//! the set.
//!
//! **What each name means is the operator's**: a squat with no qualifier is a
//! back squat, `db` a dumbbell, `bb` a barbell, and a press the overhead press.
//! The 2020 log pressed dumbbells, so its press is the dumbbell press. A name
//! with no mapping stops the run, as an unmapped Hevy template does: a gap in
//! the vocabulary is a defect here, not in the data.

use std::collections::BTreeMap;

use application::{NormalisationError, Translation, ports::Translator};
use domain::{
    gym::{
        Load, Logged, ManualExercise, ManualGymSession, ManualSet, Performed, Rir, SetKind,
        SignedKg,
        exercise::{DurationExercise, Exercise, Implement, RepsExercise},
    },
    landing::{CellRef, FileProvenance, LandedRecord, SheetCell, SheetName},
    measure::{Duration, Kg, RepCount},
    normalised::{OperatorZone, RefusalLocus, RefusalReason},
    sequence::NonEmpty,
};
use jiff::{
    ToSpan as _,
    civil::{Date, Weekday},
};

use super::sheet::{Sheet, Value};
use crate::scribe::Scribe;

/// Reads manual gym sessions out of a landed spreadsheet.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpreadsheetSessionTranslator;

impl Translator for SpreadsheetSessionTranslator {
    type Account = LandedRecord;
    type Entity = ManualGymSession;

    /// The zone is not consulted: a manual session is a day.
    fn translate(
        &self,
        record: &LandedRecord,
        _zone: &OperatorZone,
    ) -> Result<Translation<ManualGymSession>, NormalisationError> {
        super::translate_with(record, "gym sessions", |file, sheets, scribe| {
            read_sessions(record, file, sheets, scribe)
        })
    }
}

/// Every performed session in a workbook's sheets.
///
/// # Errors
///
/// [`NormalisationError::UnmappedExercise`] for a name the vocabulary does not
/// map.
pub(super) fn read_sessions(
    record: &LandedRecord,
    file: &FileProvenance,
    sheets: &[Sheet],
    scribe: &mut Scribe,
) -> Result<Vec<ManualGymSession>, NormalisationError> {
    let mut drafts = Vec::new();
    for sheet in sheets {
        session_sheet(sheet, &mut drafts);
        weights_2016(sheet, &mut drafts);
        training_log(sheet, &mut drafts);
        beginner_2020(sheet, &mut drafts);
        estimates_2023(sheet, &mut drafts);
    }
    lift_sheets_2018(sheets, &mut drafts);
    conditioning_2017(sheets, &mut drafts, scribe);

    let logged = Logged {
        landed_as: record.id(),
        source_record_id: record.source_record_id().clone(),
        file: file.path().clone(),
    };
    let mut sessions = Vec::new();
    for draft in drafts {
        if let Some(session) = finish(draft, &logged, record, scribe)? {
            sessions.push(session);
        }
    }
    Ok(sessions)
}

/// How many reps, or how long, a sheet says a set was.
#[derive(Debug, Clone, Copy)]
enum Count {
    Reps(RepCount),
    Held(Duration),
    /// No reps at a load: `Squat day (19-03-08)` writes `0` against 87.5 kg,
    /// with the note "failed".
    Failed,
    /// A load with nothing beside it. On a working set, the session was not
    /// recorded as performed.
    Missing,
}

/// One set as a sheet wrote it, before its name is mapped.
#[derive(Debug, Clone)]
struct Entry {
    name: String,
    /// `None` where the sheet does not record the load.
    load: Option<Kg>,
    /// The body weight a `CT 2017` row was logged at. Its dips and pull-ups
    /// write that as the load, which is plain bodyweight.
    bodyweight: Option<Kg>,
    count: Count,
    intensity: Option<Rir>,
    kind: SetKind,
    rest_after: Option<Duration>,
    written_in: SheetCell,
}

/// What a press is, in one layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Press {
    Barbell,
    /// The 2020 log: *"press with 2x 14kg is overhead-press-dumbbell"*.
    Dumbbell,
}

/// One session as the sheets have it, before anything is decided about it.
#[derive(Debug, Clone)]
struct Draft {
    on: Date,
    /// How the refusals name it: `Squat day (19-02-12)`, `Push, week 8`.
    what: String,
    /// Whether anything shows it was done.
    performed: bool,
    press: Press,
    entries: Vec<Entry>,
}

/// A draft becomes a session, or says why not.
fn finish(
    draft: Draft,
    logged: &Logged,
    record: &LandedRecord,
    scribe: &mut Scribe,
) -> Result<Option<ManualGymSession>, NormalisationError> {
    let Draft {
        on,
        what,
        performed,
        press,
        entries,
    } = draft;

    if !performed {
        unmodelled(
            scribe,
            format!("{what} ({on}) is a plan, with nothing to show it was performed"),
        );
        return Ok(None);
    }
    if entries
        .iter()
        .any(|entry| entry.kind == SetKind::Working && matches!(entry.count, Count::Missing))
    {
        unmodelled(
            scribe,
            format!(
                "{what} ({on}) has a working set with a load and no reps, so the sheet does not \
                 record it performed"
            ),
        );
        return Ok(None);
    }

    let mut exercises: Vec<(Exercise, Vec<Entry>)> = Vec::new();
    for entry in entries {
        let Some(exercise) = exercise_named(&entry.name, press) else {
            return Err(NormalisationError::UnmappedExercise {
                template_id: entry.name,
                source_record_id: record.source_record_id().to_string(),
            });
        };
        if matches!(entry.count, Count::Missing) || is_empty_bar(exercise, &entry) {
            continue;
        }
        match exercises.last_mut() {
            Some((last, sets)) if *last == exercise => sets.push(entry),
            _ => exercises.push((exercise, vec![entry])),
        }
    }

    let mut built = Vec::new();
    for (exercise, sets) in exercises {
        if let Some(exercise) = performed_exercise(exercise, sets, &what, scribe) {
            built.push(exercise);
        }
    }
    let Ok(exercises) = NonEmpty::new(built) else {
        unmodelled(scribe, format!("{what} ({on}) records no set"));
        return Ok(None);
    };
    Ok(Some(ManualGymSession::new(on, logged.clone(), exercises)))
}

/// A warm-up of nothing on an exercise that is never done with nothing.
///
/// `CT 2017` writes 0 kg against every deadlift warm-up, because its warm-up
/// columns were a template the deadlift never used: an empty barbell is 20 kg,
/// so 0 is a blank rather than a set.
fn is_empty_bar(exercise: Exercise, entry: &Entry) -> bool {
    entry.kind == SetKind::Warmup
        && exercise.implement() != Implement::Bodyweight
        && entry.load.is_some_and(Kg::is_none)
}

/// The sets of one exercise, in the order written.
fn performed_exercise(
    exercise: Exercise,
    entries: Vec<Entry>,
    what: &str,
    scribe: &mut Scribe,
) -> Option<ManualExercise> {
    match exercise {
        Exercise::Reps(exercise) => {
            let mut sets = Vec::new();
            for entry in entries {
                let outcome = match entry.count {
                    Count::Reps(reps) => Performed::Completed(reps),
                    Count::Failed => Performed::Failed,
                    Count::Held(_) | Count::Missing => {
                        unmodelled(
                            scribe,
                            format!(
                                "{what}: {} at {} is not counted in reps",
                                exercise, entry.written_in
                            ),
                        );
                        continue;
                    }
                };
                sets.push(manual_set(Exercise::Reps(exercise), entry, outcome));
            }
            NonEmpty::new(sets)
                .ok()
                .map(|sets| ManualExercise::ForReps { exercise, sets })
        }
        Exercise::Duration(exercise) => {
            let mut sets = Vec::new();
            for entry in entries {
                let outcome = match entry.count {
                    Count::Held(held) => Performed::Completed(held),
                    Count::Failed => Performed::Failed,
                    Count::Reps(_) | Count::Missing => {
                        unmodelled(
                            scribe,
                            format!("{what}: {} at {} is not a time", exercise, entry.written_in),
                        );
                        continue;
                    }
                };
                sets.push(manual_set(Exercise::Duration(exercise), entry, outcome));
            }
            NonEmpty::new(sets)
                .ok()
                .map(|sets| ManualExercise::ForDuration { exercise, sets })
        }
        Exercise::Distance(exercise) => {
            unmodelled(
                scribe,
                format!("{what}: no sheet records {exercise} as a distance"),
            );
            None
        }
    }
}

fn manual_set<M>(exercise: Exercise, entry: Entry, outcome: Performed<M>) -> ManualSet<M> {
    ManualSet {
        load: entry
            .load
            .map(|mass| load_of(exercise, mass, entry.bodyweight)),
        outcome,
        intensity: entry.intensity,
        kind: entry.kind,
        rest_after: entry.rest_after,
        written_in: entry.written_in,
    }
}

/// A load on the axis its exercise is loaded on.
///
/// The exercises assistance is conventionally given on are relative to
/// bodyweight, as the Hevy adapter reads them. A sheet writes the weight added,
/// so 0 is plain bodyweight — except that `CT 2017` writes the body weight
/// itself, which is plain bodyweight too.
fn load_of(exercise: Exercise, mass: Kg, bodyweight: Option<Kg>) -> Load {
    let relative = matches!(
        exercise,
        Exercise::Reps(
            RepsExercise::ChestDip
                | RepsExercise::PullUp
                | RepsExercise::PullUpNegative
                | RepsExercise::ChinUp
        )
    );
    if !relative {
        return Load::absolute(mass);
    }
    if bodyweight == Some(mass) {
        return Load::BODYWEIGHT;
    }
    Load::relative(SignedKg::from_grams(
        i64::try_from(mass.as_grams()).unwrap_or(i64::MAX),
    ))
}

/// The exercise a sheet's name means (operator, 2026-09-27).
fn exercise_named(name: &str, press: Press) -> Option<Exercise> {
    let name = name
        .replace("_x000D_", "")
        .replace("_x000d_", "")
        .trim()
        .to_lowercase();
    let reps = match name.as_str() {
        "squat" | "back squat" => RepsExercise::SquatBarbell,
        "bench" | "bench press" => RepsExercise::BenchPressBarbell,
        "deadlift" => RepsExercise::DeadliftBarbell,
        "front squat" => RepsExercise::FrontSquat,
        "rdl" => RepsExercise::RomanianDeadliftBarbell,
        // Row at 24 kg is ambiguous, and assumed a barbell.
        "bb row" | "bo row" | "row" => RepsExercise::BentOverRowBarbell,
        "1-arm db row" => RepsExercise::SingleArmRowDumbbell,
        "pull down" | "pull-down" | "pulldown" | "lat pulldown" => RepsExercise::LatPulldownCable,
        "pull up" | "pull-up" => RepsExercise::PullUp,
        "pull up negatives" => RepsExercise::PullUpNegative,
        "chin up" | "chin-up" => RepsExercise::ChinUp,
        "kb swing" => RepsExercise::KettlebellSwing,
        "dip" | "dips" => RepsExercise::ChestDip,
        "oh press" | "overhead press" | "press" => match press {
            Press::Barbell => RepsExercise::OverheadPressBarbell,
            Press::Dumbbell => RepsExercise::OverheadPressDumbbell,
        },
        "lunge" => RepsExercise::LungeDumbbell,
        "bss" => RepsExercise::BulgarianSplitSquatDumbbell,
        "deadbug" | "deadbugs" | "loaded deadbug" => RepsExercise::DeadBug,
        "pushdown" => RepsExercise::TricepsExtensionCable,
        "hip thrust" => RepsExercise::HipThrustBarbell,
        "face-pull" | "face pull" => RepsExercise::FacePullCable,
        "calf raise" | "standing calf raise" | "standing calf-raise" => {
            RepsExercise::StandingCalfRaiseDumbbell
        }
        "seated calf raise" | "seated calf-raise" => RepsExercise::SeatedCalfRaiseMachine,
        "cg bench press" => RepsExercise::CloseGripBenchPressBarbell,
        // *"Must be a 10kg EZ-bar, but we don't distinguish, so barbell."*
        "bb curl" | "curls" | "curl" => RepsExercise::BicepCurlBarbell,
        "pause squat" => RepsExercise::PauseSquatBarbell,
        "cable cross over" => RepsExercise::CableCrossover,
        "clap press-up" => RepsExercise::ClapPushUp,
        "landmines" => RepsExercise::LandmineRotation,
        "bat wings" => RepsExercise::BatWings,
        "suitcase hold" => return Some(Exercise::Duration(DurationExercise::SuitcaseHold)),
        _ => return None,
    };
    Some(Exercise::Reps(reps))
}

/// Reps in reserve from an RPE, on the scale the Hevy adapter reads.
fn from_rpe(value: &Value) -> Option<Rir> {
    let Value::Number(rpe) = value else {
        return None;
    };
    match format!("{rpe}").as_str() {
        "10" => Some(Rir::Zero),
        "9.5" => Some(Rir::ZeroOrOne),
        "9" => Some(Rir::One),
        "8.5" => Some(Rir::OneOrTwo),
        "8" => Some(Rir::Two),
        "7.5" => Some(Rir::TwoOrThree),
        "7" => Some(Rir::Three),
        "6" => Some(Rir::FourOrMore),
        _ => None,
    }
}

/// Reps in reserve as the 2020 log wrote them. `3+` is *"loads"*, so four or
/// more.
fn from_note(value: &Value) -> Option<Rir> {
    match value.text()? {
        "0" => Some(Rir::Zero),
        "0/1" => Some(Rir::ZeroOrOne),
        "1" | "~1" => Some(Rir::One),
        "1/2" => Some(Rir::OneOrTwo),
        "2" | "~2" => Some(Rir::Two),
        "2/3" => Some(Rir::TwoOrThree),
        "3" | "~3" => Some(Rir::Three),
        "3+" | "4" | "4+" => Some(Rir::FourOrMore),
        _ => None,
    }
}

fn mass(value: &Value) -> Option<Kg> {
    match value {
        Value::Number(number) => Kg::try_from(number.to_string()).ok(),
        _ => None,
    }
}

fn count(value: &Value) -> Option<Count> {
    match value {
        Value::Number(number) => format!("{number}")
            .parse::<u32>()
            .ok()
            .and_then(|reps| RepCount::new(reps).ok())
            .map(Count::Reps),
        Value::Time(seconds) if *seconds > 0 => Some(Count::Held(Duration::from_seconds(*seconds))),
        _ => None,
    }
}

const fn rest(value: &Value) -> Option<Duration> {
    match value {
        Value::Time(seconds) if *seconds > 0 => Some(Duration::from_seconds(*seconds)),
        _ => None,
    }
}

fn is_filled(value: &Value) -> bool {
    match value {
        Value::Empty => false,
        Value::Text(text) => !text.trim().is_empty(),
        _ => true,
    }
}

fn at(sheet: &Sheet, row: u32, column: u32) -> Option<SheetCell> {
    Some(SheetCell {
        sheet: SheetName::try_from(sheet.name.as_str()).ok()?,
        cell: CellRef::at(row, column),
    })
}

/// A set of reps and a load, or nothing if neither is there.
///
/// A load with no count is kept as [`Count::Missing`], because on a working
/// set it means the session was not recorded as done. A load of zero with no
/// count is a blank, as a zero is for a weigh-in: `CT 2017`'s templates write
/// 0 kg into every set not yet done. Zero reps at a load is a failed attempt.
#[expect(clippy::too_many_arguments, reason = "one set's columns, named")]
fn entry(
    sheet: &Sheet,
    name: &str,
    row: u32,
    count_column: u32,
    load: Option<Kg>,
    kind: SetKind,
    intensity: Option<Rir>,
    rest_after: Option<Duration>,
) -> Option<Entry> {
    let written = sheet.get(row, count_column);
    let loaded = load.filter(|mass| !mass.is_none());
    let count = match written {
        Value::Number(reps) if *reps == 0.0 && loaded.is_some() => Count::Failed,
        _ => count(written).or_else(|| loaded.map(|_| Count::Missing))?,
    };
    Some(Entry {
        name: name.to_owned(),
        load,
        bodyweight: None,
        count,
        intensity,
        kind,
        rest_after,
        written_in: at(sheet, row, count_column)?,
    })
}

/// The column in `row` labelled `label`, looking from `from` up to `to`.
fn column(sheet: &Sheet, row: u32, label: &str, from: u32, to: u32) -> Option<u32> {
    (from..to).find(|&column| sheet.get(row, column).text() == Some(label))
}

const fn day(value: &Value) -> Option<Date> {
    match value {
        Value::Day(day) => Some(*day),
        _ => None,
    }
}

/// Keeps drafts that share a date together, in the order their dates appear.
struct ByDate<'drafts> {
    drafts: &'drafts mut Vec<Draft>,
    index: BTreeMap<Date, usize>,
}

impl<'drafts> ByDate<'drafts> {
    const fn new(drafts: &'drafts mut Vec<Draft>) -> Self {
        Self {
            drafts,
            index: BTreeMap::new(),
        }
    }

    /// The draft for `on`, begun if this is the first set seen for that day.
    fn on(&mut self, on: Date, what: impl FnOnce() -> String, press: Press) -> Option<&mut Draft> {
        let position = *self.index.entry(on).or_insert_with(|| {
            self.drafts.push(Draft {
                on,
                what: what(),
                performed: true,
                press,
                entries: Vec::new(),
            });
            self.drafts.len().saturating_sub(1)
        });
        self.drafts.get_mut(position)
    }
}

/// The 2019 session sheets: one sheet per session, dated in its name —
/// `Squat day (19-02-12)`, `Bench day (2019-02-14)`. The undated `Squat day`
/// beside them is the template, and is not a session.
fn session_sheet(sheet: &Sheet, drafts: &mut Vec<Draft>) {
    let Some(on) = dated_name(&sheet.name) else {
        return;
    };
    let width = sheet.width();
    let (Some(name), Some(reps)) = (
        column(sheet, 0, "Exercise", 0, width),
        column(sheet, 0, "Reps", 0, width),
    ) else {
        return;
    };
    let Some(load) =
        column(sheet, 0, "KG", 0, width).or_else(|| column(sheet, 0, "Weight", 0, width))
    else {
        return;
    };
    let rest = column(sheet, 0, "Rest", 0, width);
    let rpe = column(sheet, 0, "RPE", 0, width);
    let notes: Vec<u32> = ["RPE", "Decision", "Notes"]
        .iter()
        .filter_map(|label| column(sheet, 0, label, 0, width))
        .collect();

    let mut draft = Draft {
        on,
        what: sheet.name.clone(),
        performed: false,
        press: Press::Barbell,
        entries: Vec::new(),
    };
    for row in 1..sheet.height() {
        if notes
            .iter()
            .any(|&column| is_filled(sheet.get(row, column)))
        {
            draft.performed = true;
        }
        let Some(exercise) = sheet.get(row, name).text().filter(|text| !text.is_empty()) else {
            continue;
        };
        let intensity = rpe.and_then(|column| from_rpe(sheet.get(row, column)));
        let rest_after = rest.and_then(|column| self::rest(sheet.get(row, column)));
        if let Some(entry) = entry(
            sheet,
            exercise,
            row,
            reps,
            mass(sheet.get(row, load)),
            SetKind::Working,
            intensity,
            rest_after,
        ) {
            draft.entries.push(entry);
        }
    }
    drafts.push(draft);
}

/// The date in a session sheet's name: `(19-02-12)` or `(2019-02-14)`.
fn dated_name(name: &str) -> Option<Date> {
    let inside = name.split_once('(')?.1.split_once(')')?.0;
    let mut parts = inside.split('-');
    let year: i16 = parts.next()?.parse().ok()?;
    let month: i8 = parts.next()?.parse().ok()?;
    let day: i8 = parts.next()?.parse().ok()?;
    let year = if year < 100 {
        year.checked_add(2000)?
    } else {
        year
    };
    Date::new(year, month, day).ok()
}

/// 2016's `Weights` (`Training 2015_16`): a block per exercise down the sheet,
/// its name above a `Date, S1W, S1R, …` header, a row per session. The same
/// date across blocks is one session.
fn weights_2016(sheet: &Sheet, drafts: &mut Vec<Draft>) {
    let mut sessions = ByDate::new(drafts);
    let width = sheet.width();
    for header in 1..sheet.height() {
        if sheet.get(header, 0).text() != Some("Date") || sheet.get(header, 1).text() != Some("S1W")
        {
            continue;
        }
        let Some(name) = sheet.get(header.saturating_sub(1), 0).text() else {
            continue;
        };
        let sets: Vec<(u32, u32)> = (1..=9)
            .filter_map(|set| {
                Some((
                    column(sheet, header, &format!("S{set}W"), 0, width)?,
                    column(sheet, header, &format!("S{set}R"), 0, width)?,
                ))
            })
            .collect();
        for row in header.saturating_add(1)..sheet.height() {
            let Some(on) = day(sheet.get(row, 0)) else {
                break;
            };
            let Some(draft) = sessions.on(on, || format!("{}, {on}", sheet.name), Press::Barbell)
            else {
                continue;
            };
            for &(load, reps) in &sets {
                if let Some(entry) = entry(
                    sheet,
                    name,
                    row,
                    reps,
                    mass(sheet.get(row, load)),
                    SetKind::Working,
                    None,
                    None,
                ) {
                    draft.entries.push(entry);
                }
            }
        }
    }
}

/// `workout.xlsx`'s `Training Log`, June 2018: a row per exercise, its sets
/// across as `Set 1 reps, Set 1 Kg, Set 1 RPE, Set 1 Rest`.
fn training_log(sheet: &Sheet, drafts: &mut Vec<Draft>) {
    let width = sheet.width();
    if column(sheet, 0, "Set 1 reps", 0, width).is_none() {
        return;
    }
    let (Some(date), Some(name)) = (
        column(sheet, 0, "Date", 0, width),
        column(sheet, 0, "Exercise", 0, width),
    ) else {
        return;
    };
    let sets: Vec<(u32, u32, Option<u32>, Option<u32>)> = (1..=9)
        .filter_map(|set| {
            Some((
                column(sheet, 0, &format!("Set {set} reps"), 0, width)?,
                column(sheet, 0, &format!("Set {set} Kg"), 0, width)?,
                column(sheet, 0, &format!("Set {set} RPE"), 0, width),
                column(sheet, 0, &format!("Set {set} Rest"), 0, width),
            ))
        })
        .collect();
    let mut sessions = ByDate::new(drafts);
    for row in 1..sheet.height() {
        let (Some(on), Some(exercise)) = (day(sheet.get(row, date)), sheet.get(row, name).text())
        else {
            continue;
        };
        let Some(draft) = sessions.on(on, || format!("{}, {on}", sheet.name), Press::Barbell)
        else {
            continue;
        };
        for &(reps, load, rpe, rest) in &sets {
            if let Some(entry) = entry(
                sheet,
                exercise,
                row,
                reps,
                mass(sheet.get(row, load)),
                SetKind::Working,
                rpe.and_then(|column| from_rpe(sheet.get(row, column))),
                rest.and_then(|column| self::rest(sheet.get(row, column))),
            ) {
                draft.entries.push(entry);
            }
        }
    }
}

/// `the_beginner_prescription`, October–November 2020: a row per set, a
/// session per `week_ending` and `Day`.
///
/// **No load is read.** Every `kg` is a formula whose lookup came back empty,
/// so the loads the sheet shows are zeros and what the zeros imply; the
/// operator put the real ones on Garmin (#278). **Its RPE column is the
/// target**, not a reading; what he recorded is the reps in reserve in
/// `Column1` and notes in `Column2`, and those are the evidence. Day 1, 2 and 3
/// are Monday, Wednesday and Friday of the week ending on `week_ending`.
fn beginner_2020(sheet: &Sheet, drafts: &mut Vec<Draft>) {
    let width = sheet.width();
    if column(sheet, 0, "week_ending", 0, width).is_none() {
        return;
    }
    let (Some(week), Some(day_of_week), Some(name), Some(kind), Some(reps)) = (
        column(sheet, 0, "week_ending", 0, width),
        column(sheet, 0, "Day", 0, width),
        column(sheet, 0, "Exercise", 0, width),
        column(sheet, 0, "Type", 0, width),
        column(sheet, 0, "Reps", 0, width),
    ) else {
        return;
    };
    let reserve = column(sheet, 0, "Column1", 0, width);
    let note = column(sheet, 0, "Column2", 0, width);

    let mut keys: BTreeMap<(Date, u32), usize> = BTreeMap::new();
    for row in 1..sheet.height() {
        let (Some(ending), Some(exercise)) =
            (day(sheet.get(row, week)), sheet.get(row, name).text())
        else {
            continue;
        };
        let Some(training_day) = count(sheet.get(row, day_of_week)).and_then(|count| match count {
            Count::Reps(day) => Some(day.as_u32()),
            _ => None,
        }) else {
            continue;
        };
        let offset = i64::from(training_day.saturating_sub(1)).saturating_mul(2);
        let Ok(on) = ending
            .checked_sub(6.days())
            .and_then(|monday| monday.checked_add(offset.days()))
        else {
            continue;
        };
        let position = *keys.entry((ending, training_day)).or_insert_with(|| {
            drafts.push(Draft {
                on,
                what: format!("{}, week ending {ending}, day {training_day}", sheet.name),
                performed: false,
                press: Press::Dumbbell,
                entries: Vec::new(),
            });
            drafts.len().saturating_sub(1)
        });
        let Some(draft) = drafts.get_mut(position) else {
            continue;
        };
        let evidence = [reserve, note]
            .into_iter()
            .flatten()
            .any(|column| is_filled(sheet.get(row, column)));
        draft.performed |= evidence;
        let set_kind = if sheet.get(row, kind).text() == Some("warm up") {
            SetKind::Warmup
        } else {
            SetKind::Working
        };
        if let Some(entry) = entry(
            sheet,
            exercise,
            row,
            reps,
            None,
            set_kind,
            reserve.and_then(|column| from_note(sheet.get(row, column))),
            None,
        ) {
            draft.entries.push(entry);
        }
    }
}

/// `strength_training`'s `Sheet1`, October 2023: a set per row, each fed to an
/// e1RM estimate, so each is a set that was lifted.
fn estimates_2023(sheet: &Sheet, drafts: &mut Vec<Draft>) {
    let width = sheet.width();
    if column(sheet, 0, "E1RM", 0, width).is_none() {
        return;
    }
    let (Some(date), Some(name), Some(reps), Some(load)) = (
        column(sheet, 0, "Date", 0, width),
        column(sheet, 0, "Exercise", 0, width),
        column(sheet, 0, "Reps", 0, width),
        column(sheet, 0, "Weight", 0, width),
    ) else {
        return;
    };
    let mut sessions = ByDate::new(drafts);
    for row in 1..sheet.height() {
        let (Some(on), Some(exercise)) = (day(sheet.get(row, date)), sheet.get(row, name).text())
        else {
            continue;
        };
        let Some(draft) = sessions.on(on, || format!("{}, {on}", sheet.name), Press::Barbell)
        else {
            continue;
        };
        if let Some(entry) = entry(
            sheet,
            exercise,
            row,
            reps,
            mass(sheet.get(row, load)),
            SetKind::Working,
            None,
            None,
        ) {
            draft.entries.push(entry);
        }
    }
}

/// `1RM`'s `Deadlift`, `Bench` and `Squat` sheets, April–June 2018: a row per
/// session on each. `Squat` has three sets; `Deadlift` and `Bench` have one,
/// the last set of the day, whose reps feed a 1RM estimate (operator,
/// 2026-09-27). The same date across the three is one session, in the order
/// the sheets sit in the workbook.
fn lift_sheets_2018(sheets: &[Sheet], drafts: &mut Vec<Draft>) {
    let mut sessions = ByDate::new(drafts);
    for sheet in sheets {
        let width = sheet.width();
        let Some(header) = (0..3).find(|&row| column(sheet, row, "Date", 0, width).is_some())
        else {
            continue;
        };
        let Some(date) = column(sheet, header, "Date", 0, width) else {
            continue;
        };
        let sets: Vec<(u32, u32)> = if column(sheet, header, "Set 1 Kg", 0, width).is_some() {
            (1..=9)
                .filter_map(|set| {
                    Some((
                        column(sheet, header, &format!("Set {set} Kg"), 0, width)?,
                        column(sheet, header, &format!("Set {set} Reps"), 0, width)?,
                    ))
                })
                .collect()
        } else {
            match (
                column(sheet, header, "kg", 0, width),
                column(sheet, header, "reps", 0, width),
                column(sheet, header, "1RM kg", 0, width),
            ) {
                (Some(load), Some(reps), Some(_)) => vec![(load, reps)],
                _ => continue,
            }
        };
        if sets.is_empty() {
            continue;
        }
        for row in header.saturating_add(1)..sheet.height() {
            let Some(on) = day(sheet.get(row, date)) else {
                continue;
            };
            let Some(draft) = sessions.on(on, || format!("1RM sheets, {on}"), Press::Barbell)
            else {
                continue;
            };
            for &(load, reps) in &sets {
                if let Some(entry) = entry(
                    sheet,
                    &sheet.name,
                    row,
                    reps,
                    mass(sheet.get(row, load)),
                    SetKind::Working,
                    None,
                    None,
                ) {
                    draft.entries.push(entry);
                }
            }
        }
    }
}

/// Which of the programme's days a `CT 2017` sheet is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Day {
    Push,
    Legs,
    Pull,
}

/// One `CT 2017` workout: a row of `Push`, `Legs` or `Pull`.
struct Workout {
    day: Day,
    week: i8,
    draft: Draft,
}

/// `CT 2017`'s `Push`, `Legs`, `Pull` and `Trunk` sheets, January–April 2017.
///
/// Each row of `Push`, `Legs` or `Pull` is a workout, in blocks across the
/// sheet, one per exercise, each with warm-ups, a planned column and the
/// performed `SET1`–`SET3`. A row with nothing performed is the plan.
///
/// **The day is the programme's** (operator, 2026-09-27): the sheets give the
/// week, and the second pattern on `Weekly` puts push on Monday, legs on
/// Wednesday and pull on Friday. A week with two pushes and no legs ran push,
/// pull, push on the same days. `Trunk` is two rows per exercise per week, done
/// on the days `Weekly` puts it: suitcase holds with push and pull, landmines
/// with push and legs, loaded dead bugs with legs and pull.
fn conditioning_2017(sheets: &[Sheet], drafts: &mut Vec<Draft>, scribe: &mut Scribe) {
    let mut workouts: Vec<Workout> = Vec::new();
    for sheet in sheets {
        let day = match sheet.name.as_str() {
            "Push" => Day::Push,
            "Legs" => Day::Legs,
            "Pull" => Day::Pull,
            _ => continue,
        };
        for row in 1..sheet.height() {
            let mut entries = Vec::new();
            let mut week = None;
            let mut performed = false;
            for block in blocks(sheet) {
                let Some(name) = block.name(sheet, row) else {
                    continue;
                };
                let Some(this_week) = week_of(sheet.get(row, block.week)) else {
                    continue;
                };
                week = week.or(Some(this_week));
                performed |= block
                    .sets
                    .iter()
                    .any(|&(reps, _)| is_filled(sheet.get(row, reps)));
                entries.extend(block.entries(sheet, row, &name));
            }
            let Some(week) = week else {
                continue;
            };
            let Some(monday) = monday_of_2017(week) else {
                continue;
            };
            workouts.push(Workout {
                day,
                week,
                draft: Draft {
                    on: monday,
                    what: format!("{}, week {week}", sheet.name),
                    performed,
                    press: Press::Barbell,
                    entries,
                },
            });
        }
    }
    if workouts.is_empty() {
        return;
    }

    date_by_the_programme(&mut workouts, scribe);

    trunk_2017(sheets, &mut workouts, scribe);

    drafts.extend(
        workouts
            .into_iter()
            .filter(|workout| workout.draft.performed || !workout.draft.entries.is_empty())
            .map(|workout| workout.draft),
    );
}

/// Each performed workout onto its programme day: push Monday, legs
/// Wednesday, pull Friday, and push, pull, push when a week has two pushes and
/// no legs. A week the programme cannot hold is refused.
fn date_by_the_programme(workouts: &mut [Workout], scribe: &mut Scribe) {
    let mut weeks: BTreeMap<i8, Vec<usize>> = BTreeMap::new();
    for (index, workout) in workouts.iter().enumerate() {
        if workout.draft.performed {
            weeks.entry(workout.week).or_default().push(index);
        }
    }
    for indices in weeks.values() {
        let of = |day: Day| -> Vec<usize> {
            indices
                .iter()
                .copied()
                .filter(|&index| {
                    workouts
                        .get(index)
                        .is_some_and(|workout| workout.day == day)
                })
                .collect()
        };
        let (pushes, legs, pulls) = (of(Day::Push), of(Day::Legs), of(Day::Pull));
        let days: Vec<(usize, i64)> = match (pushes.as_slice(), legs.as_slice(), pulls.as_slice()) {
            ([first, second], [], pull) if pull.len() <= 1 => {
                let mut days = vec![(*first, 0), (*second, 4)];
                days.extend(pull.iter().map(|&index| (index, 2)));
                days
            }
            (push, legs, pull) if push.len() <= 1 && legs.len() <= 1 && pull.len() <= 1 => push
                .iter()
                .map(|&index| (index, 0))
                .chain(legs.iter().map(|&index| (index, 2)))
                .chain(pull.iter().map(|&index| (index, 4)))
                .collect(),
            _ => {
                for &index in indices {
                    if let Some(workout) = workouts.get_mut(index) {
                        workout.draft.performed = false;
                        workout.draft.entries.clear();
                        unmodelled(
                            scribe,
                            format!(
                                "{}: more workouts in the week than the programme has days",
                                workout.draft.what
                            ),
                        );
                    }
                }
                continue;
            }
        };
        for (index, offset) in days {
            if let Some(workout) = workouts.get_mut(index)
                && let Ok(on) = workout.draft.on.checked_add(offset.days())
            {
                workout.draft.on = on;
            }
        }
    }
}

/// The trunk work, onto the workouts `Weekly` puts it with.
fn trunk_2017(sheets: &[Sheet], workouts: &mut [Workout], scribe: &mut Scribe) {
    let Some(sheet) = sheets.iter().find(|sheet| sheet.name == "Trunk") else {
        return;
    };
    for block in blocks(sheet) {
        let mut seen: BTreeMap<(i8, String), usize> = BTreeMap::new();
        for row in 1..sheet.height() {
            let Some(name) = block.name(sheet, row) else {
                continue;
            };
            let Some(week) = week_of(sheet.get(row, block.week)) else {
                continue;
            };
            if !block
                .sets
                .iter()
                .any(|&(reps, _)| is_filled(sheet.get(row, reps)))
            {
                continue;
            }
            let nth = seen.entry((week, name.clone())).or_insert(0);
            let with = match (name.to_lowercase().as_str(), *nth) {
                ("suitcase hold" | "landmines", 0) => Some(Day::Push),
                ("landmines", 1) | ("loaded deadbug", 0) => Some(Day::Legs),
                ("suitcase hold" | "loaded deadbug", 1) => Some(Day::Pull),
                _ => None,
            };
            *nth = nth.saturating_add(1);
            let workout = with.and_then(|day| {
                workouts.iter_mut().find(|workout| {
                    workout.day == day && workout.week == week && workout.draft.performed
                })
            });
            let Some(workout) = workout else {
                unmodelled(
                    scribe,
                    format!("Trunk, week {week}: {name} has no workout to have been done in"),
                );
                continue;
            };
            workout
                .draft
                .entries
                .extend(block.entries(sheet, row, &name));
        }
    }
}

/// One exercise's columns on a `CT 2017` sheet.
struct Block {
    week: u32,
    /// Where the exercise is named: in a column per row, or once in the header.
    name: BlockName,
    bodyweight: Option<u32>,
    warmups: Vec<(u32, u32)>,
    sets: Vec<(u32, u32)>,
}

enum BlockName {
    Column(u32),
    Header(String),
}

impl Block {
    fn name(&self, sheet: &Sheet, row: u32) -> Option<String> {
        match &self.name {
            BlockName::Column(column) => sheet
                .get(row, *column)
                .text()
                .filter(|text| !text.is_empty())
                .map(str::to_owned),
            BlockName::Header(name) => Some(name.clone()),
        }
    }

    fn entries(&self, sheet: &Sheet, row: u32, name: &str) -> Vec<Entry> {
        let bodyweight = self
            .bodyweight
            .and_then(|column| mass(sheet.get(row, column)));
        let warmups = self.warmups.iter().map(|&set| (SetKind::Warmup, set));
        let sets = self.sets.iter().map(|&set| (SetKind::Working, set));
        warmups
            .chain(sets)
            .filter_map(|(kind, (reps, load))| {
                let mut entry = entry(
                    sheet,
                    name,
                    row,
                    reps,
                    mass(sheet.get(row, load)),
                    kind,
                    None,
                    None,
                )?;
                entry.bodyweight = bodyweight;
                Some(entry)
            })
            .collect()
    }
}

/// Every exercise block on a `CT 2017` sheet, found by its `Wk` column.
///
/// The name is in the header above the workout number (`Squat\nWork Out`), or
/// in a column of its own before it (`Horizontal Push` holding `Bench`), or,
/// on `Trunk`, in the column before `Wk`.
fn blocks(sheet: &Sheet) -> Vec<Block> {
    let width = sheet.width();
    let starts: Vec<u32> = (1..width)
        .filter(|&column| {
            sheet
                .get(0, column)
                .text()
                .is_some_and(|label| label.starts_with("Wk"))
        })
        .collect();
    let mut blocks = Vec::new();
    for (index, &week) in starts.iter().enumerate() {
        let end = starts
            .get(index.saturating_add(1))
            .copied()
            .unwrap_or(width);
        let before = sheet
            .get(0, week.saturating_sub(1))
            .text()
            .unwrap_or_default();
        let name = if before == "Work Out" {
            BlockName::Column(week.saturating_sub(2))
        } else if let Some((name, _)) = before.split_once('\n') {
            BlockName::Header(name.trim().to_owned())
        } else {
            BlockName::Column(week.saturating_sub(1))
        };
        let pair = |reps: &str, load: &str| {
            Some((
                column(sheet, 0, reps, week, end)?,
                column(sheet, 0, load, week, end)?,
            ))
        };
        blocks.push(Block {
            week,
            name,
            bodyweight: column(sheet, 0, "BW", week, end),
            warmups: (1..=3)
                .filter_map(|set| pair(&format!("WU{set}(reps)"), &format!("WU{set}(kg)")))
                .collect(),
            sets: (1..=3)
                .filter_map(|set| pair(&format!("SET{set}(reps)"), &format!("SET{set}(kg)")))
                .collect(),
        });
    }
    blocks
}

fn week_of(value: &Value) -> Option<i8> {
    match count(value)? {
        Count::Reps(week) => i8::try_from(week.as_u32()).ok(),
        _ => None,
    }
}

/// The Monday of an ISO week of 2017, the year `CT 2017` was run.
fn monday_of_2017(week: i8) -> Option<Date> {
    jiff::civil::ISOWeekDate::new(2017, week, Weekday::Monday)
        .ok()
        .map(jiff::civil::ISOWeekDate::date)
}

fn unmodelled(scribe: &mut Scribe, detail: String) {
    scribe.note(RefusalLocus::Record, RefusalReason::Unmodelled { detail });
}
