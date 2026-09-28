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

use std::{collections::BTreeMap, fmt};

use application::{NormalisationError, Translation, ports::Translator};
use domain::{
    gym::{
        Load, ManualExercise, ManualGymSession, ManualItem, ManualSet, Performed, Rir, SetKind,
        SignedKg,
        exercise::{DistanceExercise, DurationExercise, Exercise, Implement, RepsExercise},
    },
    landing::{CellRef, SheetCell, SheetName},
    measure::{Duration, Kg, Metres, RepCount},
    normalised::{OperatorZone, RefusalLocus, RefusalReason},
    sequence::NonEmpty,
};
use jiff::{
    ToSpan as _,
    civil::{Date, Weekday},
};

use super::{
    Opened, Workbook,
    sheet::{Sheet, Value},
};
use crate::scribe::Scribe;

/// Reads manual gym sessions out of a landed spreadsheet.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpreadsheetSessionTranslator;

impl Translator for SpreadsheetSessionTranslator {
    type Account = Workbook;
    type Entity = ManualGymSession;

    /// The zone is not consulted: a manual session is a day.
    fn translate(
        &self,
        workbook: &Workbook,
        _zone: &OperatorZone,
    ) -> Result<Translation<ManualGymSession>, NormalisationError> {
        super::translate_with(workbook, "gym sessions", |copies, scribes| {
            scribes
                .last_mut()
                .map_or_else(|| Ok(Vec::new()), |scribe| read_sessions(copies, scribe))
        })
    }
}

/// Every performed session in a workbook, merged across its copies.
///
/// `copies` is oldest first. Each copy is read on its own, and then the copies
/// are merged (operator, 2026-09-28): a session any copy shows performed
/// derives once, every set any copy states is kept, and where two copies state
/// different values for one set the most recent wins. A blank is not a
/// statement, so it never beats a value.
///
/// # Errors
///
/// [`NormalisationError::UnmappedExercise`] for a name the vocabulary does not
/// map.
pub(super) fn read_sessions(
    copies: &[Opened<'_>],
    scribe: &mut Scribe,
) -> Result<Vec<ManualGymSession>, NormalisationError> {
    let mut read = Vec::new();
    for (index, copy) in copies.iter().enumerate() {
        let mut drafts = Vec::new();
        for sheet in &copy.sheets {
            session_sheet(sheet, &mut drafts);
            weights_2016(sheet, &mut drafts);
            training_log(sheet, &mut drafts);
            beginner_2020(sheet, &mut drafts);
            estimates_2023(sheet, &mut drafts);
        }
        lift_sheets_2018(&copy.sheets, &mut drafts);
        for draft in &mut drafts {
            from_copy(draft, index);
        }
        read.push(drafts);
    }
    let mut drafts = merge_by_date(read);
    drafts.extend(conditioning_2017(copies, scribe));

    let mut sessions = Vec::new();
    for draft in drafts {
        if let Some(session) = finish(draft, copies, scribe)? {
            sessions.push(session);
        }
    }
    Ok(sessions)
}

/// Mark a draft, and every set in it, as read from one copy.
fn from_copy(draft: &mut Draft, copy: usize) {
    draft.copy = copy;
    for entry in &mut draft.entries {
        entry.copy = copy;
    }
}

/// The copies' sessions, one per sheet and day, however many copies hold it.
///
/// `read` is each copy's drafts, oldest copy first.
fn merge_by_date(read: Vec<Vec<Draft>>) -> Vec<Draft> {
    let mut merged: Vec<Draft> = Vec::new();
    for drafts in read.into_iter().rev() {
        for draft in drafts {
            let same = merged
                .iter_mut()
                .find(|kept| kept.what == draft.what && kept.on == draft.on);
            match same {
                Some(kept) => absorb(kept, draft),
                None => merged.push(draft),
            }
        }
    }
    merged
}

/// Fold an older copy's draft of a session into a newer copy's.
///
/// Only a copy that shows the session performed contributes sets: a copy
/// holding it as a plan says nothing about what was done.
fn absorb(newer: &mut Draft, older: Draft) {
    if !older.performed {
        return;
    }
    if !newer.performed {
        newer.performed = true;
        newer.copy = older.copy;
        newer.entries = older.entries;
        return;
    }
    let press = newer.press;
    newer.entries = merge_sets(std::mem::take(&mut newer.entries), older.entries, press);
}

/// One exercise's set of one kind, at one place in the layout: the unit two
/// copies are compared on.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Slot {
    exercise: String,
    kind: SetKind,
    position: u32,
}

