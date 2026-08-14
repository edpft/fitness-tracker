//! Which of our exercises a Hevy template is, and how to read its weight.
//!
//! Code, not data (§ 9). A table in the database would make this an overlay in
//! everything but name — editable without review, invisible in a diff — and
//! deterministic translation is precisely the part that must not be.
//!
//! It lives here rather than in `domain` because it is keyed on
//! `exercise_template_id`, which is a Hevy identifier. A domain holding a
//! vendor's identifiers is a domain shaped by a source. What `domain` owns is
//! the vocabulary this points *at*, and the direction is the whole of § 8:
//! sources are translated into our entities, never the reverse.
//!
//! **Titles inform this table and never key it.** Neither of Hevy's labels is
//! stable — `Overhead Squat` has two template ids, one builtin and one custom,
//! and template `DDB29047` has appeared under two titles, having been renamed
//! mid-history. The titles in the comments are what the template was called
//! most often, and are there to be read, not matched.
//!
//! ## What decided each entry
//!
//! **Load is `Absolute` where no unloaded version of the movement exists.** The
//! implement has mass, so zero is impossible and a zero is a data error by
//! construction. `Relative` where an unloaded version does exist, so zero is a
//! real observation and the number is a delta against a bodyweight the set does
//! not record.
//!
//! That judgement is graded by the corpus and the grade is exact. 93 sets carry
//! a zero load; the model of record says 7 of them are errors. Every zero on a
//! `Relative` template is plain bodyweight and translates, every zero on an
//! `Absolute` one refuses, so this table is right if and only if exactly seven
//! refuse — and it is wrong in both directions. An eighth means a bodyweight
//! movement was called absolute; a sixth means the reverse.
//!
//! The pair that shows the rule doing real work: `Romanian Deadlift (Barbell)`
//! is `Absolute` and `Single Leg Romanian Deadlift (Dumbbell)` is `Relative`.
//! Nothing in the titles forces that. What forces it is that a single-leg RDL
//! is a balance drill before it is a loaded hinge — four sets in the corpus
//! were done with nothing in hand — and there is no barbell RDL without a
//! barbell.
//!
//! **An assisted variant negates.** Hevy has no assistance concept: assisted
//! movements are separately named exercises carrying a positive weight. So
//! `RelativeNegated` turns 20 into −20 and puts assistance and added weight on
//! one axis, which is the mapping's reason to exist.
//!
//! **A band-resistance exercise refuses**, as a declared limitation. Band
//! tension varies through the range of motion, nothing records the mechanism,
//! and the account's assisted loads run `0, 7, 14, 21, 28, 35, 42` — stacked
//! bands rather than a machine stack, which deterministic translation cannot
//! tell apart. Four templates, 16 sets.
//!
//! `Pull Up (Band)` is not among them: that is band *assistance*, and it maps
//! to `PullUp` negated like any other assisted pull-up. That band and machine
//! assistance are not comparable is a limitation declared in the model of
//! record, not a reason to refuse the set.
//!
//! **Where our category and the source's differ, ours wins.** One entry:
//! `Sled Push`, which Hevy calls distance-and-duration and which records thirty
//! seconds and a zero distance on all nine of its sets. It is a duration
//! exercise here, so that zero distance is never read and never refused.
//!
//! Hevy's own `ExerciseTemplate.type` is the only published carrier of the
//! sign convention and is invisible in a workout payload. It informed this
//! table when it was authored and is not read when translation runs — which
//! would put a network request inside a derivation that must not make one, and
//! would make the result depend on what the vendor's catalogue says today.

use domain::gym::{
    Exercise,
    exercise::{DistanceExercise, DurationExercise, RepsExercise, TimedDistanceExercise},
};

/// How to read the weight column for one exercise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadReading {
    /// The implement has mass. Zero is impossible, and a zero refuses.
    Absolute,
    /// An unloaded version exists. Zero is plain bodyweight, and an absent
    /// weight is the same thing.
    Relative,
    /// An assisted variant: the source's positive number is assistance, so it
    /// is negated onto the relative axis.
    RelativeNegated,
    /// Band resistance, which no scalar expresses honestly. Refused.
    BandResistance,
}

/// What one template resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mapped {
    pub exercise: Exercise,
    pub load: LoadReading,
}

