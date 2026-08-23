//! Which Hevy template to write one of our exercises to.
//!
//! The other direction from [`super::mapping`], and deliberately **not** its
//! inverse. That table answers "what did this template say?" and is many-to-one;
//! this one answers "where does this instruction go?" and has to pick exactly
//! one template out of the several that read back to the same exercise.
//!
//! ## The sign is half the key
//!
//! Hevy has no assistance concept: an assisted movement is a separately named
//! template carrying a *positive* weight, which [`super::mapping::LoadReading`]
//! reads back as negative. Writing has to undo that, and undoing it wrongly is
//! the one defect in this feature that produces a plausible-looking session
//! instructing the opposite of what was prescribed — a chest dip at −7kg
//! written to `Chest Dip` as 7kg is a 14kg error in the direction of harder,
//! and nothing downstream would question it.
//!
//! So the key is the exercise *and the sign of the load*:
//!
//! - a template read as `Absolute` or `Relative` carries loads **≥ 0**, because
//!   its number is the load as recorded;
//! - a template read as `RelativeNegated` carries loads **≤ 0**, because its
//!   number is how much weight was taken off;
//! - zero is expressible on either, since no assistance and no added weight are
//!   the same set;
//! - and a load no template for that exercise can carry is [`Unwritable`]. It
//!   is never coerced onto the template of the wrong sign.
//!
//! `NeutralGripPullUp` is the exercise that shows this working. Its only
//! template is declared `bodyweight_assisted`, so it can express bodyweight and
//! any amount of assistance, and cannot express a weight belt. That is a real
//! limitation of the source reported as one, rather than a silently wrong
//! number.
//!
//! ## Where more than one template would do
//!
//! Four exercises have two templates on the same side of the axis. Two rules
//! settle all four, and both prefer the template that can carry more:
//!
//! - **The weighted variant wins.** `Crunch (Weighted)` over `Crunch`,
//!   `Back Extension (Weighted Hyperextension)` over its plain form. Both read
//!   back to one exercise, and only one of them has somewhere to put a load.
//! - **Machine assistance over band.** `Pull Up (Assisted)` over
//!   `Pull Up (Band)`. Decision 0004 holds that band and machine assistance are
//!   not comparable; the machine is what the account actually uses, 159 sets to
//!   3, and a prescribed number of kilograms means a stack rather than a band.
//!
//! `Overhead Squat` has two templates and no rule to separate them — one
//! builtin, one the operator created — so it takes the one the record uses more.
//!
//! ## Drift
//!
//! Nothing here is checked against [`super::mapping`] by the compiler, so a
//! template id could be mistyped into a live-looking entry. `writes_what_it_reads`
//! closes that: every entry is looked up in the forward table and has to come
//! back as the same exercise, on a load reading that agrees with the side of the
//! axis it was filed under. The two tables cannot disagree and stay green.

use domain::gym::{
    Exercise, Kg, Load,
    exercise::{DistanceExercise, DurationExercise, RepsExercise},
};

/// The templates an exercise can be written to, one per side of the load axis.
///
/// Two fields rather than one, because which is available is a fact about the
/// source's vocabulary rather than about any particular set. Both are optional:
/// an exercise Hevy only names in its assisted form has no `added` template, and
/// most exercises have no `assisted` one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Writable {
    /// Carries loads at or above zero. The number is written as it stands.
    pub added: Option<&'static str>,
    /// Carries loads at or below zero. The number is written negated, because
    /// what the source calls weight is assistance.
    pub assisted: Option<&'static str>,
}

/// A prescribed load the source has no way to state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{exercise} cannot be written at {load}: {reason}")]
pub struct Unwritable {
    pub exercise: Exercise,
    pub load: Load,
    reason: &'static str,
}

/// A load, resolved to the template that can carry it and the number to put
/// there.
///
/// The negation happens here and nowhere else, which is the whole point of
/// returning a pair: a caller holding one of these cannot apply the sign rule a
/// second time, or forget to apply it once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrittenLoad {
    pub template_id: &'static str,
    /// Unsigned, and correct for the template it is paired with.
    pub weight: Kg,
}