fn slots(entries: &[Entry], press: Press) -> Vec<Slot> {
    let mut seen: BTreeMap<(String, bool), u32> = BTreeMap::new();
    entries
        .iter()
        .map(|entry| {
            let exercise = exercise_named(&entry.name, press).map_or_else(
                || entry.name.to_lowercase(),
                |exercise| exercise.to_string(),
            );
            let ordinal = seen
                .entry((exercise.clone(), entry.kind == SetKind::Warmup))
                .or_insert(0);
            let position = entry.position.unwrap_or(*ordinal);
            *ordinal = ordinal.saturating_add(1);
            Slot {
                exercise,
                kind: entry.kind,
                position,
            }
        })
        .collect()
}

/// Whether a set says anything. A count missing beside a load, and a warm-up
/// of nothing on an exercise never done with nothing, are blanks.
fn states(entry: &Entry, press: Press) -> bool {
    if matches!(entry.count, Count::Missing) {
        return false;
    }
    exercise_named(&entry.name, press).is_none_or(|exercise| !is_empty_bar(exercise, entry))
}

/// Two lists of one session's sets as one: where both have a set, the one
/// from the more recent copy, unless it is a blank; where only `added` has
/// one, it goes where `added` put it, after the set it followed there or
/// before the one it preceded.
fn merge_sets(kept: Vec<Entry>, added: Vec<Entry>, press: Press) -> Vec<Entry> {
    let mut keys = slots(&kept, press);
    let mut merged = kept;
    let added_keys = slots(&added, press);
    for (index, (key, entry)) in added_keys.iter().zip(added).enumerate() {
        if let Some(at) = keys.iter().position(|kept| kept == key) {
            if let Some(kept) = merged.get_mut(at) {
                let replaces = match (states(kept, press), states(&entry, press)) {
                    (false, true) => true,
                    (true, true) => entry.copy > kept.copy,
                    _ => false,
                };
                if replaces {
                    *kept = entry;
                }
            }
            continue;
        }
        let after = added_keys
            .get(..index)
            .unwrap_or_default()
            .iter()
            .rev()
            .find_map(|before| keys.iter().position(|kept| kept == before))
            .map(|at| at.saturating_add(1));
        let before = || {
            added_keys
                .get(index.saturating_add(1)..)
                .unwrap_or_default()
                .iter()
                .find_map(|next| keys.iter().position(|kept| kept == next))
        };
        let at = after.or_else(before).unwrap_or(merged.len());
        keys.insert(at, key.clone());
        merged.insert(at, entry);
    }
    merged
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
    /// Which copy of the workbook it was read from, oldest first.
    copy: usize,
    /// Where the layout puts it among its exercise's sets of its kind (`WU2`,
    /// `SET3`), where the layout numbers them. Copies of a workbook are
    /// merged set by set, and a blank in one copy must not shift the rest.
    position: Option<u32>,
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
    /// The copy it is dated by: the most recent that holds it.
    copy: usize,
}

