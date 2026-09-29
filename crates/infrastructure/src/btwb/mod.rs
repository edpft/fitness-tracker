//! Beyond The White Board's export (#265), derived as the gym sessions the
//! operator logged in it (#285).
//!
//! **A day is a session.** The export has one row per result, and a class is
//! a lift and a conditioning piece: 33 of the 36 days in the operator's export
//! of 2026-09-28 hold both. So a day's results, in the order the export lists
//! them, are one session, and each result is one item in it: an exercise, or a
//! superset where it was done in rounds.
//!
//! **Several exports are one log sent more than once.** Each day is read from
//! the export landed last that holds it, and an earlier export adds only the
//! days the later ones lack. The exports of 2026-08-04 and 2026-09-28 are
//! identical, row for row.
//!
//! **The names are BTWB's, and they win** (operator, 2026-09-28): they are the
//! more accurate record of what the class did than the nearest Hevy template
//! he picked. Where a name leaves something out, one answer is picked and
//! written down below, because there is no deterministic way to know what he
//! actually did. A name with no mapping stops the run, as an unmapped Hevy
//! template does.

use std::collections::BTreeMap;

use application::{
    NormalisationError, Translation,
    ports::{SourceAccount, Translator},
};
use domain::{
    gym::{
        Exercise, Logged, ManualExercise, ManualGymSession, ManualItem, ManualSet, Performed,
        SetKind,
        exercise::{DistanceExercise, DurationExercise, RepsExercise},
    },
    landing::{CellRef, LandedRecord, LandingRecordId, SheetCell, SheetName},
    measure::{Duration, Metres, RepCount},
    normalised::{OperatorZone, Refusal, RefusalLocus, RefusalReason},
    sequence::{AtLeastTwo, NonEmpty},
};
use jiff::civil::Date;

use self::workout::{Done, Score};
use crate::{scribe::Scribe, spreadsheets::load_of};

mod csv;
mod workout;

/// The header BTWB writes, word for word.
const HEADER: [&str; 7] = [
    "Date",
    "Formatted Result",
    "Result",
    "Performed",
    "Workout",
    "Description",
    "Notes",
];

/// The column holding a result's description, which is what a set was read
/// from.
const DESCRIPTION: u32 = 5;

/// Every export landed, oldest first.
#[derive(Debug, Clone)]
pub struct Exports {
    files: NonEmpty<LandedRecord>,
}

impl Exports {
    /// The landed exports as one account, or none if nothing has landed.
    pub fn gather(records: Vec<LandedRecord>) -> Vec<Self> {
        NonEmpty::new(records)
            .map(|files| vec![Self { files }])
            .unwrap_or_default()
    }
}

impl SourceAccount for Exports {
    fn records(&self) -> usize {
        self.files.count()
    }
}

/// One result: a row of the export.
struct Entry<'a> {
    on: Date,
    /// Zero-based, the header being row 0, so the cell is where a spreadsheet
    /// program shows it.
    row: u32,
    workout: &'a str,
    description: &'a str,
    formatted: &'a str,
    result: &'a str,
}

/// Reads the gym sessions out of Beyond The White Board's exports.
#[derive(Debug, Clone, Copy, Default)]
pub struct BtwbTranslator;

impl Translator for BtwbTranslator {
    type Account = Exports;
    type Entity = ManualGymSession;

