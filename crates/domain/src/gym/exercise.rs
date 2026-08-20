//! The exercise vocabulary. Ours, not any source's (§ 8).
//!
//! Partitioned by measure, and that is the whole of the partition: an
//! exercise's measure is fixed by which vocabulary it belongs to, so a set and
//! its exercise cannot disagree and nothing needs validating. An arbitrary
//! instance is valid by construction.
//!
//! **Identity is one level.** A set belongs to an exercise and that is the
//! whole of it. The alternative — a movement with variants, so a front squat is
//! the front variant of a squat — fails because variants are not independent of
//! movements: `Front`, `Back` and `Zercher` mean nothing applied to a pull-up,
//! so a shared variant vocabulary makes illegal pairs constructible. Grouping
//! arrives later as a relation over these, which only asserts what is true.
//!
//! Implement and laterality are not fields either. A trap bar is a deadlift
//! variant and a suitcase carry is the single-arm farmer's carry; naming
//! absorbs both, and an attribute schema would not have absorbed the safety bar
//! or the Zercher case.
//!
//! **What is here is what has been needed so far, not what exists.** 128
//! exercises cover the 134 templates one source has served; a second source, or
//! programming that introduces a movement nobody has recorded yet, adds
//! members. Nothing about the vocabulary is closed, and an exercise is added
//! here before anything can map onto it.
//!
//! The six fewer than 134 are collapses, and they are all the same collapse: a
//! variant that differs only in how the movement is loaded is not a different
//! movement. Assisted and unassisted are one exercise, weighted and unweighted
//! are one exercise, and `Overhead Squat` happens to have two template ids.
//!
//! Each variant carries a stable text key, which is what the store writes and
//! reads back. Renaming a variant without changing its key is free; changing a
//! key is a migration.

use std::fmt;

/// Why an exercise could not be read back.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} does not name an exercise in the vocabulary")]
pub struct UnknownExercise {
    value: String,
}

impl UnknownExercise {
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Declare one vocabulary.
///
/// The variant and its key are written once, on one line, so the two cannot
/// drift apart — which they would if `as_str` and `TryFrom` were two match
/// blocks of 119 arms each.
macro_rules! vocabulary {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $key:literal,)+ }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $name {
            $($variant,)+
        }

        impl $name {
            /// Every member, in declaration order. What a property test
            /// enumerates and what proves the keys are distinct.
            pub const ALL: &'static [Self] = &[$(Self::$variant,)+];

            /// The stable key. Persisted, so it outlives a rename.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $key,)+
                }
            }
        }

        impl TryFrom<String> for $name {
            type Error = UnknownExercise;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                match value.as_str() {
                    $($key => Ok(Self::$variant),)+
                    _ => Err(UnknownExercise { value }),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        crate::newtype::from_str_via_string!($name, UnknownExercise);
    };
}