/// A draft becomes a session, or says why not.
fn finish(
    draft: Draft,
    copies: &[Opened<'_>],
    scribe: &mut Scribe,
) -> Result<Option<ManualGymSession>, NormalisationError> {
    let Draft {
        on,
        what,
        performed,
        press,
        entries,
        copy,
    } = draft;
    let Some(dated_by) = copies.get(copy) else {
        return Ok(None);
    };

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
            let source_record_id = copies
                .get(entry.copy)
                .unwrap_or(dated_by)
                .record
                .source_record_id()
                .to_string();
            return Err(NormalisationError::UnmappedExercise {
                template_id: entry.name,
                source_record_id,
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
        if let Some(exercise) = performed_exercise(exercise, sets, copies, &what, scribe) {
            built.push(ManualItem::Exercise(exercise));
        }
    }
    let Ok(items) = NonEmpty::new(built) else {
        unmodelled(scribe, format!("{what} ({on}) records no set"));
        return Ok(None);
    };
    // The copy it is dated by first, then every other copy a set came from.
    let mut drawn_from = vec![dated_by.logged()];
    for item in &items {
        for copy in item.exercises().flat_map(ManualExercise::copies) {
            if !drawn_from.iter().any(|logged| logged.landed_as == copy)
                && let Some(opened) = copies.iter().find(|opened| opened.record.id() == copy)
            {
                drawn_from.push(opened.logged());
            }
        }
    }
    let Ok(drawn_from) = NonEmpty::new(drawn_from) else {
        return Ok(None);
    };
    Ok(Some(ManualGymSession::new(on, drawn_from, items)))
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
    copies: &[Opened<'_>],
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
                if let Some(set) = manual_set(Exercise::Reps(exercise), entry, outcome, copies) {
                    sets.push(set);
                }
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
                if let Some(set) = manual_set(Exercise::Duration(exercise), entry, outcome, copies)
                {
                    sets.push(set);
                }
            }
            NonEmpty::new(sets)
                .ok()
                .map(|sets| ManualExercise::ForDuration { exercise, sets })
        }
        // Counted in carries of a known length: `CT 2017`'s carries.
        Exercise::Distance(exercise) => {
            let mut sets = Vec::new();
            for entry in entries {
                let outcome = match entry.count {
                    Count::Reps(carries) => Performed::Completed(Metres::from_millimetres(
                        CARRY
                            .as_millimetres()
                            .saturating_mul(u64::from(carries.as_u32())),
                    )),
                    Count::Failed => Performed::Failed,
                    Count::Held(_) | Count::Missing => {
                        unmodelled(
                            scribe,
                            format!(
                                "{what}: {} at {} is not a carry",
                                exercise, entry.written_in
                            ),
                        );
                        continue;
                    }
                };
                if let Some(set) = manual_set(Exercise::Distance(exercise), entry, outcome, copies)
                {
                    sets.push(set);
                }
            }
            NonEmpty::new(sets)
                .ok()
                .map(|sets| ManualExercise::ForDistance { exercise, sets })
        }
    }
}

fn manual_set<M>(
    exercise: Exercise,
    entry: Entry,
    outcome: Performed<M>,
    copies: &[Opened<'_>],
) -> Option<ManualSet<M>> {
    let copy = copies.get(entry.copy)?.record.id();
    Some(ManualSet {
        load: entry
            .load
            .map(|mass| load_of(exercise, mass, entry.bodyweight)),
        outcome: match outcome {
            Performed::Completed(measure) => Performed::Completed(Some(measure)),
            Performed::Failed => Performed::Failed,
        },
        intensity: entry.intensity,
        kind: entry.kind,
        rest_after: entry.rest_after,
        copy,
        at: entry.written_in,
    })
}

/// A load on the axis its exercise is loaded on.
///
/// The exercises assistance is conventionally given on are relative to
/// bodyweight, as the Hevy adapter reads them. A sheet writes the weight added,
/// so 0 is plain bodyweight — except that `CT 2017` writes the body weight
/// itself, which is plain bodyweight too.
pub fn load_of(exercise: Exercise, mass: Kg, bodyweight: Option<Kg>) -> Load {
    let relative = matches!(
        exercise,
        Exercise::Reps(
            RepsExercise::ChestDip
                | RepsExercise::RingDip
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
        "suitcase carry" => return Some(Exercise::Distance(DistanceExercise::SuitcaseCarry)),
        "farmers walk" => return Some(Exercise::Distance(DistanceExercise::FarmersWalk)),
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
        // The May copy of `CT 2017` times its later carries in words: `20secs`.
        Value::Text(text) => text
            .trim()
            .strip_suffix("secs")
            .and_then(|seconds| seconds.trim().parse::<u64>().ok())
            .filter(|&seconds| seconds > 0)
            .map(|seconds| Count::Held(Duration::from_seconds(seconds))),
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
        copy: 0,
        position: None,
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
                copy: 0,
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
        copy: 0,
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
                copy: 0,
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

/// Which of the programme's days a `CT 2017` workout is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Day {
    Push,
    Legs,
    Pull,
}

impl fmt::Display for Day {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Push => "Push",
            Self::Legs => "Legs",
            Self::Pull => "Pull",
        })
    }
}

