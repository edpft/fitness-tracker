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
//! **`Relative` is for the movements this source also names in an assisted
//! form.** Seven templates and four exercises, every one of them a pull-up, a
//! chin-up or a dip, and that is the whole of the relative family. What puts
//! them there is not that the movement can be done unloaded — plenty of
//! absolute ones can — but that Hevy sells the same movement twice, once plain
//! and once assisted, so the axis has to run through zero in both directions
//! for the pair to be one series.
//!
//! **`Absolute` is everything else**, where the number is external load and a
//! zero is none of it. That is a real observation rather than an absence, which
//! is what `Kg::NONE` says in as many words, and the corpus bears it out: 43 of
//! its sets carry a zero on an absolute template — twelve hammer twists, four
//! Bulgarian split squats, three sissy squats. Nothing about them is wrong and
//! none of them refuses.
//!
//! The other 50 of the corpus's 93 zeros sit on `Chest Dip (Assisted)` and
//! `Pull Up (Assisted)`, where zero is no assistance — the same set as an
//! unassisted rep, which is the collapse the relative axis exists to make.
//!
//! **What refuses is a value that is not a mass**: text that will not parse, or
//! a negative on an absolute template, since `Kg` is unsigned. The corpus holds
//! neither, which is why all 3,779 of its sets translate.
//!
//! This section used to claim the opposite — that `Absolute` meant no unloaded
//! version of the movement existed, that a zero on such a template refused, and
//! that the table was right if and only if seven of the 93 did. None of it was
//! ever true: `load_of` has read a zero as `Load::UNLOADED` since the first
//! commit of this module, and the single-leg Romanian deadlift it offered as
//! the specimen `Relative` entry has been `Absolute` for just as long. It is
//! recorded here so the rule does not get reinstated from the paragraph that
//! described it.
//!
//! **An assisted variant negates.** Hevy has no assistance concept: assisted
//! movements are separately named exercises carrying a positive weight. So
//! `RelativeNegated` turns 20 into −20 and puts assistance and added weight on
//! one axis, which is the mapping's reason to exist.
//!
//! **Band resistance is read as load, and the limitation is declared rather
//! than enforced.** Band tension varies through the range of motion and nothing
//! records the mechanism, so a banded lateral raise at 7kg is not comparable
//! with a cable one at 7kg. Four templates and 16 sets are affected, and they
//! are `Absolute` like any other resistance: the number is what the source
//! said, and `band_resistance_is_load_and_band_assistance_is_negative` pins it.
//!
//! Refusing them was described here once and never implemented, which was the
//! better outcome — comparability is § 6's business, and a set that cannot be
//! compared is still a set that happened.
//!
//! `Pull Up (Band)` is a different thing: that is band *assistance*, and it
//! maps to `PullUp` negated like any other assisted pull-up. That band and
//! machine assistance are not comparable is likewise declared in the model of
//! record rather than acted on here.
//!
//! **A mapped template is not the same as a performed one.** Seven entries here
//! have never appeared in a workout, for two different reasons.
//!
//! Four the operator created in Hevy on 2026-08-20, because it had no exercise
//! for the movements he wanted and he had been logging the nearest thing it
//! offered instead. Both now exist and both stay distinct — `Pull Up
//! (Assisted)` is an assisted pull-up however many neutral-grip pull-ups were
//! recorded under it. Correcting those workouts is the edit overlay's job, not
//! this table's.
//!
//! Three are builtin templates he has simply never used, mapped because the
//! autumn block's slots name the movements: a barbell bench press, a barbell
//! skullcrusher and a barbell Bulgarian split squat. Nothing was created and
//! nothing was logged under a stand-in.
//!
//! Either way the slots these fill are reported underivable until they have
//! been performed (FR-011), which is the honest answer for an exercise with no
//! history rather than a defect.
//!
//! **What he has been logging them under stays mapped to what it says.** The
//! record holds `Pull Up (Assisted)`, `Cable Twist (Up to down)` and
//! `Stretching` because those are the templates he picked as the nearest thing
//! Hevy offered. Reading them as the movement he meant would be this table
//! asserting something the source never said; the substitution is a correction
//! to those workouts, and belongs in the edit overlay rather than here.
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
    exercise::{DistanceExercise, DurationExercise, RepsExercise},
};