const fn reps(exercise: RepsExercise, load: LoadReading) -> Mapped {
    Mapped {
        exercise: Exercise::Reps(exercise),
        load,
    }
}

const fn duration(exercise: DurationExercise, load: LoadReading) -> Mapped {
    Mapped {
        exercise: Exercise::Duration(exercise),
        load,
    }
}

const fn distance(exercise: DistanceExercise, load: LoadReading) -> Mapped {
    Mapped {
        exercise: Exercise::Distance(exercise),
        load,
    }
}

const fn timed(exercise: TimedDistanceExercise, load: LoadReading) -> Mapped {
    Mapped {
        exercise: Exercise::TimedDistance(exercise),
        load,
    }
}

/// The mapping. 134 templates onto 130 exercises, many-to-one.
///
/// A template this does not cover fails the run naming itself. There is no
/// passthrough, no fallback exercise and no silent omission: the vocabulary is
/// ours and a gap in it is a defect to fix, not data to record around.
///
/// Ordered by how much of the corpus each accounts for, so the entries that
/// matter most are the ones read first. The comment on each is the source's
/// title and the number of sets it carried.
///
/// `match_same_arms` is allowed because two arms agreeing is the mapping
/// *working*: `Pull Up (Assisted)` and `Pull Up (Band)` reach one exercise on
/// purpose, and merging them would delete the per-template comment that says
/// which source template each covers. `too_many_lines` is allowed for the same
/// reason a phone book is long. Neither is a forbidden lint, and neither is
/// hiding a defect.
#[allow(clippy::too_many_lines, clippy::match_same_arms)]
pub fn lookup(template_id: &str) -> Option<Mapped> {
    let mapped = match template_id {
        "D04AC939" => reps(RepsExercise::SquatBarbell, LoadReading::Absolute),              // Squat (Barbell) (377)
        "E9E4089F" => reps(RepsExercise::ChestDip, LoadReading::RelativeNegated),           // Chest Dip (Assisted) (277)
        "C6272009" => reps(RepsExercise::DeadliftBarbell, LoadReading::Absolute),           // Deadlift (Barbell) (196)
        "2C37EC5E" => reps(RepsExercise::PullUp, LoadReading::RelativeNegated),             // Pull Up (Assisted) (159)
        "5046D0A9" => reps(RepsExercise::FrontSquat, LoadReading::Absolute),                // Front Squat (147)
        "4F942934" => reps(RepsExercise::PreacherCurlBarbell, LoadReading::Absolute),       // Preacher Curl (Barbell) (121)
        "9202CC23" => reps(RepsExercise::SeatedWristExtensionBarbell, LoadReading::Absolute), // Seated Wrist Extension (Barbell) (119)
        "21310F5F" => reps(RepsExercise::TricepsExtensionCable, LoadReading::Absolute),     // Triceps Extension (Cable) (117)
        "B5EFBF9C" => reps(RepsExercise::OverheadTricepsExtensionCable, LoadReading::Absolute), // Overhead Triceps Extension (Cable) (109)
        "2B4B7310" => reps(RepsExercise::RomanianDeadliftBarbell, LoadReading::Absolute),   // Romanian Deadlift (Barbell) (103)
        "1B2B1E7C" => reps(RepsExercise::PullUp, LoadReading::Relative),                    // Pull Up (97)
        "425805F4" => reps(RepsExercise::InvertedRow, LoadReading::Relative),               // Inverted Row (85)
        "6FCD7755" => reps(RepsExercise::ChestDip, LoadReading::Relative),                  // Chest Dip (84)
        "7B8D84E8" => reps(RepsExercise::OverheadPressBarbell, LoadReading::Absolute),      // Overhead Press (Barbell) (82)
        "8A2E6481" => reps(RepsExercise::LowRowSuspension, LoadReading::Relative),          // Low Row (Suspension) (77)
        "A2D838BD" => reps(RepsExercise::CableTwistUpToDown, LoadReading::Absolute),        // Cable Twist (Up to down) (75)
        "6AC96645" => reps(RepsExercise::OverheadPressDumbbell, LoadReading::Absolute),     // Overhead Press (Dumbbell) (64)
        "BE289E45" => reps(RepsExercise::LateralRaiseCable, LoadReading::Absolute),         // Lateral Raise (Cable) (63)
        "56092DD1" => reps(RepsExercise::BoxJump, LoadReading::Relative),                   // Box Jump (59)
        "FAB6EB2F" => reps(RepsExercise::PreacherCurlDumbbell, LoadReading::Absolute),      // Preacher Curl (Dumbbell) (59)
        "75A4F6C4" => reps(RepsExercise::LegExtensionMachine, LoadReading::Absolute),       // Leg Extension (Machine) (54)
        "D2387AB1" => reps(RepsExercise::StraightArmLatPulldownCable, LoadReading::Absolute), // Straight Arm Lat Pulldown (Cable) (51)
        "1006DF48" => reps(RepsExercise::SeatedPalmsUpWristCurl, LoadReading::Absolute),    // Seated Palms Up Wrist Curl (49)
        "c6e09263-5d20-450d-a219-95ba47ee8305" => reps(RepsExercise::Pogo, LoadReading::Relative), // Pogo (45)
        "7E3BC8B6" => reps(RepsExercise::HammerCurlDumbbell, LoadReading::Absolute),        // Hammer Curl (Dumbbell) (41)
        "2F8D3067" => reps(RepsExercise::TricepsExtensionBarbell, LoadReading::Absolute),   // Triceps Extension (Barbell) (38)
        "F8A0FCCA" => reps(RepsExercise::KettlebellSwing, LoadReading::Absolute),           // Kettlebell Swing (36)
        "43573BB8" => duration(DurationExercise::AirBike, LoadReading::Relative),           // Air Bike (32)
        "BB792A36" => reps(RepsExercise::Burpee, LoadReading::Relative),                    // Burpee (31)
        "FB09C938" => reps(RepsExercise::Snatch, LoadReading::Absolute),                    // Snatch (31)
        "10313AFD" => reps(RepsExercise::ThrusterKettlebell, LoadReading::Absolute),        // Thruster (Kettlebell) (31)
        "422B08F1" => reps(RepsExercise::LateralRaiseDumbbell, LoadReading::Absolute),      // Lateral Raise (Dumbbell) (29)
        "B8127AD1" => reps(RepsExercise::LyingLegCurlMachine, LoadReading::Absolute),       // Lying Leg Curl (Machine) (29)
        "5c98d763-9ceb-412c-8365-18110f9d5897" => duration(DurationExercise::NinetyNinety, LoadReading::Relative), // 90/90 (28)
        "e2182af0-2577-4603-8e70-18273be1d48b" => duration(DurationExercise::CouchStretch, LoadReading::Relative), // Couch Stretch (28)
        "091737FA" => reps(RepsExercise::BackExtensionWeightedHyperextension, LoadReading::Relative), // Back Extension (Weighted Hyperextension) (26)
        "93472AC1" => reps(RepsExercise::SingleLegRomanianDeadliftBarbell, LoadReading::Absolute), // Single Leg Romanian Deadlift (Barbell) (26)
        "A733CC5B" => distance(DistanceExercise::WalkingLungeDumbbell, LoadReading::Absolute), // Walking Lunge (Dumbbell) (26)
        "118ed850-2aa7-4010-ab93-bdf8b0352660" => reps(RepsExercise::HammerTwists, LoadReading::Relative), // Hammer Twists (24)
        "4E5257DE" => reps(RepsExercise::LatPulldownCloseGripCable, LoadReading::Absolute), // Lat Pulldown - Close Grip (Cable) (24)
        "542F3CD5" => reps(RepsExercise::PushPress, LoadReading::Absolute),                 // Push Press (23)
        "B5D3A742" => reps(RepsExercise::BulgarianSplitSquat, LoadReading::Relative),       // Bulgarian Split Squat (21)
        "ABEC557F" => reps(RepsExercise::ShrugDumbbell, LoadReading::Absolute),             // Shrug (Dumbbell) (21)
        "AC1BB830" => timed(TimedDistanceExercise::Running, LoadReading::Relative),         // Running (19)
        "50DFDFAB" => reps(RepsExercise::InclineBenchPressBarbell, LoadReading::Absolute),  // Incline Bench Press (Barbell) (18)
        "108D7A14" => reps(RepsExercise::NordicHamstringsCurls, LoadReading::Relative),     // Nordic Hamstrings Curls (18)
        "DE68C825" => reps(RepsExercise::SingleArmLateralRaiseCable, LoadReading::Absolute), // Single Arm Lateral Raise (Cable) (18)
        "B9380898" => duration(DurationExercise::DeadHang, LoadReading::Relative),          // Dead Hang (17)
        "527DA061" => duration(DurationExercise::Stretching, LoadReading::Relative),        // Stretching (16)
        "50C613D0" => distance(DistanceExercise::FarmersWalk, LoadReading::Absolute),       // Farmers Walk (15)
        "8347DFD1" => reps(RepsExercise::SingleArmTricepExtensionDumbbell, LoadReading::Absolute), // Single Arm Tricep Extension (Dumbbell) (15)
        "D928C232" => reps(RepsExercise::CrunchWeighted, LoadReading::Relative),            // Crunch (Weighted) (11)
        "914F3A96" => reps(RepsExercise::ChestSupportedInclineRowDumbbell, LoadReading::Absolute), // Chest Supported Incline Row (Dumbbell) (10)
        "F4E77594" => reps(RepsExercise::HangSnatch, LoadReading::Absolute),                // Hang Snatch (10)
        "040BA2E3" => duration(DurationExercise::JumpRope, LoadReading::Relative),          // Jump Rope (10)
        "D8911FC4" => reps(RepsExercise::DeadBug, LoadReading::Relative),                   // Dead Bug (9)
        "B537D09F" => reps(RepsExercise::LungeDumbbell, LoadReading::Absolute),             // Lunge (Dumbbell) (9)
        "1B89CA1B" => reps(RepsExercise::RenegadeRowDumbbell, LoadReading::Absolute),       // Renegade Row (Dumbbell) (9)
        "937292AB" => reps(RepsExercise::SingleLegRomanianDeadliftDumbbell, LoadReading::Relative), // Single Leg Romanian Deadlift (Dumbbell) (9)
        "7757171F" => duration(DurationExercise::SledPush, LoadReading::Relative),          // Sled Push (9)
        "2b9f6f49-71cf-45bd-88d8-a6a2bb6c0814" => reps(RepsExercise::BackSquatWithSnatchPushPress, LoadReading::Absolute), // Back Squat w/ Snatch Push Press (8)
        "29083183" => reps(RepsExercise::ChinUp, LoadReading::Relative),                    // Chin Up (8)
        "266f2cb6-b4a5-45a9-9fc6-722386547616" => reps(RepsExercise::DropSnatch, LoadReading::Absolute), // Drop Snatch (8)
        "756EE329" => reps(RepsExercise::FloorPressDumbbell, LoadReading::Absolute),        // Floor Press (Dumbbell) (8)
        "BE3615CF" => duration(DurationExercise::HandstandHold, LoadReading::Relative),     // Handstand Hold (8)
        "39796be6-52b2-49e5-a27f-26fb0009260c" => reps(RepsExercise::HangHighPull, LoadReading::Absolute), // Hang High Pull (8)
        "F99C211D" => reps(RepsExercise::KettlebellClean, LoadReading::Absolute),           // Kettlebell Clean (8)
        "a500417a-5f3d-4061-aa9b-0635181868ec" => reps(RepsExercise::OverheadSquat, LoadReading::Absolute), // Overhead Squat (8)
        "018ADC12" => reps(RepsExercise::PendlayRowBarbell, LoadReading::Absolute),         // Pendlay Row (Barbell) (8)
        "856037db-34d2-41f5-b8b7-1ca15a7d348c" => reps(RepsExercise::PowerMuscleSnatch, LoadReading::Absolute), // Power Muscle Snatch (8)
        "392887AA" => reps(RepsExercise::PushUp, LoadReading::Relative),                    // Push Up (8)
        "2DBCA395" => reps(RepsExercise::BehindTheBackCurlCable, LoadReading::Absolute),    // Behind the Back Curl (Cable) (7)
        "55E6546F" => reps(RepsExercise::BentOverRowBarbell, LoadReading::Absolute),        // Bent Over Row (Barbell) (7)
        "C628D768" => reps(RepsExercise::PowerClean, LoadReading::Absolute),                // Power Clean (7)
        "0393F233" => reps(RepsExercise::SeatedCableRowVGripCable, LoadReading::Absolute),  // Seated Cable Row - V Grip (Cable) (7)
        "4F5866F8" => reps(RepsExercise::BackExtensionHyperextension, LoadReading::Relative), // Back Extension (Hyperextension) (6)
        "A05C064D" => reps(RepsExercise::BackExtensionMachine, LoadReading::Absolute),      // Back Extension (Machine) (6)
        "DDB29047" => reps(RepsExercise::BehindTheBackWristCurlBarbell, LoadReading::Absolute), // Behind the Back Wrist Curl (Barbell) (6)
        "DCF3B31B" => reps(RepsExercise::Crunch, LoadReading::Relative),                    // Crunch (6)
        "BC10A922" => reps(RepsExercise::DeclineCrunch, LoadReading::Relative),             // Decline Crunch (6)
        "3D0C7C75" => reps(RepsExercise::GobletSquat, LoadReading::Absolute),               // Goblet Squat (6)
        "36E8F14E" => reps(RepsExercise::HammerCurlCable, LoadReading::Absolute),           // Hammer Curl (Cable) (6)
        "11A123F3" => reps(RepsExercise::SeatedLegCurlMachine, LoadReading::Absolute),      // Seated Leg Curl (Machine) (6)
        "629AE73D" => reps(RepsExercise::SingleLegExtensions, LoadReading::Absolute),       // Single Leg Extensions (6)
        "20C1A3CB" => reps(RepsExercise::SplitSquatDumbbell, LoadReading::Absolute),        // Split Squat (Dumbbell) (6)
        "6BE68B62" => reps(RepsExercise::VUp, LoadReading::Relative),                       // V Up (6)
        "fb0ab15d-b64e-4aaa-9028-cf8d28380697" => reps(RepsExercise::WallClimbs, LoadReading::Relative), // Wall Climbs (6)
        "E8D86EE8" => reps(RepsExercise::BandPullaparts, LoadReading::BandResistance),      // Band Pullaparts (5)
        "3e585d65-ec43-4689-bc33-d5257aaaecb5" => reps(RepsExercise::BandedScapulaProtraction, LoadReading::BandResistance), // Banded Scapula Protraction (5)
        "37FCC2BB" => reps(RepsExercise::BicepCurlDumbbell, LoadReading::Absolute),         // Bicep Curl (Dumbbell) (5)
        "30F03BF0" => reps(RepsExercise::FrontLeverRaise, LoadReading::Relative),           // Front Lever Raise (5)
        "ca868acf-25c1-4537-b0f5-08850d79665d" => reps(RepsExercise::PikePullThrough, LoadReading::Relative), // Pike Pull Through (5)
        "06d2c3e9-bd3b-409c-a729-89b6c4a4b543" => reps(RepsExercise::ShoulderInternalExternalRotation, LoadReading::Absolute), // Shoulder Internal/External Rotation (5)
        "4567e678-1184-4306-9e18-66cc5c59e81d" => reps(RepsExercise::HipSnatch, LoadReading::Absolute), // Hip Snatch (4)
        "07B38369" => reps(RepsExercise::InclineBenchPressDumbbell, LoadReading::Absolute), // Incline Bench Press (Dumbbell) (4)
        "f60e7f99-d56d-4f36-b2d0-6f60ab36a244" => reps(RepsExercise::MuscleSnatchIntoOverheadSquat, LoadReading::Absolute), // Muscle Snatch Into Overhead Squat (4)
        "2CFED196" => reps(RepsExercise::OverheadSquat, LoadReading::Absolute),             // Overhead Squat (4)
        "31436F5D" => reps(RepsExercise::PlankPushup, LoadReading::Relative),               // Plank Pushup (4)
        "7014f03f-04d9-4b0b-90bc-adcdbc958fba" => reps(RepsExercise::RingPushups, LoadReading::Relative), // Ring Pushups (4)
        "aa6ea7c8-197d-4895-ac2a-a3ee9877d027" => reps(RepsExercise::RingRows, LoadReading::Relative), // Ring Rows (4)
        "022DF610" => reps(RepsExercise::SitUp, LoadReading::Relative),                     // Sit Up (4)
        "5ca69118-ce96-4cc1-a6b6-e1554698b6a6" => reps(RepsExercise::SnatchBalance, LoadReading::Absolute), // Snatch Balance (4)
        "66786745-7825-45df-bdc9-25430cdaf820" => reps(RepsExercise::SnatchGripBehindTheNeckPress, LoadReading::Absolute), // Snatch-Grip Behind The Neck Press (4)
        "BD0AD077" => reps(RepsExercise::BirdDog, LoadReading::Relative),                   // Bird Dog (3)
        "86B00DDE" => reps(RepsExercise::BurpeeOverTheBar, LoadReading::Relative),          // Burpee Over the Bar (3)
        "9DCE2D64" => reps(RepsExercise::ButterflyPecDeck, LoadReading::Absolute),          // Butterfly (Pec Deck) (3)
        "7EB3F7C3" => reps(RepsExercise::ChestPressMachine, LoadReading::Absolute),         // Chest Press (Machine) (3)
        "F21D5693" => reps(RepsExercise::ChestSupportedYRaiseDumbbell, LoadReading::Relative), // Chest Supported Y Raise (Dumbbell) (3)
        "D3095577" => reps(RepsExercise::CleanAndPress, LoadReading::Absolute),             // Clean and Press (3)
        "5F4E6DD3" => reps(RepsExercise::DeadliftDumbbell, LoadReading::Absolute),          // Deadlift (Dumbbell) (3)
        "1479354e-a862-42dd-87e3-3f0ecaa7c8c0" => reps(RepsExercise::DeficitPushups, LoadReading::Relative), // Deficit Pushups (3)
        "F3717B0E" => reps(RepsExercise::DumbbellSnatch, LoadReading::Absolute),            // Dumbbell Snatch (3)
        "47B036EF" => reps(RepsExercise::FrontRaiseBand, LoadReading::BandResistance),      // Front Raise (Band) (3)
        "4180C405" => reps(RepsExercise::GoodMorningBarbell, LoadReading::Absolute),        // Good Morning (Barbell) (3)
        "08590920" => reps(RepsExercise::HangingKneeRaise, LoadReading::Relative),          // Hanging Knee Raise (3)
        "5260995e-36b5-49f0-b6a2-eecd5a4b9883" => reps(RepsExercise::KettlebellCleanAndPress, LoadReading::Absolute), // Kettlebell Clean and Press (3)
        "DF200976" => reps(RepsExercise::LateralRaiseBand, LoadReading::BandResistance),    // Lateral Raise (Band) (3)
        "C7973E0E" => reps(RepsExercise::LegPressMachine, LoadReading::Absolute),           // Leg Press (Machine) (3)
        "54E60954" => reps(RepsExercise::OverheadPlateRaise, LoadReading::Absolute),        // Overhead Plate Raise (3)
        "56808FD2" => reps(RepsExercise::PullUp, LoadReading::RelativeNegated),             // Pull Up (Band) (3)
        "8BAB2735" => reps(RepsExercise::SeatedInclineCurlDumbbell, LoadReading::Absolute), // Seated Incline Curl (Dumbbell) (3)
        "D0C4A899" => reps(RepsExercise::SingleArmCableRow, LoadReading::Absolute),         // Single Arm Cable Row (3)
        "F5DEF1EB" => reps(RepsExercise::SissySquatWeighted, LoadReading::Relative),        // Sissy Squat (Weighted) (3)
        "4f870422-92aa-4fb9-8ee5-12352c1dfe50" => reps(RepsExercise::SleeperStretch, LoadReading::Relative), // Sleeper Stretch (3)
        "90E506D5" => reps(RepsExercise::ThrusterBarbell, LoadReading::Absolute),           // Thruster (Barbell) (3)
        "75BAC5C3" => reps(RepsExercise::ToeTouch, LoadReading::Relative),                  // Toe Touch (3)
        "B94E35E1" => reps(RepsExercise::ToesToBar, LoadReading::Relative),                 // Toes to Bar (3)
        "c319b826-fcaa-4f35-96f6-9dcd2a735201" => reps(RepsExercise::WeightedJumpSquat, LoadReading::Absolute), // Weighted Jump Squat (3)
        "35747eea-14d4-4833-bb09-24ecf70a0896" => reps(RepsExercise::AboveAndBelowTheKneePauseSnatch, LoadReading::Absolute), // Above And Below The Knee Pause Snatch (2)
        "07a0cc37-90f1-40e7-af5b-9d05cf5256cd" => reps(RepsExercise::DownwardDogToPlancheLean, LoadReading::Relative), // Downward Dog To Planche Lean (2)
        "6A6C31A5" => reps(RepsExercise::LatPulldownCable, LoadReading::Absolute),          // Lat Pulldown (Cable) (2)
        "D8460FA6" => reps(RepsExercise::ReverseWristCurlDumbbell, LoadReading::Absolute),  // Reverse Wrist Curl (Dumbbell) (2)
        "C7AE420A" => reps(RepsExercise::ScapularPullUps, LoadReading::Relative),           // Scapular Pull Ups (1)
        "0b9db86f-666c-46fd-b567-2918c3c269cd" => reps(RepsExercise::SerratusRock, LoadReading::Relative), // Serratus Rock (1)
        _ => return None,
    };
    Some(mapped)
}