/// One `CT 2017` workout: the Nth of its day.
struct Workout {
    day: Day,
    number: u32,
    /// The calendar week, where a copy says it. The first layout's `Wk` is the
    /// template's week, which the programme ran a week and more behind, so it
    /// is not read as one.
    week: Option<i8>,
    /// The body weight written beside it: what ties the earlier copies' trunk
    /// work to it.
    bodyweight: Option<Kg>,
    draft: Draft,
}

/// `CT 2017`, January–April 2017, merged across its copies.
///
/// **Two layouts.** The final workbook and the May copy keep a sheet per day
/// (`Push`, `Legs`, `Pull`), a workout per row and an exercise per block, each
/// with warm-ups, a planned column and the performed `SET1`–`SET3`. The March
/// and April copies keep a sheet per lift (`Squat`, `Bench`, …), whose row N is
/// the Nth workout of that lift's day. A row with nothing performed is the plan.
///
/// **The copies are one workbook** (operator, 2026-09-28), so a workout is its
/// day and number, whichever copy and layout it is read from, and the merge
/// keeps every set any copy states, the most recent copy winning.
///
/// **The day is the programme's** (operator, 2026-09-27): the sheets give the
/// week, and the second pattern on the final `Weekly` puts push on Monday, legs
/// on Wednesday and pull on Friday. A week with two pushes and no legs ran
/// push, pull, push on the same days. The final `Trunk` is two rows per
/// exercise per week, done on the days `Weekly` puts it; the earlier copies'
/// trunk sheets are joined by [`trunk_before_week_14`].
fn conditioning_2017(copies: &[Opened<'_>], scribe: &mut Scribe) -> Vec<Draft> {
    let read: Vec<Vec<Workout>> = copies
        .iter()
        .enumerate()
        .map(|(copy, opened)| workouts_2017(&opened.sheets, copy))
        .collect();
    let mut workouts = merge_workouts(read);
    if workouts.is_empty() {
        return Vec::new();
    }

    workouts.retain_mut(|workout| {
        let Some(week) = workout.week else {
            if workout.draft.performed {
                unmodelled(
                    scribe,
                    format!("{}: no copy says which week it was", workout.draft.what),
                );
            }
            return false;
        };
        match monday_of_2017(week) {
            Some(monday) => {
                workout.draft.on = monday;
                true
            }
            None => false,
        }
    });

    date_by_the_programme(&mut workouts, scribe);

    if let Some((copy, opened)) = copies
        .iter()
        .enumerate()
        .rev()
        .find(|(_, opened)| opened.sheets.iter().any(|sheet| sheet.name == "Trunk"))
    {
        trunk_2017(&opened.sheets, copy, &mut workouts, scribe);
    }
    trunk_before_week_14(copies, &mut workouts, scribe);

    workouts
        .into_iter()
        .filter(|workout| workout.draft.performed || !workout.draft.entries.is_empty())
        .map(|workout| workout.draft)
        .collect()
}

/// One copy's workouts, in either layout.
fn workouts_2017(sheets: &[Sheet], copy: usize) -> Vec<Workout> {
    let mut workouts: Vec<Workout> = Vec::new();
    for sheet in sheets {
        let (day, lift) = match sheet.name.as_str() {
            "Push" => (Day::Push, None),
            "Legs" => (Day::Legs, None),
            "Pull" => (Day::Pull, None),
            "Squat" => (Day::Legs, Some("Squat")),
            "Deadlift" => (Day::Legs, Some("Deadlift")),
            "Lunge" => (Day::Legs, Some("Lunge")),
            "Bench" => (Day::Push, Some("Bench")),
            "Press" => (Day::Push, Some("Press")),
            "Dips" => (Day::Push, Some("Dips")),
            "Pullup" => (Day::Pull, Some("Pull up")),
            "Row" => (Day::Pull, Some("Row")),
            "Curl" => (Day::Pull, Some("Curl")),
            _ => continue,
        };
        let blocks = lift.map_or_else(
            || blocks(sheet),
            |name| lift_sheet(sheet, name).into_iter().collect(),
        );
        for row in 1..sheet.height() {
            let mut number = None;
            let mut week = None;
            let mut bodyweight = None;
            let mut performed = false;
            let mut entries = Vec::new();
            for block in &blocks {
                let Some(name) = block.name(sheet, row) else {
                    continue;
                };
                let Some(this_number) = block.number(sheet, row) else {
                    continue;
                };
                number = number.or(Some(this_number));
                week = week.or_else(|| block.week(sheet, row));
                bodyweight = bodyweight.or_else(|| block.bodyweight(sheet, row));
                performed |= block
                    .sets
                    .iter()
                    .any(|&(reps, _)| is_filled(sheet.get(row, reps)));
                entries.extend(block.entries(sheet, row, &name, copy));
            }
            let Some(number) = number else {
                continue;
            };
            match workouts
                .iter_mut()
                .find(|workout| workout.day == day && workout.number == number)
            {
                // A sheet per lift: the workout is already begun by an earlier
                // lift's sheet.
                Some(workout) => {
                    workout.week = workout.week.or(week);
                    workout.bodyweight = workout.bodyweight.or(bodyweight);
                    if performed {
                        workout.draft.performed = true;
                        workout.draft.entries.extend(entries);
                    }
                }
                None => workouts.push(Workout {
                    day,
                    number,
                    week,
                    bodyweight,
                    draft: Draft {
                        on: Date::MIN,
                        what: format!("{day}, workout {number}"),
                        performed,
                        press: Press::Barbell,
                        entries: if performed || lift.is_none() {
                            entries
                        } else {
                            Vec::new()
                        },
                        copy,
                    },
                }),
            }
        }
    }
    workouts
}

/// Every copy's workouts as one, the most recent copy first.
fn merge_workouts(read: Vec<Vec<Workout>>) -> Vec<Workout> {
    let mut merged: Vec<Workout> = Vec::new();
    for workouts in read.into_iter().rev() {
        for workout in workouts {
            let Some(kept) = merged
                .iter_mut()
                .find(|kept| kept.day == workout.day && kept.number == workout.number)
            else {
                merged.push(workout);
                continue;
            };
            if workout.draft.performed && !kept.draft.performed {
                kept.week = workout.week.or(kept.week);
                kept.bodyweight = workout.bodyweight.or(kept.bodyweight);
            } else if workout.draft.performed {
                kept.week = kept.week.or(workout.week);
                kept.bodyweight = kept.bodyweight.or(workout.bodyweight);
            }
            absorb(&mut kept.draft, workout.draft);
        }
    }
    merged
}

/// Each performed workout onto its programme day: push Monday, legs
/// Wednesday, pull Friday, and push, pull, push when a week has two pushes and
/// no legs. A week the programme cannot hold is refused.
fn date_by_the_programme(workouts: &mut [Workout], scribe: &mut Scribe) {
    let mut weeks: BTreeMap<i8, Vec<usize>> = BTreeMap::new();
    for (index, workout) in workouts.iter().enumerate() {
        if workout.draft.performed
            && let Some(week) = workout.week
        {
            weeks.entry(week).or_default().push(index);
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

/// The final workbook's trunk work, onto the workouts `Weekly` puts it with.
fn trunk_2017(sheets: &[Sheet], copy: usize, workouts: &mut [Workout], scribe: &mut Scribe) {
    let Some(sheet) = sheets.iter().find(|sheet| sheet.name == "Trunk") else {
        return;
    };
    for block in blocks(sheet) {
        let mut seen: BTreeMap<(i8, String), usize> = BTreeMap::new();
        for row in 1..sheet.height() {
            let Some(name) = block.name(sheet, row) else {
                continue;
            };
            let Some(week) = block.week(sheet, row) else {
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
                    workout.day == day && workout.week == Some(week) && workout.draft.performed
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
                .extend(block.entries(sheet, row, &name, copy));
        }
    }
}

/// What a row of the earlier copies' trunk sheets is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trunk {
    Carry,
    Landmines,
    Deadbugs,
}

impl Trunk {
    /// The days of a programme week it was done on, in the order the earlier
    /// copies' `Weekly` numbers them.
    const fn days(self) -> &'static [Day] {
        match self {
            Self::Carry => &[Day::Legs, Day::Push, Day::Pull],
            Self::Landmines => &[Day::Push, Day::Pull],
            Self::Deadbugs => &[Day::Legs],
        }
    }
}

impl fmt::Display for Trunk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Carry => "Carry",
            Self::Landmines => "Landmines",
            Self::Deadbugs => "Deadbugs",
        })
    }
}