/// How to read the weight column for one exercise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadReading {
    /// Adding weight is the whole progression. The number is external load and
    /// none is a real answer.
    Absolute,
    /// Assistance is conventionally available, so the axis runs through zero in
    /// both directions. Zero is plain bodyweight.
    Relative,
    /// An assisted variant: the source's positive number is assistance, so it is
    /// negated onto the relative axis.
    RelativeNegated,
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

/// The mapping. 141 templates onto 135 exercises, many-to-one.
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
/// purpose, as do a weighted variant and its unweighted form, and merging them
/// would delete the per-template comment saying which source template each
/// covers. `too_many_lines` is allowed for the reason a phone book is long.
/// Neither is a forbidden lint, and neither is hiding a defect.
#[allow(clippy::too_many_lines, clippy::match_same_arms)]
pub fn lookup(template_id: &str) -> Option<Mapped> {
    let mapped = match template_id {
        "D04AC939" => reps(RepsExercise::SquatBarbell, LoadReading::Absolute), // Squat (Barbell) (377)
        "E9E4089F" => reps(RepsExercise::ChestDip, LoadReading::RelativeNegated), // Chest Dip (Assisted) (277)
        "C6272009" => reps(RepsExercise::DeadliftBarbell, LoadReading::Absolute), // Deadlift (Barbell) (196)
        "2C37EC5E" => reps(RepsExercise::PullUp, LoadReading::RelativeNegated), // Pull Up (Assisted) (159)
        "5046D0A9" => reps(RepsExercise::FrontSquat, LoadReading::Absolute),    // Front Squat (147)
        "4F942934" => reps(RepsExercise::PreacherCurlBarbell, LoadReading::Absolute), // Preacher Curl (Barbell) (121)
        "9202CC23" => reps(
            RepsExercise::SeatedWristExtensionBarbell,
            LoadReading::Absolute,
        ), // Seated Wrist Extension (Barbell) (119)
        "21310F5F" => reps(RepsExercise::TricepsExtensionCable, LoadReading::Absolute), // Triceps Extension (Cable) (117)
        "B5EFBF9C" => reps(
            RepsExercise::OverheadTricepsExtensionCable,
            LoadReading::Absolute,
        ), // Overhead Triceps Extension (Cable) (109)
        "2B4B7310" => reps(RepsExercise::RomanianDeadliftBarbell, LoadReading::Absolute), // Romanian Deadlift (Barbell) (103)
        "1B2B1E7C" => reps(RepsExercise::PullUp, LoadReading::Relative), // Pull Up (97)
        "425805F4" => reps(RepsExercise::InvertedRow, LoadReading::Absolute), // Inverted Row (85)
        "6FCD7755" => reps(RepsExercise::ChestDip, LoadReading::Relative), // Chest Dip (84)
        "7B8D84E8" => reps(RepsExercise::OverheadPressBarbell, LoadReading::Absolute), // Overhead Press (Barbell) (82)
        "8A2E6481" => reps(RepsExercise::LowRowSuspension, LoadReading::Absolute), // Low Row (Suspension) (77)
        "A2D838BD" => reps(RepsExercise::CableTwistUpToDown, LoadReading::Absolute), // Cable Twist (Up to down) (75)
        "6AC96645" => reps(RepsExercise::OverheadPressDumbbell, LoadReading::Absolute), // Overhead Press (Dumbbell) (64)
        "BE289E45" => reps(RepsExercise::LateralRaiseCable, LoadReading::Absolute), // Lateral Raise (Cable) (63)
        "56092DD1" => reps(RepsExercise::BoxJump, LoadReading::Absolute),           // Box Jump (59)
        "FAB6EB2F" => reps(RepsExercise::PreacherCurlDumbbell, LoadReading::Absolute), // Preacher Curl (Dumbbell) (59)
        "75A4F6C4" => reps(RepsExercise::LegExtensionMachine, LoadReading::Absolute), // Leg Extension (Machine) (54)
        "D2387AB1" => reps(
            RepsExercise::StraightArmLatPulldownCable,
            LoadReading::Absolute,
        ), // Straight Arm Lat Pulldown (Cable) (51)
        "1006DF48" => reps(RepsExercise::WristFlexionDumbbell, LoadReading::Absolute), // Seated Palms Up Wrist Curl (49)
        "c6e09263-5d20-450d-a219-95ba47ee8305" => reps(RepsExercise::Pogo, LoadReading::Absolute), // Pogo (45)
        "7E3BC8B6" => reps(RepsExercise::HammerCurlDumbbell, LoadReading::Absolute), // Hammer Curl (Dumbbell) (41)
        "2F8D3067" => reps(RepsExercise::TricepsExtensionBarbell, LoadReading::Absolute), // Triceps Extension (Barbell) (38)
        "F8A0FCCA" => reps(RepsExercise::KettlebellSwing, LoadReading::Absolute), // Kettlebell Swing (36)
        "43573BB8" => duration(DurationExercise::AirBike, LoadReading::Absolute), // Air Bike (32)
        "BB792A36" => reps(RepsExercise::Burpee, LoadReading::Absolute),          // Burpee (31)
        "FB09C938" => reps(RepsExercise::Snatch, LoadReading::Absolute),          // Snatch (31)
        "10313AFD" => reps(RepsExercise::ThrusterKettlebell, LoadReading::Absolute), // Thruster (Kettlebell) (31)
        "422B08F1" => reps(RepsExercise::LateralRaiseDumbbell, LoadReading::Absolute), // Lateral Raise (Dumbbell) (29)
        "B8127AD1" => reps(RepsExercise::LyingLegCurlMachine, LoadReading::Absolute), // Lying Leg Curl (Machine) (29)
        "5c98d763-9ceb-412c-8365-18110f9d5897" => {
            duration(DurationExercise::NinetyNinety, LoadReading::Absolute)
        } // 90/90 (28)
        "e2182af0-2577-4603-8e70-18273be1d48b" => {
            duration(DurationExercise::CouchStretch, LoadReading::Absolute)
        } // Couch Stretch (28)
        "091737FA" => reps(
            RepsExercise::BackExtensionHyperextension,
            LoadReading::Absolute,
        ), // Back Extension (Weighted Hyperextension) (26)
        "93472AC1" => reps(
            RepsExercise::SingleLegRomanianDeadliftBarbell,
            LoadReading::Absolute,
        ), // Single Leg Romanian Deadlift (Barbell) (26)
        "A733CC5B" => distance(
            DistanceExercise::WalkingLungeDumbbell,
            LoadReading::Absolute,
        ), // Walking Lunge (Dumbbell) (26)
        "118ed850-2aa7-4010-ab93-bdf8b0352660" => {
            reps(RepsExercise::HammerTwists, LoadReading::Absolute)
        } // Hammer Twists (24)
        "4E5257DE" => reps(
            RepsExercise::LatPulldownCloseGripCable,
            LoadReading::Absolute,
        ), // Lat Pulldown - Close Grip (Cable) (24)
        "542F3CD5" => reps(RepsExercise::PushPress, LoadReading::Absolute), // Push Press (23)
        // Hevy's dumbbell template, though the record only ever called it
        // `Bulgarian Split Squat`: the title informs this table and never keys
        // it, and the barbell variant is a separate template below. Four of the
        // 21 sets carry a zero, which on an absolute reading is the movement
        // done unloaded rather than a missing number.
        "B5D3A742" => reps(
            RepsExercise::BulgarianSplitSquatDumbbell,
            LoadReading::Absolute,
        ), // Bulgarian Split Squat (21)
        "ABEC557F" => reps(RepsExercise::ShrugDumbbell, LoadReading::Absolute), // Shrug (Dumbbell) (21)
        "AC1BB830" => distance(DistanceExercise::Running, LoadReading::Absolute), // Running (19)
        "50DFDFAB" => reps(
            RepsExercise::InclineBenchPressBarbell,
            LoadReading::Absolute,
        ), // Incline Bench Press (Barbell) (18)
        "108D7A14" => reps(RepsExercise::NordicHamstringsCurls, LoadReading::Absolute), // Nordic Hamstrings Curls (18)
        "DE68C825" => reps(
            RepsExercise::SingleArmLateralRaiseCable,
            LoadReading::Absolute,
        ), // Single Arm Lateral Raise (Cable) (18)
        "B9380898" => duration(DurationExercise::DeadHang, LoadReading::Absolute), // Dead Hang (17)
        "527DA061" => duration(DurationExercise::Stretching, LoadReading::Absolute), // Stretching (16)
        "50C613D0" => distance(DistanceExercise::FarmersWalk, LoadReading::Absolute), // Farmers Walk (15)
        "8347DFD1" => reps(
            RepsExercise::SingleArmTricepExtensionDumbbell,
            LoadReading::Absolute,
        ), // Single Arm Tricep Extension (Dumbbell) (15)
        "D928C232" => reps(RepsExercise::Crunch, LoadReading::Absolute), // Crunch (Weighted) (11)
        "914F3A96" => reps(
            RepsExercise::ChestSupportedInclineRowDumbbell,
            LoadReading::Absolute,
        ), // Chest Supported Incline Row (Dumbbell) (10)
        "F4E77594" => reps(RepsExercise::HangSnatch, LoadReading::Absolute), // Hang Snatch (10)
        "040BA2E3" => duration(DurationExercise::JumpRope, LoadReading::Absolute), // Jump Rope (10)
        "D8911FC4" => reps(RepsExercise::DeadBug, LoadReading::Absolute), // Dead Bug (9)
        "B537D09F" => reps(RepsExercise::LungeDumbbell, LoadReading::Absolute), // Lunge (Dumbbell) (9)
        "1B89CA1B" => reps(RepsExercise::RenegadeRowDumbbell, LoadReading::Absolute), // Renegade Row (Dumbbell) (9)
        "937292AB" => reps(
            RepsExercise::SingleLegRomanianDeadliftDumbbell,
            LoadReading::Absolute,
        ), // Single Leg Romanian Deadlift (Dumbbell) (9)
        "7757171F" => duration(DurationExercise::SledPush, LoadReading::Absolute), // Sled Push (9)
        "2b9f6f49-71cf-45bd-88d8-a6a2bb6c0814" => reps(
            RepsExercise::BackSquatWithSnatchPushPress,
            LoadReading::Absolute,
        ), // Back Squat w/ Snatch Push Press (8)
        "29083183" => reps(RepsExercise::ChinUp, LoadReading::Relative),           // Chin Up (8)
        "266f2cb6-b4a5-45a9-9fc6-722386547616" => {
            reps(RepsExercise::DropSnatch, LoadReading::Absolute)
        } // Drop Snatch (8)
        "756EE329" => reps(RepsExercise::FloorPressDumbbell, LoadReading::Absolute), // Floor Press (Dumbbell) (8)
        "BE3615CF" => duration(DurationExercise::HandstandHold, LoadReading::Absolute), // Handstand Hold (8)
        "39796be6-52b2-49e5-a27f-26fb0009260c" => {
            reps(RepsExercise::HangHighPull, LoadReading::Absolute)
        } // Hang High Pull (8)
        "F99C211D" => reps(RepsExercise::KettlebellClean, LoadReading::Absolute), // Kettlebell Clean (8)
        "a500417a-5f3d-4061-aa9b-0635181868ec" => {
            reps(RepsExercise::OverheadSquat, LoadReading::Absolute)
        } // Overhead Squat (8)
        "018ADC12" => reps(RepsExercise::PendlayRowBarbell, LoadReading::Absolute), // Pendlay Row (Barbell) (8)
        "856037db-34d2-41f5-b8b7-1ca15a7d348c" => {
            reps(RepsExercise::PowerMuscleSnatch, LoadReading::Absolute)
        } // Power Muscle Snatch (8)
        "392887AA" => reps(RepsExercise::PushUp, LoadReading::Absolute),            // Push Up (8)
        "2DBCA395" => reps(RepsExercise::BehindTheBackCurlCable, LoadReading::Absolute), // Behind the Back Curl (Cable) (7)
        "55E6546F" => reps(RepsExercise::BentOverRowBarbell, LoadReading::Absolute), // Bent Over Row (Barbell) (7)
        "C628D768" => reps(RepsExercise::PowerClean, LoadReading::Absolute), // Power Clean (7)
        "0393F233" => reps(
            RepsExercise::SeatedCableRowVGripCable,
            LoadReading::Absolute,
        ), // Seated Cable Row - V Grip (Cable) (7)
        "4F5866F8" => reps(
            RepsExercise::BackExtensionHyperextension,
            LoadReading::Absolute,
        ), // Back Extension (Hyperextension) (6)
        "A05C064D" => reps(RepsExercise::BackExtensionMachine, LoadReading::Absolute), // Back Extension (Machine) (6)
        "DDB29047" => reps(
            RepsExercise::BehindTheBackWristCurlBarbell,
            LoadReading::Absolute,
        ), // Behind the Back Wrist Curl (Barbell) (6)
        "DCF3B31B" => reps(RepsExercise::Crunch, LoadReading::Absolute),               // Crunch (6)
        "BC10A922" => reps(RepsExercise::DeclineCrunch, LoadReading::Absolute), // Decline Crunch (6)
        "3D0C7C75" => reps(RepsExercise::GobletSquat, LoadReading::Absolute),   // Goblet Squat (6)
        "36E8F14E" => reps(RepsExercise::HammerCurlCable, LoadReading::Absolute), // Hammer Curl (Cable) (6)
        "11A123F3" => reps(RepsExercise::SeatedLegCurlMachine, LoadReading::Absolute), // Seated Leg Curl (Machine) (6)
        "629AE73D" => reps(RepsExercise::SingleLegExtensions, LoadReading::Absolute), // Single Leg Extensions (6)
        "20C1A3CB" => reps(RepsExercise::SplitSquatDumbbell, LoadReading::Absolute), // Split Squat (Dumbbell) (6)
        "6BE68B62" => reps(RepsExercise::VUp, LoadReading::Absolute),                // V Up (6)
        "fb0ab15d-b64e-4aaa-9028-cf8d28380697" => {
            reps(RepsExercise::WallClimbs, LoadReading::Absolute)
        } // Wall Climbs (6)
        "E8D86EE8" => reps(RepsExercise::BandPullaparts, LoadReading::Absolute), // Band Pullaparts (5)
        "3e585d65-ec43-4689-bc33-d5257aaaecb5" => reps(
            RepsExercise::BandedScapulaProtraction,
            LoadReading::Absolute,
        ), // Banded Scapula Protraction (5)
        "37FCC2BB" => reps(RepsExercise::BicepCurlDumbbell, LoadReading::Absolute), // Bicep Curl (Dumbbell) (5)
        "30F03BF0" => reps(RepsExercise::FrontLeverRaise, LoadReading::Absolute), // Front Lever Raise (5)
        "ca868acf-25c1-4537-b0f5-08850d79665d" => {
            reps(RepsExercise::PikePullThrough, LoadReading::Absolute)
        } // Pike Pull Through (5)
        "06d2c3e9-bd3b-409c-a729-89b6c4a4b543" => reps(
            RepsExercise::ShoulderInternalExternalRotation,
            LoadReading::Absolute,
        ), // Shoulder Internal/External Rotation (5)
        "4567e678-1184-4306-9e18-66cc5c59e81d" => {
            reps(RepsExercise::HipSnatch, LoadReading::Absolute)
        } // Hip Snatch (4)
        "07B38369" => reps(
            RepsExercise::InclineBenchPressDumbbell,
            LoadReading::Absolute,
        ), // Incline Bench Press (Dumbbell) (4)
        "f60e7f99-d56d-4f36-b2d0-6f60ab36a244" => reps(
            RepsExercise::MuscleSnatchIntoOverheadSquat,
            LoadReading::Absolute,
        ), // Muscle Snatch Into Overhead Squat (4)
        "2CFED196" => reps(RepsExercise::OverheadSquat, LoadReading::Absolute), // Overhead Squat (4)
        "31436F5D" => reps(RepsExercise::PlankPushup, LoadReading::Absolute),   // Plank Pushup (4)
        "7014f03f-04d9-4b0b-90bc-adcdbc958fba" => {
            reps(RepsExercise::RingPushups, LoadReading::Absolute)
        } // Ring Pushups (4)
        "aa6ea7c8-197d-4895-ac2a-a3ee9877d027" => {
            reps(RepsExercise::RingRows, LoadReading::Absolute)
        } // Ring Rows (4)
        "022DF610" => reps(RepsExercise::SitUp, LoadReading::Absolute),         // Sit Up (4)
        "5ca69118-ce96-4cc1-a6b6-e1554698b6a6" => {
            reps(RepsExercise::SnatchBalance, LoadReading::Absolute)
        } // Snatch Balance (4)
        "66786745-7825-45df-bdc9-25430cdaf820" => reps(
            RepsExercise::SnatchGripBehindTheNeckPress,
            LoadReading::Absolute,
        ), // Snatch-Grip Behind The Neck Press (4)
        "BD0AD077" => reps(RepsExercise::BirdDog, LoadReading::Absolute),       // Bird Dog (3)
        "86B00DDE" => reps(RepsExercise::BurpeeOverTheBar, LoadReading::Absolute), // Burpee Over the Bar (3)
        "9DCE2D64" => reps(RepsExercise::ButterflyPecDeck, LoadReading::Absolute), // Butterfly (Pec Deck) (3)
        "7EB3F7C3" => reps(RepsExercise::ChestPressMachine, LoadReading::Absolute), // Chest Press (Machine) (3)
        "F21D5693" => reps(
            RepsExercise::ChestSupportedYRaiseDumbbell,
            LoadReading::Absolute,
        ), // Chest Supported Y Raise (Dumbbell) (3)
        "D3095577" => reps(RepsExercise::CleanAndPress, LoadReading::Absolute), // Clean and Press (3)
        "5F4E6DD3" => reps(RepsExercise::DeadliftDumbbell, LoadReading::Absolute), // Deadlift (Dumbbell) (3)
        "1479354e-a862-42dd-87e3-3f0ecaa7c8c0" => {
            reps(RepsExercise::DeficitPushups, LoadReading::Absolute)
        } // Deficit Pushups (3)
        "F3717B0E" => reps(RepsExercise::DumbbellSnatch, LoadReading::Absolute), // Dumbbell Snatch (3)
        "47B036EF" => reps(RepsExercise::FrontRaiseBand, LoadReading::Absolute), // Front Raise (Band) (3)
        "4180C405" => reps(RepsExercise::GoodMorningBarbell, LoadReading::Absolute), // Good Morning (Barbell) (3)
        "08590920" => reps(RepsExercise::HangingKneeRaise, LoadReading::Absolute), // Hanging Knee Raise (3)
        "5260995e-36b5-49f0-b6a2-eecd5a4b9883" => {
            reps(RepsExercise::KettlebellCleanAndPress, LoadReading::Absolute)
        } // Kettlebell Clean and Press (3)
        "DF200976" => reps(RepsExercise::LateralRaiseBand, LoadReading::Absolute), // Lateral Raise (Band) (3)
        "C7973E0E" => reps(RepsExercise::LegPressMachine, LoadReading::Absolute), // Leg Press (Machine) (3)
        "54E60954" => reps(RepsExercise::OverheadPlateRaise, LoadReading::Absolute), // Overhead Plate Raise (3)
        "56808FD2" => reps(RepsExercise::PullUp, LoadReading::RelativeNegated), // Pull Up (Band) (3)
        "8BAB2735" => reps(
            RepsExercise::SeatedInclineCurlDumbbell,
            LoadReading::Absolute,
        ), // Seated Incline Curl (Dumbbell) (3)
        "D0C4A899" => reps(RepsExercise::SingleArmCableRow, LoadReading::Absolute), // Single Arm Cable Row (3)
        "F5DEF1EB" => reps(RepsExercise::SissySquat, LoadReading::Absolute), // Sissy Squat (Weighted) (3)
        "4f870422-92aa-4fb9-8ee5-12352c1dfe50" => {
            reps(RepsExercise::SleeperStretch, LoadReading::Absolute)
        } // Sleeper Stretch (3)
        "90E506D5" => reps(RepsExercise::ThrusterBarbell, LoadReading::Absolute), // Thruster (Barbell) (3)
        "75BAC5C3" => reps(RepsExercise::ToeTouch, LoadReading::Absolute),        // Toe Touch (3)
        "B94E35E1" => reps(RepsExercise::ToesToBar, LoadReading::Absolute),       // Toes to Bar (3)
        "c319b826-fcaa-4f35-96f6-9dcd2a735201" => {
            reps(RepsExercise::WeightedJumpSquat, LoadReading::Absolute)
        } // Weighted Jump Squat (3)
        "35747eea-14d4-4833-bb09-24ecf70a0896" => reps(
            RepsExercise::AboveAndBelowTheKneePauseSnatch,
            LoadReading::Absolute,
        ), // Above And Below The Knee Pause Snatch (2)
        "07a0cc37-90f1-40e7-af5b-9d05cf5256cd" => reps(
            RepsExercise::DownwardDogToPlancheLean,
            LoadReading::Absolute,
        ), // Downward Dog To Planche Lean (2)
        "6A6C31A5" => reps(RepsExercise::LatPulldownCable, LoadReading::Absolute), // Lat Pulldown (Cable) (2)
        "D8460FA6" => reps(RepsExercise::WristExtensionDumbbell, LoadReading::Absolute), // Reverse Wrist Curl (Dumbbell) (2)
        "C7AE420A" => reps(RepsExercise::ScapularPullUps, LoadReading::Absolute), // Scapular Pull Ups (1)
        "0b9db86f-666c-46fd-b567-2918c3c269cd" => {
            reps(RepsExercise::SerratusRock, LoadReading::Absolute)
        } // Serratus Rock (1)

        // Created 2026-08-20 and not yet performed, so each is zero in a table
        // otherwise ordered by how often the corpus holds it. The template the
        // operator was using as a stand-in still reads as itself: `Pull Up
        // (Assisted)` above is an assisted pull-up, not a neutral-grip one.
        //
        // Each load reading is the template's own `type` rather than a guess.
        // `Neutral Grip Pull Up` is declared `bodyweight_assisted`, which is why
        // it negates like every other assisted variant — the number recorded is
        // weight taken off.
        "72f032e8-d574-4dab-9bb3-b76377b973f8" => reps(
            RepsExercise::NeutralGripPullUp,
            LoadReading::RelativeNegated,
        ), // Neutral Grip Pull Up (0)
        "48fdc527-90a4-4713-a766-ced702d9295c" => {
            reps(RepsExercise::BentOverCableChop, LoadReading::Absolute)
        } // Bent Over Cable Chop (0)
        "19ec9b58-0556-4a00-acbc-628a081d0be7" => duration(
            DurationExercise::SquattingGroinStretch,
            LoadReading::Absolute,
        ), // Squatting Groin Stretch (0)
        "e459e508-356d-41be-8fac-301909a91c6c" => duration(
            DurationExercise::StandingStraddleFold,
            LoadReading::Absolute,
        ), // Standing Straddle Fold (0)

        // Builtin templates the operator has never logged, added because the
        // autumn block's slots name the movements and an exercise has to exist
        // here before anything can be prescribed to it. Zero for the same
        // reason as the block above, and a different reason from it: nothing
        // was created in Hevy for these, they were simply never performed.
        //
        // All three are `Absolute`. No unloaded version of any of them is a
        // movement the operator would record — a bench press without a bar is
        // not a set — so a zero here would be a data error rather than an
        // observation.
        "79D0BB3A" => reps(RepsExercise::BenchPressBarbell, LoadReading::Absolute), // Bench Press (Barbell) (0)
        "875F585F" => reps(RepsExercise::SkullcrusherBarbell, LoadReading::Absolute), // Skullcrusher (EZ) (0)
        "0F24286A" => reps(
            RepsExercise::BulgarianSplitSquatBarbell,
            LoadReading::Absolute,
        ), // Bulgarian Split Squat (Barbell) (0)
        _ => return None,
    };
    Some(mapped)
}