#[cfg(test)]
mod tests {
    use super::{LoadReading, lookup};

    /// The four templates whose sets are refused outright, and the count that
    /// makes the model of record's 16 add up.
    #[test]
    fn band_resistance_is_refused_and_band_assistance_is_not() {
        for template in ["3e585d65-ec43-4689-bc33-d5257aaaecb5", "E8D86EE8", "47B036EF", "DF200976"]
        {
            let mapped = lookup(template).expect("a band template is mapped");
            assert_eq!(mapped.load, LoadReading::BandResistance, "{template}");
        }

        // `Pull Up (Band)` is assistance, not resistance.
        let banded_pull_up = lookup("56808FD2").expect("Pull Up (Band) is mapped");
        assert_eq!(banded_pull_up.load, LoadReading::RelativeNegated);
    }

    /// The collapse the whole mapping exists for: assisted and unassisted are
    /// one exercise, one series.
    #[test]
    fn assisted_and_unassisted_are_one_exercise() {
        let plain = lookup("1B2B1E7C").expect("Pull Up is mapped");
        let assisted = lookup("2C37EC5E").expect("Pull Up (Assisted) is mapped");
        let banded = lookup("56808FD2").expect("Pull Up (Band) is mapped");
        assert_eq!(plain.exercise, assisted.exercise);
        assert_eq!(plain.exercise, banded.exercise);
        assert_eq!(plain.load, LoadReading::Relative);
        assert_eq!(assisted.load, LoadReading::RelativeNegated);

        let dip = lookup("6FCD7755").expect("Chest Dip is mapped");
        let dip_assisted = lookup("E9E4089F").expect("Chest Dip (Assisted) is mapped");
        assert_eq!(dip.exercise, dip_assisted.exercise);
    }