vocabulary! {
    /// Exercises counted in repetitions. Most of them.
    RepsExercise {
        AboveAndBelowTheKneePauseSnatch => "above-and-below-the-knee-pause-snatch",
        BackExtensionHyperextension => "back-extension-hyperextension",
        BackExtensionMachine => "back-extension-machine",
        BackSquatWithSnatchPushPress => "back-squat-with-snatch-push-press",
        BandPullaparts => "band-pullaparts",
        BandedScapulaProtraction => "banded-scapula-protraction",
        BehindTheBackCurlCable => "behind-the-back-curl-cable",
        BehindTheBackWristCurlBarbell => "behind-the-back-wrist-curl-barbell",
        BentOverCableChop => "bent-over-cable-chop",
        BentOverRowBarbell => "bent-over-row-barbell",
        BicepCurlDumbbell => "bicep-curl-dumbbell",
        BirdDog => "bird-dog",
        BoxJump => "box-jump",
        BulgarianSplitSquat => "bulgarian-split-squat",
        Burpee => "burpee",
        BurpeeOverTheBar => "burpee-over-the-bar",
        ButterflyPecDeck => "butterfly-pec-deck",
        CableTwistUpToDown => "cable-twist-up-to-down",
        ChestDip => "chest-dip",
        ChestPressMachine => "chest-press-machine",
        ChestSupportedInclineRowDumbbell => "chest-supported-incline-row-dumbbell",
        ChestSupportedYRaiseDumbbell => "chest-supported-y-raise-dumbbell",
        ChinUp => "chin-up",
        CleanAndPress => "clean-and-press",
        Crunch => "crunch",
        DeadBug => "dead-bug",
        DeadliftBarbell => "deadlift-barbell",
        DeadliftDumbbell => "deadlift-dumbbell",
        DeclineCrunch => "decline-crunch",
        DeficitPushups => "deficit-pushups",
        DownwardDogToPlancheLean => "downward-dog-to-planche-lean",
        DropSnatch => "drop-snatch",
        DumbbellSnatch => "dumbbell-snatch",
        FloorPressDumbbell => "floor-press-dumbbell",
        FrontLeverRaise => "front-lever-raise",
        FrontRaiseBand => "front-raise-band",
        FrontSquat => "front-squat",
        GobletSquat => "goblet-squat",
        GoodMorningBarbell => "good-morning-barbell",
        HammerCurlCable => "hammer-curl-cable",
        HammerCurlDumbbell => "hammer-curl-dumbbell",
        HammerTwists => "hammer-twists",
        HangHighPull => "hang-high-pull",
        HangSnatch => "hang-snatch",
        HangingKneeRaise => "hanging-knee-raise",
        HipSnatch => "hip-snatch",
        InclineBenchPressBarbell => "incline-bench-press-barbell",
        InclineBenchPressDumbbell => "incline-bench-press-dumbbell",
        InvertedRow => "inverted-row",
        KettlebellClean => "kettlebell-clean",
        KettlebellCleanAndPress => "kettlebell-clean-and-press",
        KettlebellSwing => "kettlebell-swing",
        LatPulldownCable => "lat-pulldown-cable",
        LatPulldownCloseGripCable => "lat-pulldown-close-grip-cable",
        LateralRaiseBand => "lateral-raise-band",
        LateralRaiseCable => "lateral-raise-cable",
        LateralRaiseDumbbell => "lateral-raise-dumbbell",
        LegExtensionMachine => "leg-extension-machine",
        LegPressMachine => "leg-press-machine",
        LowRowSuspension => "low-row-suspension",
        LungeDumbbell => "lunge-dumbbell",
        LyingLegCurlMachine => "lying-leg-curl-machine",
        MuscleSnatchIntoOverheadSquat => "muscle-snatch-into-overhead-squat",
        NeutralGripPullUp => "neutral-grip-pull-up",
        NordicHamstringsCurls => "nordic-hamstrings-curls",
        OverheadPlateRaise => "overhead-plate-raise",
        OverheadPressBarbell => "overhead-press-barbell",
        OverheadPressDumbbell => "overhead-press-dumbbell",
        OverheadSquat => "overhead-squat",
        OverheadTricepsExtensionCable => "overhead-triceps-extension-cable",
        PendlayRowBarbell => "pendlay-row-barbell",
        PikePullThrough => "pike-pull-through",
        PlankPushup => "plank-pushup",
        Pogo => "pogo",
        PowerClean => "power-clean",
        PowerMuscleSnatch => "power-muscle-snatch",
        PreacherCurlBarbell => "preacher-curl-barbell",
        PreacherCurlDumbbell => "preacher-curl-dumbbell",
        PullUp => "pull-up",
        PushPress => "push-press",
        PushUp => "push-up",
        RenegadeRowDumbbell => "renegade-row-dumbbell",
        RingPushups => "ring-pushups",
        RingRows => "ring-rows",
        RomanianDeadliftBarbell => "romanian-deadlift-barbell",
        ScapularPullUps => "scapular-pull-ups",
        SeatedCableRowVGripCable => "seated-cable-row-v-grip-cable",
        SeatedInclineCurlDumbbell => "seated-incline-curl-dumbbell",
        SeatedLegCurlMachine => "seated-leg-curl-machine",
        SeatedWristExtensionBarbell => "seated-wrist-extension-barbell",
        SerratusRock => "serratus-rock",
        ShoulderInternalExternalRotation => "shoulder-internal-external-rotation",
        ShrugDumbbell => "shrug-dumbbell",
        SingleArmCableRow => "single-arm-cable-row",
        SingleArmLateralRaiseCable => "single-arm-lateral-raise-cable",
        SingleArmTricepExtensionDumbbell => "single-arm-tricep-extension-dumbbell",
        SingleLegExtensions => "single-leg-extensions",
        SingleLegRomanianDeadliftBarbell => "single-leg-romanian-deadlift-barbell",
        SingleLegRomanianDeadliftDumbbell => "single-leg-romanian-deadlift-dumbbell",
        SissySquat => "sissy-squat",
        SitUp => "sit-up",
        SleeperStretch => "sleeper-stretch",
        Snatch => "snatch",
        SnatchBalance => "snatch-balance",
        SnatchGripBehindTheNeckPress => "snatch-grip-behind-the-neck-press",
        SplitSquatDumbbell => "split-squat-dumbbell",
        SquatBarbell => "squat-barbell",
        StraightArmLatPulldownCable => "straight-arm-lat-pulldown-cable",
        ThrusterBarbell => "thruster-barbell",
        ThrusterKettlebell => "thruster-kettlebell",
        ToeTouch => "toe-touch",
        ToesToBar => "toes-to-bar",
        TricepsExtensionBarbell => "triceps-extension-barbell",
        TricepsExtensionCable => "triceps-extension-cable",
        VUp => "v-up",
        WallClimbs => "wall-climbs",
        WeightedJumpSquat => "weighted-jump-squat",
        WristExtensionDumbbell => "wrist-extension-dumbbell",
        WristFlexionDumbbell => "wrist-flexion-dumbbell",
    }
}