#[cfg(test)]
mod tests {
    use super::{LoadReading, lookup};

    /// Band *resistance* and band *assistance* are different things, and the
    /// difference is which direction the band pulls.
    ///
    /// A banded lateral raise is resistance: the band is what you are working
    /// against, and the recorded number is read as external load like any
    /// other. A banded pull-up is assistance: the band takes weight off, so it
    /// negates onto the relative axis.
    #[test]
    fn band_resistance_is_load_and_band_assistance_is_negative() {
        for template in [
            "3e585d65-ec43-4689-bc33-d5257aaaecb5",
            "E8D86EE8",
            "47B036EF",
            "DF200976",
        ] {
            let mapped = lookup(template).expect("a band template is mapped");
            assert_eq!(mapped.load, LoadReading::Absolute, "{template}");
        }

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

    /// A weighted variant is the same movement carrying a load, exactly as an
    /// assisted one is — so the two templates reach one exercise and the number
    /// tells them apart.
    #[test]
    fn weighted_and_unweighted_variants_are_one_exercise() {
        let plain = lookup("DCF3B31B").expect("Crunch is mapped");
        let weighted = lookup("D928C232").expect("Crunch (Weighted) is mapped");
        assert_eq!(plain.exercise, weighted.exercise);

        let hyper = lookup("4F5866F8").expect("Back Extension (Hyperextension) is mapped");
        let weighted_hyper =
            lookup("091737FA").expect("Back Extension (Weighted Hyperextension) is mapped");
        assert_eq!(hyper.exercise, weighted_hyper.exercise);
    }

    /// A squat is absolute and a pull-up is relative, and the reason is
    /// convention rather than physics: nobody de-loads a squat, and everybody
    /// assists a pull-up.
    #[test]
    fn the_load_axis_is_bidirectional_only_where_assistance_is_conventional() {
        let squat = lookup("D04AC939").expect("Squat (Barbell) is mapped");
        let deadlift = lookup("C6272009").expect("Deadlift (Barbell) is mapped");
        let pull_up = lookup("1B2B1E7C").expect("Pull Up is mapped");
        let dip = lookup("6FCD7755").expect("Chest Dip is mapped");

        assert_eq!(squat.load, LoadReading::Absolute);
        assert_eq!(deadlift.load, LoadReading::Absolute);
        assert_eq!(pull_up.load, LoadReading::Relative);
        assert_eq!(dip.load, LoadReading::Relative);
    }

    /// Our category beats the source's, and the mapping is where that is
    /// decided — so the nine zero distances are never read.
    #[test]
    fn the_sled_is_a_duration_exercise() {
        let sled = lookup("7757171F").expect("Sled Push is mapped");
        assert_eq!(sled.exercise.measure(), "duration");
    }

    /// A run is ground covered, like a carry. The duration alongside it was an
    /// interval target rather than a measurement, and a target is prescription.
    #[test]
    fn a_run_is_measured_in_ground_covered() {
        let running = lookup("AC1BB830").expect("Running is mapped");
        let carry = lookup("50C613D0").expect("Farmers Walk is mapped");
        assert_eq!(running.exercise.measure(), "distance");
        assert_eq!(carry.exercise.measure(), "distance");
    }

    #[test]
    fn an_unmapped_template_resolves_to_nothing() {
        assert!(lookup("not-a-template").is_none());
    }
}