/// Where this exercise at this load goes, and what number to write.
///
/// # Errors
///
/// [`Unwritable`] where the source names no template on the side of the axis the
/// load falls on.
pub fn write_load(exercise: Exercise, load: Load) -> Result<WrittenLoad, Unwritable> {
    let templates = writable(exercise);
    let unwritable = |reason| Unwritable {
        exercise,
        load,
        reason,
    };

    let grams = match load {
        Load::Absolute(mass) => i64::try_from(mass.as_grams())
            .map_err(|_| unwritable("the load is larger than a weight can be"))?,
        Load::Relative(delta) => delta.as_grams(),
    };

    // Zero first, and on either template. No assistance and no added weight are
    // the same set, so an exercise the source only names in one direction can
    // still state it.
    if grams == 0 {
        return templates
            .added
            .or(templates.assisted)
            .map(|template_id| WrittenLoad {
                template_id,
                weight: Kg::NONE,
            })
            .ok_or_else(|| unwritable("the source names no template for it at all"));
    }

    if grams > 0 {
        let template_id = templates
            .added
            .ok_or_else(|| unwritable("the source names it only in its assisted form"))?;
        return Ok(WrittenLoad {
            template_id,
            weight: Kg::from_grams(grams.unsigned_abs()),
        });
    }

    let template_id = templates
        .assisted
        .ok_or_else(|| unwritable("the source names no assisted form of it"))?;
    Ok(WrittenLoad {
        template_id,
        weight: Kg::from_grams(grams.unsigned_abs()),
    })
}