/// Which workout a trunk row says it was done in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pointer {
    /// The programme week and the slot in it: `2.1` is the first of week 2.
    /// That week's workouts are the second of each day.
    Programme { week: u32, day: Day },
    /// A calendar week, as the May copy writes its later rows.
    Calendar(i8),
}

/// One row of an earlier copy's trunk sheet: the Nth of its kind.
struct TrunkRow {
    trunk: Trunk,
    number: u32,
    pointer: Option<Pointer>,
    bodyweight: Option<Kg>,
    draft: Draft,
}

/// The March, April and May copies' trunk work, onto the workouts it was done
/// in (operator, 2026-09-28).
///
/// The final `Trunk` starts at week 14, so the carries, landmines and dead bugs
/// before it are only in the copies. A row joins the workout its copy points it
/// at, and only where the body weight written on it is that workout's: the
/// pointer says which workout, and the body weight confirms it. A row that does
/// not match is refused, not guessed onto a day.
fn trunk_before_week_14(copies: &[Opened<'_>], workouts: &mut [Workout], scribe: &mut Scribe) {
    for row in merge_trunk(copies) {
        if !row.draft.performed {
            continue;
        }
        let what = format!(
            "{}, workout {} in {}",
            row.trunk,
            row.number,
            copies
                .get(row.draft.copy)
                .map_or_else(String::new, |copy| copy.file.path().to_string())
        );
        let Some(workout) = placed(&row, &what, workouts, scribe) else {
            continue;
        };
        let press = workout.draft.press;
        let entries = std::mem::take(&mut workout.draft.entries);
        workout.draft.entries = merge_sets(entries, row.draft.entries, press);
    }
}

/// Every copy's trunk rows as one, the most recent copy first.
fn merge_trunk(copies: &[Opened<'_>]) -> Vec<TrunkRow> {
    let mut merged: Vec<TrunkRow> = Vec::new();
    for (copy, opened) in copies.iter().enumerate().rev() {
        for row in opened
            .sheets
            .iter()
            .flat_map(|sheet| trunk_rows(sheet, copy))
        {
            let Some(kept) = merged
                .iter_mut()
                .find(|kept| kept.trunk == row.trunk && kept.number == row.number)
            else {
                merged.push(row);
                continue;
            };
            kept.pointer = kept.pointer.or(row.pointer);
            kept.bodyweight = kept.bodyweight.or(row.bodyweight);
            absorb(&mut kept.draft, row.draft);
        }
    }
    merged
}

/// The workout a trunk row was done in: the one its copy points at, if the
/// body weight on the row is that workout's. Otherwise it is refused.
fn placed<'workouts>(
    row: &TrunkRow,
    what: &str,
    workouts: &'workouts mut [Workout],
    scribe: &mut Scribe,
) -> Option<&'workouts mut Workout> {
    let Some(bodyweight) = row.bodyweight else {
        unmodelled(scribe, format!("{what}: no body weight to place it by"));
        return None;
    };
    match row.pointer {
        None => {
            unmodelled(scribe, format!("{what}: no copy says which week it was"));
            None
        }
        Some(Pointer::Programme { week, day }) => {
            let found = workouts.iter_mut().find(|workout| {
                workout.day == day && workout.number == week && workout.draft.performed
            });
            match found {
                Some(workout) if workout.bodyweight == Some(bodyweight) => Some(workout),
                Some(workout) => {
                    let theirs = workout
                        .bodyweight
                        .map_or_else(|| "none".to_owned(), |kg| format!("{kg} kg"));
                    unmodelled(
                        scribe,
                        format!(
                            "{what}: body weight {bodyweight} kg is not {}'s ({theirs})",
                            workout.draft.what
                        ),
                    );
                    None
                }
                None => {
                    unmodelled(
                        scribe,
                        format!("{what}: {day}, workout {week} was not performed"),
                    );
                    None
                }
            }
        }
        Some(Pointer::Calendar(week)) => {
            let mut found = workouts.iter_mut().filter(|workout| {
                workout.week == Some(week)
                    && workout.draft.performed
                    && workout.bodyweight == Some(bodyweight)
            });
            let (first, second) = (found.next(), found.next());
            if let (Some(workout), None) = (first, second) {
                return Some(workout);
            }
            unmodelled(
                scribe,
                format!("{what}: not one workout in week {week} has body weight {bodyweight} kg"),
            );
            None
        }
    }
}