vocabulary! {
    /// Exercises counted in elapsed time.
    ///
    /// `SledPush` is here because our category beats the source's: Hevy calls it
    /// distance-and-duration, and what it holds is thirty seconds and a zero
    /// distance on every one of its nine sets.
    DurationExercise {
        AirBike => "air-bike",
        CouchStretch => "couch-stretch",
        DeadHang => "dead-hang",
        HandstandHold => "handstand-hold",
        JumpRope => "jump-rope",
        NinetyNinety => "ninety-ninety",
        SledPush => "sled-push",
        SquattingGroinStretch => "squatting-groin-stretch",
        StandingStraddleFold => "standing-straddle-fold",
        Stretching => "stretching",
    }
}

vocabulary! {
    /// Exercises counted in ground covered.
    ///
    /// A carry and a run are both this. `Running` was briefly its own measure,
    /// carrying the duration alongside — until the records showed every entry
    /// repeating one identical distance and time across all its sets, which is an
    /// interval target rather than anything that was measured.
    DistanceExercise {
        FarmersWalk => "farmers-walk",
        Running => "running",
        WalkingLungeDumbbell => "walking-lunge-dumbbell",
    }
}

/// One of ours, whichever vocabulary it came from.
///
/// The measure is not a field: it is which variant this is. That is what makes
/// a stored measurement type unnecessary and a disagreement between a set and
/// its exercise unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Exercise {
    Reps(RepsExercise),
    Duration(DurationExercise),
    Distance(DistanceExercise),
}

impl Exercise {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reps(exercise) => exercise.as_str(),
            Self::Duration(exercise) => exercise.as_str(),
            Self::Distance(exercise) => exercise.as_str(),
        }
    }

    /// The name of the measure this exercise is counted in. For the store and
    /// for a message; the type is the authority.
    pub const fn measure(self) -> &'static str {
        match self {
            Self::Reps(_) => "reps",
            Self::Duration(_) => "duration",
            Self::Distance(_) => "distance",
        }
    }
}

impl fmt::Display for Exercise {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