    /// The zone is not consulted: a result is a day.
    fn translate(
        &self,
        exports: &Exports,
        _zone: &OperatorZone,
    ) -> Result<Translation<ManualGymSession>, NormalisationError> {
        let mut scribes: Vec<Scribe> = Vec::new();
        let mut read: Vec<Option<Vec<Vec<String>>>> = Vec::new();
        let mut prepared: Vec<Option<Export>> = Vec::new();
        for record in exports.files.iter() {
            let mut scribe = Scribe::new(record);
            let records = opened(record, &mut scribe);
            // Once per export rather than once per day, so a file this cannot
            // name gives one reason rather than one for every day in it.
            prepared.push(records.as_ref().and_then(|_| prepare(record, &mut scribe)));
            read.push(records);
            scribes.push(scribe);
        }

        // The export each day is read from: the last landed that holds it.
        let mut days: BTreeMap<Date, usize> = BTreeMap::new();
        for (index, records) in read.iter().enumerate() {
            for (on, _) in results(records.as_deref().unwrap_or_default()) {
                days.insert(on, index);
            }
        }

        let mut sessions = Vec::new();
        for (on, index) in days {
            let (Some(Some(export)), Some(Some(records)), Some(scribe)) =
                (prepared.get(index), read.get(index), scribes.get_mut(index))
            else {
                continue;
            };
            let day: Vec<Entry<'_>> = results(records)
                .filter(|(date, _)| *date == on)
                .map(|(_, result)| result)
                .collect();
            if let Some(session) = session(export, on, &day, scribe)? {
                sessions.push(session);
            }
        }

        let refusals: Vec<Refusal> = scribes
            .into_iter()
            .flat_map(Scribe::into_refusals)
            .collect();
        if let Ok(entities) = NonEmpty::new(sessions) {
            return Ok(Translation::Entities { entities, refusals });
        }
        if let Ok(refusals) = NonEmpty::new(refusals) {
            return Ok(Translation::Refused(refusals));
        }
        Ok(Scribe::new(exports.files.first())
            .only(RefusalLocus::Record, RefusalReason::NothingTranslatable))
    }
}

/// One landed file as records, or a refusal saying why it is not an export.
fn opened(record: &LandedRecord, scribe: &mut Scribe) -> Option<Vec<Vec<String>>> {
    let path = record.provenance().as_file().map_or_else(
        || record.provenance().to_string(),
        |file| file.path().to_string(),
    );
    let refuse = |scribe: &mut Scribe, why: &str| {
        scribe.note(
            RefusalLocus::Record,
            RefusalReason::Unmodelled {
                detail: format!(
                    "{path}, a file that is not a Beyond The White Board export ({why}),"
                ),
            },
        );
    };
    match csv::records(record.payload().as_bytes()) {
        Ok(records) if records.first().is_some_and(|header| *header == HEADER) => Some(records),
        Ok(_) => {
            refuse(scribe, "its header is not BTWB's");
            None
        }
        Err(detail) => {
            refuse(scribe, &detail);
            None
        }
    }
}

/// Every dated result in an export's records.
fn results(records: &[Vec<String>]) -> impl Iterator<Item = (Date, Entry<'_>)> {
    records
        .iter()
        .enumerate()
        .skip(1)
        .filter_map(|(row, fields)| {
            let field = |column: usize| fields.get(column).map_or("", String::as_str);
            let on = field(0).parse::<Date>().ok()?;
            Some((
                on,
                Entry {
                    on,
                    row: u32::try_from(row).ok()?,
                    formatted: field(1),
                    result: field(2),
                    workout: field(4),
                    description: field(5),
                },
            ))
        })
}

/// One export, ready to read: what a session it holds is dated by, and the
/// name a cell of it is addressed under.
struct Export {
    logged: Logged,
    sheet: SheetName,
}

/// An opened export as the thing a session names, or a refusal saying why it
/// cannot be named.
///
/// A file BTWB sent through a folder always has a path, and this says so rather
/// than dropping the export: an account that yields nothing yields a reason
/// (§ 37).
fn prepare(record: &LandedRecord, scribe: &mut Scribe) -> Option<Export> {
    let Some(file) = record.provenance().as_file() else {
        scribe.note(
            RefusalLocus::Record,
            RefusalReason::UnreadablePayload {
                detail: format!("{} was served by a feed, not a folder", record.provenance()),
            },
        );
        return None;
    };
    let stem = file
        .path()
        .as_str()
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map_or_else(|| file.path().to_string(), |(stem, _)| stem.to_owned());
    let Ok(sheet) = SheetName::try_from(stem.as_str()) else {
        scribe.note(
            RefusalLocus::Record,
            RefusalReason::Unmodelled {
                detail: format!(
                    "{}, whose name a cell cannot be addressed under,",
                    file.path()
                ),
            },
        );
        return None;
    };
    Some(Export {
        logged: Logged {
            landed_as: record.id(),
            source_record_id: record.source_record_id().clone(),
            file: file.path().clone(),
        },
        sheet,
    })
}