/// How far one of the earlier copies' carries went. They count carries, and a
/// carry was *"one walk over a set distance, no idea, probably 20 meters"*
/// (operator, 2026-09-28).
const CARRY: Metres = Metres::from_millimetres(20_000);

/// The rows of one earlier-copy trunk sheet, in whichever of the three layouts
/// it has.
fn trunk_rows(sheet: &Sheet, copy: usize) -> Vec<TrunkRow> {
    let width = sheet.width();
    // (what it is, the slot a sheet of one slot is, how the row is numbered)
    let (trunk, slot) = match sheet.name.as_str() {
        "Carry, Left" => (Trunk::Carry, Some(1)),
        "Carry, Right" => (Trunk::Carry, Some(2)),
        "Carry, Both" => (Trunk::Carry, Some(3)),
        "Landmines1" => (Trunk::Landmines, Some(1)),
        "Landmines2" => (Trunk::Landmines, Some(2)),
        "Carry" => (Trunk::Carry, None),
        "Landmines" => (Trunk::Landmines, None),
        "Deadbugs" => (Trunk::Deadbugs, Some(1)),
        _ => return Vec::new(),
    };
    let Some(week) = column(sheet, 0, "Wk", 0, width) else {
        return Vec::new();
    };
    let numbered = column(sheet, 0, "Work Out", 0, width);
    let block = Block::found(
        sheet,
        None,
        None,
        BlockName::Header(String::new()),
        0,
        width,
    );
    let slots = trunk.days().len();

    let mut rows = Vec::new();
    for row in 1..sheet.height() {
        let written = sheet.get(row, week);
        let (number, pointer) = match (numbered, slot) {
            // The May copy: numbered, and a calendar week where it says one.
            (Some(numbered), _) => {
                let Some(number) = whole(sheet.get(row, numbered)) else {
                    continue;
                };
                (number, week_of(written).map(Pointer::Calendar))
            }
            // The March copy, and the April dead bugs: a sheet per slot, a
            // row per programme week.
            (None, Some(slot)) => {
                let Some(programme) = whole(written) else {
                    continue;
                };
                let number = u32::try_from(slots)
                    .unwrap_or(1)
                    .saturating_mul(programme.saturating_sub(1))
                    .saturating_add(slot);
                let day = trunk
                    .days()
                    .get(usize::try_from(slot).unwrap_or(0).saturating_sub(1));
                (
                    number,
                    day.map(|&day| Pointer::Programme {
                        week: programme,
                        day,
                    }),
                )
            }
            // The April copy: a row per workout, `Wk` the programme week and
            // slot (`2.1`), or blank.
            (None, None) => (row, week_and_slot(written, trunk)),
        };
        let bodyweight = column(sheet, 0, "BW", 0, width)
            .and_then(|bw| mass(sheet.get(row, bw)))
            .filter(|kg| !kg.is_none());
        // A carry in one hand is a suitcase carry, and in both a farmer's walk.
        // The May copy times its later ones, which are suitcase holds.
        let both = sheet.name == "Carry, Both"
            || column(sheet, 0, "Side", 0, width)
                .and_then(|side| sheet.get(row, side).text())
                .is_some_and(|side| side.trim() == "Both");
        let name = match trunk {
            Trunk::Carry if both => "farmers walk",
            Trunk::Carry => "suitcase carry",
            Trunk::Landmines => "landmines",
            Trunk::Deadbugs => "deadbugs",
        };
        let mut entries = block.entries(sheet, row, name, copy);
        for entry in &mut entries {
            if trunk == Trunk::Carry && matches!(entry.count, Count::Held(_)) {
                "suitcase hold".clone_into(&mut entry.name);
            }
        }
        let performed = block
            .sets
            .iter()
            .any(|&(reps, _)| count(sheet.get(row, reps)).is_some());
        rows.push(TrunkRow {
            trunk,
            number,
            pointer,
            bodyweight,
            draft: Draft {
                on: Date::MIN,
                what: format!("{trunk}, workout {number}"),
                performed,
                press: Press::Barbell,
                entries: if performed { entries } else { Vec::new() },
                copy,
            },
        });
    }
    rows
}