/// The table. Total over the vocabulary, and exhaustive per measure, so adding
/// an exercise is a compile error until someone says where it is written.
///
/// The comment on each entry is the template's title, which informs this table
/// and never keys it — the same rule [`super::mapping`] is written under.
#[allow(
    clippy::too_many_lines,
    reason = "a phone book is long; the alternative is a shorter table that says less"
)]
const fn writable(exercise: Exercise) -> Writable {
    match exercise {
        Exercise::Reps(exercise) => match exercise {
            RepsExercise::AboveAndBelowTheKneePauseSnatch => Writable {
                added: Some("35747eea-14d4-4833-bb09-24ecf70a0896"),
                assisted: None,
            }, // Above And Below The Knee Pause Snatch
            RepsExercise::BackExtensionHyperextension => Writable {
                added: Some("091737FA"),
                assisted: None,
            }, // Back Extension (Weighted Hyperextension)
            RepsExercise::BackExtensionMachine => Writable {
                added: Some("A05C064D"),
                assisted: None,
            }, // Back Extension (Machine)
            RepsExercise::BackSquatWithSnatchPushPress => Writable {
                added: Some("2b9f6f49-71cf-45bd-88d8-a6a2bb6c0814"),
                assisted: None,
            }, // Back Squat w/ Snatch Push Press
            RepsExercise::BandPullaparts => Writable {
                added: Some("E8D86EE8"),
                assisted: None,
            }, // Band Pullaparts
            RepsExercise::BandedScapulaProtraction => Writable {
                added: Some("3e585d65-ec43-4689-bc33-d5257aaaecb5"),
                assisted: None,
            }, // Banded Scapula Protraction
            RepsExercise::BehindTheBackCurlCable => Writable {
                added: Some("2DBCA395"),
                assisted: None,
            }, // Behind the Back Curl (Cable)
            RepsExercise::BehindTheBackWristCurlBarbell => Writable {
                added: Some("DDB29047"),
                assisted: None,
            }, // Behind the Back Wrist Curl (Barbell)
            RepsExercise::BentOverCableChop => Writable {
                added: Some("48fdc527-90a4-4713-a766-ced702d9295c"),
                assisted: None,
            }, // Bent Over Cable Chop
            RepsExercise::BentOverRowBarbell => Writable {
                added: Some("55E6546F"),
                assisted: None,
            }, // Bent Over Row (Barbell)
            RepsExercise::BicepCurlDumbbell => Writable {
                added: Some("37FCC2BB"),
                assisted: None,
            }, // Bicep Curl (Dumbbell)
            RepsExercise::BirdDog => Writable {
                added: Some("BD0AD077"),
                assisted: None,
            }, // Bird Dog
            RepsExercise::BoxJump => Writable {
                added: Some("56092DD1"),
                assisted: None,
            }, // Box Jump
            RepsExercise::BulgarianSplitSquat => Writable {
                added: Some("B5D3A742"),
                assisted: None,
            }, // Bulgarian Split Squat
            RepsExercise::Burpee => Writable {
                added: Some("BB792A36"),
                assisted: None,
            }, // Burpee
            RepsExercise::BurpeeOverTheBar => Writable {
                added: Some("86B00DDE"),
                assisted: None,
            }, // Burpee Over the Bar
            RepsExercise::ButterflyPecDeck => Writable {
                added: Some("9DCE2D64"),
                assisted: None,
            }, // Butterfly (Pec Deck)
            RepsExercise::CableTwistUpToDown => Writable {
                added: Some("A2D838BD"),
                assisted: None,
            }, // Cable Twist (Up to down)
            RepsExercise::ChestDip => Writable {
                added: Some("6FCD7755"),
                assisted: Some("E9E4089F"),
            }, // Chest Dip, assisted: Chest Dip (Assisted)
            RepsExercise::ChestPressMachine => Writable {
                added: Some("7EB3F7C3"),
                assisted: None,
            }, // Chest Press (Machine)
            RepsExercise::ChestSupportedInclineRowDumbbell => Writable {
                added: Some("914F3A96"),
                assisted: None,
            }, // Chest Supported Incline Row (Dumbbell)
            RepsExercise::ChestSupportedYRaiseDumbbell => Writable {
                added: Some("F21D5693"),
                assisted: None,
            }, // Chest Supported Y Raise (Dumbbell)
            RepsExercise::ChinUp => Writable {
                added: Some("29083183"),
                assisted: None,
            }, // Chin Up
            RepsExercise::CleanAndPress => Writable {
                added: Some("D3095577"),
                assisted: None,
            }, // Clean and Press
            RepsExercise::Crunch => Writable {
                added: Some("D928C232"),
                assisted: None,
            }, // Crunch (Weighted)
            RepsExercise::DeadBug => Writable {
                added: Some("D8911FC4"),
                assisted: None,
            }, // Dead Bug
            RepsExercise::DeadliftBarbell => Writable {
                added: Some("C6272009"),
                assisted: None,
            }, // Deadlift (Barbell)
            RepsExercise::DeadliftDumbbell => Writable {
                added: Some("5F4E6DD3"),
                assisted: None,
            }, // Deadlift (Dumbbell)
            RepsExercise::DeclineCrunch => Writable {
                added: Some("BC10A922"),
                assisted: None,
            }, // Decline Crunch
            RepsExercise::DeficitPushups => Writable {
                added: Some("1479354e-a862-42dd-87e3-3f0ecaa7c8c0"),
                assisted: None,
            }, // Deficit Pushups
            RepsExercise::DownwardDogToPlancheLean => Writable {
                added: Some("07a0cc37-90f1-40e7-af5b-9d05cf5256cd"),
                assisted: None,
            }, // Downward Dog To Planche Lean
            RepsExercise::DropSnatch => Writable {
                added: Some("266f2cb6-b4a5-45a9-9fc6-722386547616"),
                assisted: None,
            }, // Drop Snatch
            RepsExercise::DumbbellSnatch => Writable {
                added: Some("F3717B0E"),
                assisted: None,
            }, // Dumbbell Snatch
            RepsExercise::FloorPressDumbbell => Writable {
                added: Some("756EE329"),
                assisted: None,
            }, // Floor Press (Dumbbell)
            RepsExercise::FrontLeverRaise => Writable {
                added: Some("30F03BF0"),
                assisted: None,
            }, // Front Lever Raise
            RepsExercise::FrontRaiseBand => Writable {
                added: Some("47B036EF"),
                assisted: None,
            }, // Front Raise (Band)
            RepsExercise::FrontSquat => Writable {
                added: Some("5046D0A9"),
                assisted: None,
            }, // Front Squat
            RepsExercise::GobletSquat => Writable {
                added: Some("3D0C7C75"),
                assisted: None,
            }, // Goblet Squat
            RepsExercise::GoodMorningBarbell => Writable {
                added: Some("4180C405"),
                assisted: None,
            }, // Good Morning (Barbell)
            RepsExercise::HammerCurlCable => Writable {
                added: Some("36E8F14E"),
                assisted: None,
            }, // Hammer Curl (Cable)
            RepsExercise::HammerCurlDumbbell => Writable {
                added: Some("7E3BC8B6"),
                assisted: None,
            }, // Hammer Curl (Dumbbell)
            RepsExercise::HammerTwists => Writable {
                added: Some("118ed850-2aa7-4010-ab93-bdf8b0352660"),
                assisted: None,
            }, // Hammer Twists
            RepsExercise::HangHighPull => Writable {
                added: Some("39796be6-52b2-49e5-a27f-26fb0009260c"),
                assisted: None,
            }, // Hang High Pull
            RepsExercise::HangSnatch => Writable {
                added: Some("F4E77594"),
                assisted: None,
            }, // Hang Snatch
            RepsExercise::HangingKneeRaise => Writable {
                added: Some("08590920"),
                assisted: None,
            }, // Hanging Knee Raise
            RepsExercise::HipSnatch => Writable {
                added: Some("4567e678-1184-4306-9e18-66cc5c59e81d"),
                assisted: None,
            }, // Hip Snatch
            RepsExercise::InclineBenchPressBarbell => Writable {
                added: Some("50DFDFAB"),
                assisted: None,
            }, // Incline Bench Press (Barbell)
            RepsExercise::InclineBenchPressDumbbell => Writable {
                added: Some("07B38369"),
                assisted: None,
            }, // Incline Bench Press (Dumbbell)
            RepsExercise::InvertedRow => Writable {
                added: Some("425805F4"),
                assisted: None,
            }, // Inverted Row
            RepsExercise::KettlebellClean => Writable {
                added: Some("F99C211D"),
                assisted: None,
            }, // Kettlebell Clean
            RepsExercise::KettlebellCleanAndPress => Writable {
                added: Some("5260995e-36b5-49f0-b6a2-eecd5a4b9883"),
                assisted: None,
            }, // Kettlebell Clean and Press
            RepsExercise::KettlebellSwing => Writable {
                added: Some("F8A0FCCA"),
                assisted: None,
            }, // Kettlebell Swing
            RepsExercise::LatPulldownCable => Writable {
                added: Some("6A6C31A5"),
                assisted: None,
            }, // Lat Pulldown (Cable)
            RepsExercise::LatPulldownCloseGripCable => Writable {
                added: Some("4E5257DE"),
                assisted: None,
            }, // Lat Pulldown - Close Grip (Cable)
            RepsExercise::LateralRaiseBand => Writable {
                added: Some("DF200976"),
                assisted: None,
            }, // Lateral Raise (Band)
            RepsExercise::LateralRaiseCable => Writable {
                added: Some("BE289E45"),
                assisted: None,
            }, // Lateral Raise (Cable)
            RepsExercise::LateralRaiseDumbbell => Writable {
                added: Some("422B08F1"),
                assisted: None,
            }, // Lateral Raise (Dumbbell)
            RepsExercise::LegExtensionMachine => Writable {
                added: Some("75A4F6C4"),
                assisted: None,
            }, // Leg Extension (Machine)
            RepsExercise::LegPressMachine => Writable {
                added: Some("C7973E0E"),
                assisted: None,
            }, // Leg Press (Machine)
            RepsExercise::LowRowSuspension => Writable {
                added: Some("8A2E6481"),
                assisted: None,
            }, // Low Row (Suspension)
            RepsExercise::LungeDumbbell => Writable {
                added: Some("B537D09F"),
                assisted: None,
            }, // Lunge (Dumbbell)
            RepsExercise::LyingLegCurlMachine => Writable {
                added: Some("B8127AD1"),
                assisted: None,
            }, // Lying Leg Curl (Machine)
            RepsExercise::MuscleSnatchIntoOverheadSquat => Writable {
                added: Some("f60e7f99-d56d-4f36-b2d0-6f60ab36a244"),
                assisted: None,
            }, // Muscle Snatch Into Overhead Squat
            RepsExercise::NeutralGripPullUp => Writable {
                added: None,
                assisted: Some("72f032e8-d574-4dab-9bb3-b76377b973f8"),
            }, // assisted only: Neutral Grip Pull Up
            RepsExercise::NordicHamstringsCurls => Writable {
                added: Some("108D7A14"),
                assisted: None,
            }, // Nordic Hamstrings Curls
            RepsExercise::OverheadPlateRaise => Writable {
                added: Some("54E60954"),
                assisted: None,
            }, // Overhead Plate Raise
            RepsExercise::OverheadPressBarbell => Writable {
                added: Some("7B8D84E8"),
                assisted: None,
            }, // Overhead Press (Barbell)
            RepsExercise::OverheadPressDumbbell => Writable {
                added: Some("6AC96645"),
                assisted: None,
            }, // Overhead Press (Dumbbell)
            RepsExercise::OverheadSquat => Writable {
                added: Some("a500417a-5f3d-4061-aa9b-0635181868ec"),
                assisted: None,
            }, // Overhead Squat
            RepsExercise::OverheadTricepsExtensionCable => Writable {
                added: Some("B5EFBF9C"),
                assisted: None,
            }, // Overhead Triceps Extension (Cable)
            RepsExercise::PendlayRowBarbell => Writable {
                added: Some("018ADC12"),
                assisted: None,
            }, // Pendlay Row (Barbell)
            RepsExercise::PikePullThrough => Writable {
                added: Some("ca868acf-25c1-4537-b0f5-08850d79665d"),
                assisted: None,
            }, // Pike Pull Through
            RepsExercise::PlankPushup => Writable {
                added: Some("31436F5D"),
                assisted: None,
            }, // Plank Pushup
            RepsExercise::Pogo => Writable {
                added: Some("c6e09263-5d20-450d-a219-95ba47ee8305"),
                assisted: None,
            }, // Pogo
            RepsExercise::PowerClean => Writable {
                added: Some("C628D768"),
                assisted: None,
            }, // Power Clean
            RepsExercise::PowerMuscleSnatch => Writable {
                added: Some("856037db-34d2-41f5-b8b7-1ca15a7d348c"),
                assisted: None,
            }, // Power Muscle Snatch
            RepsExercise::PreacherCurlBarbell => Writable {
                added: Some("4F942934"),
                assisted: None,
            }, // Preacher Curl (Barbell)
            RepsExercise::PreacherCurlDumbbell => Writable {
                added: Some("FAB6EB2F"),
                assisted: None,
            }, // Preacher Curl (Dumbbell)
            RepsExercise::PullUp => Writable {
                added: Some("1B2B1E7C"),
                assisted: Some("2C37EC5E"),
            }, // Pull Up, assisted: Pull Up (Assisted)
            RepsExercise::PushPress => Writable {
                added: Some("542F3CD5"),
                assisted: None,
            }, // Push Press
            RepsExercise::PushUp => Writable {
                added: Some("392887AA"),
                assisted: None,
            }, // Push Up
            RepsExercise::RenegadeRowDumbbell => Writable {
                added: Some("1B89CA1B"),
                assisted: None,
            }, // Renegade Row (Dumbbell)
            RepsExercise::RingPushups => Writable {
                added: Some("7014f03f-04d9-4b0b-90bc-adcdbc958fba"),
                assisted: None,
            }, // Ring Pushups
            RepsExercise::RingRows => Writable {
                added: Some("aa6ea7c8-197d-4895-ac2a-a3ee9877d027"),
                assisted: None,
            }, // Ring Rows
            RepsExercise::RomanianDeadliftBarbell => Writable {
                added: Some("2B4B7310"),
                assisted: None,
            }, // Romanian Deadlift (Barbell)
            RepsExercise::ScapularPullUps => Writable {
                added: Some("C7AE420A"),
                assisted: None,
            }, // Scapular Pull Ups
            RepsExercise::SeatedCableRowVGripCable => Writable {
                added: Some("0393F233"),
                assisted: None,
            }, // Seated Cable Row - V Grip (Cable)
            RepsExercise::SeatedInclineCurlDumbbell => Writable {
                added: Some("8BAB2735"),
                assisted: None,
            }, // Seated Incline Curl (Dumbbell)
            RepsExercise::SeatedLegCurlMachine => Writable {
                added: Some("11A123F3"),
                assisted: None,
            }, // Seated Leg Curl (Machine)
            RepsExercise::SeatedWristExtensionBarbell => Writable {
                added: Some("9202CC23"),
                assisted: None,
            }, // Seated Wrist Extension (Barbell)
            RepsExercise::SerratusRock => Writable {
                added: Some("0b9db86f-666c-46fd-b567-2918c3c269cd"),
                assisted: None,
            }, // Serratus Rock
            RepsExercise::ShoulderInternalExternalRotation => Writable {
                added: Some("06d2c3e9-bd3b-409c-a729-89b6c4a4b543"),
                assisted: None,
            }, // Shoulder Internal/External Rotation
            RepsExercise::ShrugDumbbell => Writable {
                added: Some("ABEC557F"),
                assisted: None,
            }, // Shrug (Dumbbell)
            RepsExercise::SingleArmCableRow => Writable {
                added: Some("D0C4A899"),
                assisted: None,
            }, // Single Arm Cable Row
            RepsExercise::SingleArmLateralRaiseCable => Writable {
                added: Some("DE68C825"),
                assisted: None,
            }, // Single Arm Lateral Raise (Cable)
            RepsExercise::SingleArmTricepExtensionDumbbell => Writable {
                added: Some("8347DFD1"),
                assisted: None,
            }, // Single Arm Tricep Extension (Dumbbell)
            RepsExercise::SingleLegExtensions => Writable {
                added: Some("629AE73D"),
                assisted: None,
            }, // Single Leg Extensions
            RepsExercise::SingleLegRomanianDeadliftBarbell => Writable {
                added: Some("93472AC1"),
                assisted: None,
            }, // Single Leg Romanian Deadlift (Barbell)
            RepsExercise::SingleLegRomanianDeadliftDumbbell => Writable {
                added: Some("937292AB"),
                assisted: None,
            }, // Single Leg Romanian Deadlift (Dumbbell)
            RepsExercise::SissySquat => Writable {
                added: Some("F5DEF1EB"),
                assisted: None,
            }, // Sissy Squat (Weighted)
            RepsExercise::SitUp => Writable {
                added: Some("022DF610"),
                assisted: None,
            }, // Sit Up
            RepsExercise::SleeperStretch => Writable {
                added: Some("4f870422-92aa-4fb9-8ee5-12352c1dfe50"),
                assisted: None,
            }, // Sleeper Stretch
            RepsExercise::Snatch => Writable {
                added: Some("FB09C938"),
                assisted: None,
            }, // Snatch
            RepsExercise::SnatchBalance => Writable {
                added: Some("5ca69118-ce96-4cc1-a6b6-e1554698b6a6"),
                assisted: None,
            }, // Snatch Balance
            RepsExercise::SnatchGripBehindTheNeckPress => Writable {
                added: Some("66786745-7825-45df-bdc9-25430cdaf820"),
                assisted: None,
            }, // Snatch-Grip Behind The Neck Press
            RepsExercise::SplitSquatDumbbell => Writable {
                added: Some("20C1A3CB"),
                assisted: None,
            }, // Split Squat (Dumbbell)
            RepsExercise::SquatBarbell => Writable {
                added: Some("D04AC939"),
                assisted: None,
            }, // Squat (Barbell)
            RepsExercise::StraightArmLatPulldownCable => Writable {
                added: Some("D2387AB1"),
                assisted: None,
            }, // Straight Arm Lat Pulldown (Cable)
            RepsExercise::ThrusterBarbell => Writable {
                added: Some("90E506D5"),
                assisted: None,
            }, // Thruster (Barbell)
            RepsExercise::ThrusterKettlebell => Writable {
                added: Some("10313AFD"),
                assisted: None,
            }, // Thruster (Kettlebell)
            RepsExercise::ToeTouch => Writable {
                added: Some("75BAC5C3"),
                assisted: None,
            }, // Toe Touch
            RepsExercise::ToesToBar => Writable {
                added: Some("B94E35E1"),
                assisted: None,
            }, // Toes to Bar
            RepsExercise::TricepsExtensionBarbell => Writable {
                added: Some("2F8D3067"),
                assisted: None,
            }, // Triceps Extension (Barbell)
            RepsExercise::TricepsExtensionCable => Writable {
                added: Some("21310F5F"),
                assisted: None,
            }, // Triceps Extension (Cable)
            RepsExercise::VUp => Writable {
                added: Some("6BE68B62"),
                assisted: None,
            }, // V Up
            RepsExercise::WallClimbs => Writable {
                added: Some("fb0ab15d-b64e-4aaa-9028-cf8d28380697"),
                assisted: None,
            }, // Wall Climbs
            RepsExercise::WeightedJumpSquat => Writable {
                added: Some("c319b826-fcaa-4f35-96f6-9dcd2a735201"),
                assisted: None,
            }, // Weighted Jump Squat
            RepsExercise::WristExtensionDumbbell => Writable {
                added: Some("D8460FA6"),
                assisted: None,
            }, // Reverse Wrist Curl (Dumbbell)
            RepsExercise::WristFlexionDumbbell => Writable {
                added: Some("1006DF48"),
                assisted: None,
            }, // Seated Palms Up Wrist Curl
        },
        Exercise::Duration(exercise) => match exercise {
            DurationExercise::AirBike => Writable {
                added: Some("43573BB8"),
                assisted: None,
            }, // Air Bike
            DurationExercise::CouchStretch => Writable {
                added: Some("e2182af0-2577-4603-8e70-18273be1d48b"),
                assisted: None,
            }, // Couch Stretch
            DurationExercise::DeadHang => Writable {
                added: Some("B9380898"),
                assisted: None,
            }, // Dead Hang
            DurationExercise::HandstandHold => Writable {
                added: Some("BE3615CF"),
                assisted: None,
            }, // Handstand Hold
            DurationExercise::JumpRope => Writable {
                added: Some("040BA2E3"),
                assisted: None,
            }, // Jump Rope
            DurationExercise::NinetyNinety => Writable {
                added: Some("5c98d763-9ceb-412c-8365-18110f9d5897"),
                assisted: None,
            }, // 90/90
            DurationExercise::SledPush => Writable {
                added: Some("7757171F"),
                assisted: None,
            }, // Sled Push
            DurationExercise::SquattingGroinStretch => Writable {
                added: Some("19ec9b58-0556-4a00-acbc-628a081d0be7"),
                assisted: None,
            }, // Squatting Groin Stretch
            DurationExercise::StandingStraddleFold => Writable {
                added: Some("e459e508-356d-41be-8fac-301909a91c6c"),
                assisted: None,
            }, // Standing Straddle Fold
            DurationExercise::Stretching => Writable {
                added: Some("527DA061"),
                assisted: None,
            }, // Stretching
        },
        Exercise::Distance(exercise) => match exercise {
            DistanceExercise::FarmersWalk => Writable {
                added: Some("50C613D0"),
                assisted: None,
            }, // Farmers Walk
            DistanceExercise::Running => Writable {
                added: Some("AC1BB830"),
                assisted: None,
            }, // Running
            DistanceExercise::WalkingLungeDumbbell => Writable {
                added: Some("A733CC5B"),
                assisted: None,
            }, // Walking Lunge (Dumbbell)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{Writable, writable, write_load};
    use crate::hevy::mapping::{LoadReading, lookup};
    use domain::gym::{
        Exercise, Kg, Load, SignedKg,
        exercise::{DistanceExercise, DurationExercise, RepsExercise},
    };

    /// Every exercise in the vocabulary, whichever measure it is counted in.
    fn every_exercise() -> Vec<Exercise> {
        RepsExercise::ALL
            .iter()
            .copied()
            .map(Exercise::Reps)
            .chain(
                DurationExercise::ALL
                    .iter()
                    .copied()
                    .map(Exercise::Duration),
            )
            .chain(
                DistanceExercise::ALL
                    .iter()
                    .copied()
                    .map(Exercise::Distance),
            )
            .collect()
    }

    /// **The check that keeps the two tables honest.** Nothing in the compiler
    /// relates a template id here to the forward table, so every entry is looked
    /// up and has to come back as the exercise it was filed under, on a reading
    /// that agrees with the side of the axis it was filed on.
    ///
    /// A mistyped id fails at `lookup`; an id copied from the wrong row fails on
    /// the exercise; a template filed as assisted that is nothing of the kind
    /// fails on the reading.
    #[test]
    fn writes_what_it_reads() {
        for exercise in every_exercise() {
            let Writable { added, assisted } = writable(exercise);

            if let Some(template) = added {
                let mapped = lookup(template).expect("an added template is in the forward table");
                assert_eq!(mapped.exercise, exercise, "{exercise} added to {template}");
                assert!(
                    matches!(mapped.load, LoadReading::Absolute | LoadReading::Relative),
                    "{exercise}: {template} is filed as carrying added weight but reads as {:?}",
                    mapped.load
                );
            }

            if let Some(template) = assisted {
                let mapped =
                    lookup(template).expect("an assisted template is in the forward table");
                assert_eq!(
                    mapped.exercise, exercise,
                    "{exercise} assisted to {template}"
                );
                assert_eq!(
                    mapped.load,
                    LoadReading::RelativeNegated,
                    "{exercise}: {template} is filed as assisted but reads as {:?}",
                    mapped.load
                );
            }
        }
    }

    /// Total: the vocabulary is ours, and an exercise with nowhere to be written
    /// is a session that cannot be delivered.
    #[test]
    fn every_exercise_can_be_written_somewhere() {
        for exercise in every_exercise() {
            let Writable { added, assisted } = writable(exercise);
            assert!(
                added.is_some() || assisted.is_some(),
                "{exercise} has no template on either side of the axis"
            );
        }
    }

    /// The defect this module exists to prevent, pinned on the exercise that
    /// carries it in the next session: −7kg is 7kg of assistance on the assisted
    /// template, and never 7kg of added weight on the plain one.
    #[test]
    fn assistance_is_written_to_the_assisted_template_as_a_positive_number() {
        let assisted = write_load(
            Exercise::Reps(RepsExercise::ChestDip),
            Load::Relative(SignedKg::from_grams(-7_000)),
        )
        .expect("an assisted chest dip is writable");

        assert_eq!(assisted.template_id, "E9E4089F");
        assert_eq!(assisted.weight, Kg::from_grams(7_000));

        let added = write_load(
            Exercise::Reps(RepsExercise::ChestDip),
            Load::Relative(SignedKg::from_grams(10_000)),
        )
        .expect("a weighted chest dip is writable");

        assert_eq!(added.template_id, "6FCD7755");
        assert_eq!(added.weight, Kg::from_grams(10_000));
        assert_ne!(assisted.template_id, added.template_id);
    }

    /// Zero is the same set on either axis, so an exercise the source names only
    /// in its assisted form can still say "bodyweight" — which is what the next
    /// session prescribes.
    #[test]
    fn bodyweight_is_writable_on_an_assisted_only_exercise() {
        let written = write_load(
            Exercise::Reps(RepsExercise::NeutralGripPullUp),
            Load::BODYWEIGHT,
        )
        .expect("a bodyweight neutral-grip pull-up is writable");

        assert_eq!(written.weight, Kg::NONE);
        assert_eq!(
            lookup(written.template_id)
                .expect("the template is mapped")
                .exercise,
            Exercise::Reps(RepsExercise::NeutralGripPullUp)
        );
    }

    /// And the limitation is reported rather than coerced: the source has no
    /// weighted neutral-grip pull-up, so a weight belt is refused instead of
    /// being written as assistance.
    #[test]
    fn added_weight_refuses_where_the_source_names_only_an_assisted_form() {
        let refused = write_load(
            Exercise::Reps(RepsExercise::NeutralGripPullUp),
            Load::Relative(SignedKg::from_grams(10_000)),
        );

        assert!(
            refused.is_err(),
            "a weighted neutral-grip pull-up is not writable"
        );
    }

    /// A prescribed number of kilograms of assistance means a stack. Decision
    /// 0004 holds band and machine assistance are not comparable, so the band
    /// template is never what a prescription is written to.
    #[test]
    fn assistance_is_written_to_the_machine_rather_than_the_band() {
        let written = write_load(
            Exercise::Reps(RepsExercise::PullUp),
            Load::Relative(SignedKg::from_grams(-20_000)),
        )
        .expect("an assisted pull-up is writable");

        assert_eq!(written.template_id, "2C37EC5E");
        assert_ne!(written.template_id, "56808FD2");
    }

    /// Where two templates read back to one exercise and only one of them has
    /// somewhere to put a load, the loaded one is where a load is written.
    #[test]
    fn the_weighted_variant_is_the_one_written_to() {
        let crunch = write_load(
            Exercise::Reps(RepsExercise::Crunch),
            Load::Absolute(Kg::from_grams(10_000)),
        )
        .expect("a weighted crunch is writable");

        assert_eq!(crunch.template_id, "D928C232");
    }
}