/// One day's results as a session, refusing each result it cannot read.
fn session(
    export: &Export,
    on: Date,
    day: &[Entry<'_>],
    scribe: &mut Scribe,
) -> Result<Option<ManualGymSession>, NormalisationError> {
    let mut items = Vec::new();
    for result in day {
        let score = Score {
            formatted: result.formatted,
            result: result.result,
        };
        let groups = match workout::read(result.description, &score) {
            Ok(groups) => groups,
            Err(detail) => {
                scribe.note(
                    RefusalLocus::Record,
                    RefusalReason::Unmodelled {
                        detail: format!(
                            "{} ({}), row {}: {detail},",
                            result.workout,
                            result.on,
                            result.row.saturating_add(1)
                        ),
                    },
                );
                continue;
            }
        };
        let at = SheetCell {
            sheet: export.sheet.clone(),
            cell: CellRef::at(result.row, DESCRIPTION),
        };
        for group in groups {
            if let Some(item) = item(group, export, &at)? {
                items.push(item);
            }
        }
    }
    Ok(NonEmpty::new(items).ok().map(|items| {
        let copies = NonEmpty::of(export.logged.clone(), Vec::new());
        ManualGymSession::new(on, copies, items)
    }))
}

/// One group of sets as an item: one exercise, or a superset of the
/// exercises in the order they were first done.
fn item(
    sets: Vec<Done>,
    export: &Export,
    at: &SheetCell,
) -> Result<Option<ManualItem>, NormalisationError> {
    let mut exercises: Vec<(Exercise, Vec<Done>)> = Vec::new();
    for set in sets {
        let Some(exercise) = exercise_named(&set.name) else {
            return Err(NormalisationError::UnmappedExercise {
                template_id: set.name,
                source_record_id: export.logged.source_record_id.to_string(),
            });
        };
        match exercises.iter_mut().find(|(seen, _)| *seen == exercise) {
            Some((_, done)) => done.push(set),
            None => exercises.push((exercise, vec![set])),
        }
    }
    let mut built: Vec<ManualExercise> = exercises
        .into_iter()
        .filter_map(|(exercise, sets)| performed(exercise, &sets, export.logged.landed_as, at))
        .collect();
    Ok(match built.len() {
        0 => None,
        1 => built.pop().map(ManualItem::Exercise),
        _ => AtLeastTwo::new(built).ok().map(ManualItem::Superset),
    })
}

/// The sets of one exercise, counted in its own measure.
fn performed(
    exercise: Exercise,
    sets: &[Done],
    copy: LandingRecordId,
    at: &SheetCell,
) -> Option<ManualExercise> {
    let set = |done: &Done| -> ManualSet<()> {
        ManualSet {
            load: done.load.map(|mass| load_of(exercise, mass, None)),
            outcome: Performed::Completed(None),
            intensity: None,
            kind: SetKind::Working,
            rest_after: done.rest_after.map(Duration::from_seconds),
            copy,
            at: at.clone(),
        }
    };
    match exercise {
        Exercise::Reps(exercise) => {
            let sets = sets
                .iter()
                .map(|done| {
                    // A rep exercise given a time was prescribed the time.
                    let reps = done
                        .count
                        .filter(|_| done.seconds.is_none())
                        .and_then(|count| RepCount::new(count).ok());
                    with(set(done), reps)
                })
                .collect();
            NonEmpty::new(sets)
                .ok()
                .map(|sets| ManualExercise::ForReps { exercise, sets })
        }
        Exercise::Duration(exercise) => {
            // A count on a timed exercise is calories or skips, which were
            // the prescription.
            let sets = sets
                .iter()
                .map(|done| with(set(done), done.seconds.map(Duration::from_seconds)))
                .collect();
            NonEmpty::new(sets)
                .ok()
                .map(|sets| ManualExercise::ForDuration { exercise, sets })
        }
        Exercise::Distance(exercise) => {
            let sets = sets
                .iter()
                .map(|done| with(set(done), done.millimetres.map(Metres::from_millimetres)))
                .collect();
            NonEmpty::new(sets)
                .ok()
                .map(|sets| ManualExercise::ForDistance { exercise, sets })
        }
    }
}

/// A set given its measure, which may be unstated.
fn with<M>(set: ManualSet<()>, measure: Option<M>) -> ManualSet<M> {
    ManualSet {
        load: set.load,
        outcome: Performed::Completed(measure),
        intensity: set.intensity,
        kind: set.kind,
        rest_after: set.rest_after,
        copy: set.copy,
        at: set.at,
    }
}

/// The exercise a BTWB movement is (operator, 2026-09-28).
///
/// Each name as the export spells it, singular or plural. The picks:
///
/// - **A machine for calories** is the air bike: he chose it whenever he was
///   given the choice. A row or ski erg is the ski erg, which he chose over
///   the rower.
/// - **Double and single unders** are the jump rope, which Hevy counts in time.
/// - **Assisted, self-assisted, kneeling and weighted** are the movement
///   itself: assistance is a load, not an exercise.
/// - **A pull-over or push-down with straight arms** is the straight-arm
///   pull-down, on the cable at the loads he wrote.
/// - Where BTWB gives no implement, the one Hevy shows he used: a cable row is
///   the single-arm cable row, a split squat and a reverse or front-rack lunge
///   are dumbbells, a floor press is dumbbells, a tricep extension is the
///   cable.
fn exercise_named(name: &str) -> Option<Exercise> {
    let reps = match name {
        "Back Rack Reverse Lunges" => RepsExercise::ReverseLungeBarbell,
        "Back Squats" => RepsExercise::BackSquatBarbell,
        "Bar Facing Burpees" => RepsExercise::BurpeeOverTheBar,
        "Barbell Curls" => RepsExercise::BicepCurlBarbell,
        "Barbell Thrusters" | "Thrusters" | "Thruster" => RepsExercise::ThrusterBarbell,
        "Bent Over Rows" => RepsExercise::BentOverRowBarbell,
        "Bicep Curls" | "Dumbbell Bicep Curl" | "Dumbbell Bicep Curls" => {
            RepsExercise::BicepCurlDumbbell
        }
        "Box Jumps" | "Box Jump Overs" => RepsExercise::BoxJump,
        "Broad Jumps" => RepsExercise::BroadJump,
        "Burpee" | "Burpees" => RepsExercise::Burpee,
        "Cable Rows" => RepsExercise::SingleArmCableRow,
        "Close Grip Push-ups" => RepsExercise::CloseGripPushUp,
        "Cyclist Squat" => RepsExercise::CyclistSquat,
        "Deadlift" | "Deadlifts" => RepsExercise::DeadliftBarbell,
        "Deficit Push-ups" => RepsExercise::DeficitPushups,
        "Devils Press" => RepsExercise::DevilPressDumbbell,
        "Double Dumbbell Front Squats" => RepsExercise::FrontSquatDumbbell,
        "Double Dumbbell Hang Snatches" | "Dumbbell Hang Snatches" => {
            RepsExercise::HangSnatchDumbbell
        }
        "Dumbbell Floor Press" | "Floor Press" => RepsExercise::FloorPressDumbbell,
        "Dumbbell Front Rack Lunges" | "Dumbbell Reverse Lunges" => RepsExercise::LungeDumbbell,
        "Dumbbell Lateral Shoulder Raises" => RepsExercise::LateralRaiseDumbbell,
        "Dumbbell Push Press" => RepsExercise::PushPressDumbbell,
        "Dumbbell Snatches" => RepsExercise::DumbbellSnatch,
        "Dumbbell Thruster" | "Dumbbell Thrusters" => RepsExercise::ThrusterDumbbell,
        "Front Squats" => RepsExercise::FrontSquatBarbell,
        "Hammer Curls" => RepsExercise::HammerCurlDumbbell,
        "Handstand Shoulder Taps" => RepsExercise::HandstandShoulderTap,
        "Hanging Knee Raises" | "Weighted Knee Raises" => RepsExercise::HangingKneeRaise,
        "Hanging L-Sit Complexes" => RepsExercise::HangingLSitComplex,
        "Kettlebell Crush Grip Curls" => RepsExercise::CrushGripCurlKettlebell,
        "Kettlebell Rows" => RepsExercise::RowKettlebell,
        "Kettlebell Skull Crushers" => RepsExercise::SkullcrusherKettlebell,
        "Kettlebell Swings" | "Russian Kettlebell Swings" | "American Kettlebell Swings" => {
            RepsExercise::KettlebellSwing
        }
        "Lu Raise" => RepsExercise::OverheadLateralRaise,
        "Overhead Tricep Extensions" => RepsExercise::OverheadTricepsExtensionCable,
        "Pendlay Rows" => RepsExercise::PendlayRowBarbell,
        "Pike Compressions" | "Pike Compression Lift Overs" => RepsExercise::PikeCompression,
        "Power Cleans" => RepsExercise::PowerClean,
        "Press" => RepsExercise::OverheadPressBarbell,
        "Pull-ups" | "Self Assisted Pull-ups" => RepsExercise::PullUp,
        "Push Press" => RepsExercise::PushPress,
        "Push-ups" | "Kneeling Push-ups" => RepsExercise::PushUp,
        "Rear Delt Raises" => RepsExercise::RearDeltRaiseDumbbell,
        "Renegade Rows" | "Renegade Row + Push-ups" => RepsExercise::RenegadeRowDumbbell,
        "Ring Dips" | "Assisted Ring Dips" => RepsExercise::RingDip,
        "Assisted Dips" => RepsExercise::ChestDip,
        "Ring Row" => RepsExercise::RingRows,
        "Romanian Deadlifts" => RepsExercise::RomanianDeadliftBarbell,
        "Sandbag Good Morning" => RepsExercise::SandbagGoodMorning,
        "Sandbag Squats" => RepsExercise::SandbagSquat,
        "Sandbag-to-Shoulders" => RepsExercise::SandbagToShoulder,
        "Single Arm Devil Press" => RepsExercise::SingleArmDevilPressDumbbell,
        "Single Arm Kettlebell Clean & Jerks" => RepsExercise::SingleArmCleanAndJerkKettlebell,
        "Sit-ups" => RepsExercise::SitUp,
        "Split Squats" => RepsExercise::SplitSquatDumbbell,
        "Straight Arm Lat Pull Down"
        | "Straight Arm Lat Pull Downs"
        | "Lat Push Down"
        | "Straight Arm Pull Overs"
        | "Cable Lat Pull Overs"
        | "Lat Pullovers" => RepsExercise::StraightArmLatPulldownCable,
        "Toe Touches" => RepsExercise::ToeTouch,
        "Toes-to-bars" => RepsExercise::ToesToBar,
        "Tricep Push Downs" => RepsExercise::TricepsPushdownCable,
        "Wall Ball" | "Wall Balls" => RepsExercise::WallBall,
        "Wall Climbs" => RepsExercise::WallClimbs,
        "Weighted Step-ups" => RepsExercise::StepUpDumbbell,
        _ => {
            let duration = match name {
                "Any Machine Calorie"
                | "Any Machine Calories"
                | "Machine Calorie"
                | "Echo Bike"
                | "Echo Bike Calorie" => DurationExercise::AirBike,
                "Row / Ski Erg" | "Row/Ski Erg Cals" => DurationExercise::SkiErg,
                "Double Under" | "Double Unders" | "Single Unders" => DurationExercise::JumpRope,
                "Handstand Hold" => DurationExercise::HandstandHold,
                "Run" => return Some(Exercise::Distance(DistanceExercise::Running)),
                _ => return None,
            };
            return Some(Exercise::Duration(duration));
        }
    };
    Some(Exercise::Reps(reps))
}