/// A whole number in a cell.
fn whole(value: &Value) -> Option<u32> {
    match count(value)? {
        Count::Reps(number) => Some(number.as_u32()),
        _ => None,
    }
}

/// `2.1`: the programme week, and which of its slots.
fn week_and_slot(value: &Value, trunk: Trunk) -> Option<Pointer> {
    let Value::Number(number) = value else {
        return None;
    };
    let written = format!("{number}");
    let (week, slot) = written.split_once('.')?;
    let week = week.parse::<u32>().ok()?;
    let slot = slot.parse::<usize>().ok()?;
    let day = *trunk.days().get(slot.checked_sub(1)?)?;
    Some(Pointer::Programme { week, day })
}

/// One exercise's columns on a `CT 2017` sheet.
struct Block {
    /// The calendar week, where the layout writes one.
    week: Option<u32>,
    /// The workout's number among its day's. `None` where the layout numbers
    /// its workouts by row.
    number: Option<u32>,
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
    /// The block whose columns lie between `from` and `to`.
    fn found(
        sheet: &Sheet,
        week: Option<u32>,
        number: Option<u32>,
        name: BlockName,
        from: u32,
        to: u32,
    ) -> Self {
        let pair = |reps: &str, load: &str| {
            Some((
                column(sheet, 0, reps, from, to)?,
                column(sheet, 0, load, from, to)?,
            ))
        };
        Self {
            week,
            number,
            name,
            bodyweight: column(sheet, 0, "BW", from, to),
            warmups: (1..=3)
                .filter_map(|set| pair(&format!("WU{set}(reps)"), &format!("WU{set}(kg)")))
                .collect(),
            sets: (1..=3)
                .filter_map(|set| pair(&format!("SET{set}(reps)"), &format!("SET{set}(kg)")))
                .collect(),
        }
    }

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