    /// Two template ids, one exercise. Neither title keyed anything.
    #[test]
    fn both_overhead_squat_templates_reach_one_exercise() {
        let builtin = lookup("2CFED196").expect("the builtin Overhead Squat is mapped");
        let custom =
            lookup("a500417a-5f3d-4061-aa9b-0635181868ec").expect("the custom one is mapped");
        assert_eq!(builtin.exercise, custom.exercise);
    }

    /// The pair that shows rule 2 is a judgement about the movement rather than
    /// a pattern match on the title.
    #[test]
    fn a_single_leg_hinge_is_relative_and_a_barbell_one_is_not() {
        let barbell = lookup("2B4B7310").expect("Romanian Deadlift (Barbell) is mapped");
        let single_leg =
            lookup("937292AB").expect("Single Leg Romanian Deadlift (Dumbbell) is mapped");
        assert_eq!(barbell.load, LoadReading::Absolute);
        assert_eq!(single_leg.load, LoadReading::Relative);
    }

    /// Our category beats the source's, and the mapping is where that is
    /// decided — so the nine zero distances are never read.
    #[test]
    fn the_sled_is_a_duration_exercise() {
        let sled = lookup("7757171F").expect("Sled Push is mapped");
        assert_eq!(sled.exercise.measure(), "duration");
    }

    #[test]
    fn an_unmapped_template_resolves_to_nothing() {
        assert!(lookup("not-a-template").is_none());
    }
}