    /// Which of its day's workouts `row` is. A layout that numbers by row
    /// only counts a row that has a week beside it.
    fn number(&self, sheet: &Sheet, row: u32) -> Option<u32> {
        self.number.map_or_else(
            || {
                self.week
                    .map_or(Some(row), |week| week_of(sheet.get(row, week)).map(|_| row))
            },
            |column| whole(sheet.get(row, column)),
        )
    }

    fn week(&self, sheet: &Sheet, row: u32) -> Option<i8> {
        self.week.and_then(|week| week_of(sheet.get(row, week)))
    }

    fn bodyweight(&self, sheet: &Sheet, row: u32) -> Option<Kg> {
        self.bodyweight
            .and_then(|column| mass(sheet.get(row, column)))
            .filter(|kg| !kg.is_none())
    }

    fn entries(&self, sheet: &Sheet, row: u32, name: &str, copy: usize) -> Vec<Entry> {
        let bodyweight = self
            .bodyweight
            .and_then(|column| mass(sheet.get(row, column)));
        let warmups = self
            .warmups
            .iter()
            .zip(1..)
            .map(|(&set, position)| (SetKind::Warmup, set, position));
        let sets = self
            .sets
            .iter()
            .zip(1..)
            .map(|(&set, position)| (SetKind::Working, set, position));
        warmups
            .chain(sets)
            .filter_map(|(kind, (reps, load), position)| {
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
                entry.copy = copy;
                entry.position = Some(position);
                Some(entry)
            })
            .collect()
    }
}

/// Every exercise block on a `Push`, `Legs`, `Pull` or `Trunk` sheet, found by
/// its `Wk` column.
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
        let numbered = before.contains("Work Out").then(|| week.saturating_sub(1));
        let name = if before == "Work Out" {
            BlockName::Column(week.saturating_sub(2))
        } else if let Some((name, _)) = before.split_once('\n') {
            BlockName::Header(name.trim().to_owned())
        } else {
            BlockName::Column(week.saturating_sub(1))
        };
        blocks.push(Block::found(sheet, Some(week), numbered, name, week, end));
    }
    blocks
}

/// The one block on an earlier copy's sheet per lift.
///
/// Its `Wk` is a calendar week only beside a `WorkOut` column (the April
/// copy's `Bench` and `Press`). Without one, it is the template's week, and
/// the row is the workout's number.
fn lift_sheet(sheet: &Sheet, name: &str) -> Option<Block> {
    let width = sheet.width();
    let week = column(sheet, 0, "Wk", 0, width)?;
    let numbered = column(sheet, 0, "WorkOut", 0, width);
    Some(Block::found(
        sheet,
        numbered.map(|_| week),
        numbered,
        BlockName::Header(name.to_owned()),
        0,
        width,
    ))
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
