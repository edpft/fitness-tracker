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
//! **Laterality is not a field.** A suitcase carry is the single-arm farmer's
//! carry; naming absorbs it, and an attribute schema would not have absorbed
//! the safety bar or the Zercher case.
//!
//! **The implement is a field, and it was not always.** It carried no weight
//! while nothing consumed it, and the argument then was that naming absorbs it
//! too. That argument was false and the vocabulary shows it: seventy of the
//! keys below name no implement at all, so `front-squat` being a barbell and
//! `chest-dip` being bodyweight was nowhere written down. What made it matter
//! is that the loading increment is a property of the implement — a dumbbell
//! rack does not move in 2.5kg steps — so a prescription that progresses a
//! dumbbell by a barbell's plate lands on a weight that does not exist.
//!
//! It is declared per exercise on the same line as the key, so it cannot drift,
//! and it is a total function *over* identity rather than an axis *of* it. That
//! distinction is what keeps the objection above intact: were identity a
//! `(movement, implement)` pair, `pull-up × barbell` and `pogo × machine` would
//! be constructible. Here nothing exists that was not declared. Grouping the
//! implements of one movement — a barbell and a dumbbell preacher curl — is
//! still the relation this note has always said it was, and is not yet needed.
//! When it arrives it asserts "same movement" and nothing more: § 8 makes
//! assistance a property of a pull-up because assisted and unassisted share a
//! load axis, and a 30kg barbell curl and a 30kg dumbbell curl do not.
//!
//! **What is here is what has been needed so far, not what exists.** 128
//! exercises cover the 134 templates one source has served; a second source, or
//! programming that introduces a movement nobody has recorded yet, adds
//! members. Nothing about the vocabulary is closed, and an exercise is added
//! here before anything can map onto it.
//!
//! 135 are declared, so seven have served nothing yet. That is the sentence
//! above doing what it says rather than a gap: four movements the operator had
//! been logging under a stand-in, and three the autumn block's slots name — a
//! barbell bench press, a barbell skullcrusher and a barbell Bulgarian split
//! squat. An exercise exists here before it can be prescribed, and it is
//! prescribed before it can have been performed.
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

/// What an exercise is loaded with.
///
/// The vocabulary of equipment, not of movements. Its reason to exist is that
/// the loading increment is a fact about equipment (§ 14): a barbell moves in
/// plates, a dumbbell rack in whole kilos, and a prescription that confuses
/// them asks for a weight the gym does not have.
///
/// `Bodyweight` is a member rather than an absence. A dip and a pull-up are
/// loaded — by the lifter — and both take added or assisting load on the same
/// axis, which is § 8's rule and the reason there is no `None` here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Implement {
    Barbell,
    Dumbbell,
    Kettlebell,
    Cable,
    Machine,
    Band,
    Plate,
    Sled,
    Bodyweight,
}

impl Implement {
    /// Every member, in declaration order.
    pub const ALL: &'static [Self] = &[
        Self::Barbell,
        Self::Dumbbell,
        Self::Kettlebell,
        Self::Cable,
        Self::Machine,
        Self::Band,
        Self::Plate,
        Self::Sled,
        Self::Bodyweight,
    ];

    /// The stable key. Persisted and authored.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Barbell => "barbell",
            Self::Dumbbell => "dumbbell",
            Self::Kettlebell => "kettlebell",
            Self::Cable => "cable",
            Self::Machine => "machine",
            Self::Band => "band",
            Self::Plate => "plate",
            Self::Sled => "sled",
            Self::Bodyweight => "bodyweight",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} does not name an implement")]
pub struct UnknownImplement {
    value: String,
}

impl TryFrom<String> for Implement {
    type Error = UnknownImplement;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::ALL
            .iter()
            .find(|implement| implement.as_str() == value)
            .copied()
            .ok_or(UnknownImplement { value })
    }
}

impl fmt::Display for Implement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

crate::newtype::from_str_via_string!(Implement, UnknownImplement);

/// Declare one vocabulary.
///
/// The variant and its key are written once, on one line, so the two cannot
/// drift apart — which they would if `as_str` and `TryFrom` were two match
/// blocks of 119 arms each.
macro_rules! vocabulary {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $key:literal, $implement:ident,)+ }) => {
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

            /// What this exercise is loaded with.
            ///
            /// Total, and declared on the same line as the key so the two
            /// cannot drift. Adding an exercise without naming its implement
            /// is a compile error, which is the point: the name does not carry
            /// it — seventy of the keys here mention no implement at all.
            pub const fn implement(self) -> Implement {
                match self {
                    $(Self::$variant => Implement::$implement,)+
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
        AboveAndBelowTheKneePauseSnatch => "above-and-below-the-knee-pause-snatch", Barbell,
        BackExtensionHyperextension => "back-extension-hyperextension", Bodyweight,
        BackExtensionMachine => "back-extension-machine", Machine,
        BackSquatWithSnatchPushPress => "back-squat-with-snatch-push-press", Barbell,
        BandPullaparts => "band-pullaparts", Band,
        BandedScapulaProtraction => "banded-scapula-protraction", Band,
        BehindTheBackCurlCable => "behind-the-back-curl-cable", Cable,
        BehindTheBackWristCurlBarbell => "behind-the-back-wrist-curl-barbell", Barbell,
        BenchPressBarbell => "bench-press-barbell", Barbell,
        BentOverCableChop => "bent-over-cable-chop", Cable,
        BentOverRowBarbell => "bent-over-row-barbell", Barbell,
        BicepCurlDumbbell => "bicep-curl-dumbbell", Dumbbell,
        BirdDog => "bird-dog", Bodyweight,
        BoxJump => "box-jump", Bodyweight,
        BulgarianSplitSquatBarbell => "bulgarian-split-squat-barbell", Barbell,
        BulgarianSplitSquatDumbbell => "bulgarian-split-squat-dumbbell", Dumbbell,
        Burpee => "burpee", Bodyweight,
        BurpeeOverTheBar => "burpee-over-the-bar", Bodyweight,
        ButterflyPecDeck => "butterfly-pec-deck", Machine,
        CableTwistUpToDown => "cable-twist-up-to-down", Cable,
        ChestDip => "chest-dip", Bodyweight,
        ChestPressMachine => "chest-press-machine", Machine,
        ChestSupportedInclineRowDumbbell => "chest-supported-incline-row-dumbbell", Dumbbell,
        ChestSupportedYRaiseDumbbell => "chest-supported-y-raise-dumbbell", Dumbbell,
        ChinUp => "chin-up", Bodyweight,
        CleanAndPress => "clean-and-press", Barbell,
        Crunch => "crunch", Bodyweight,
        DeadBug => "dead-bug", Bodyweight,
        DeadliftBarbell => "deadlift-barbell", Barbell,
        DeadliftDumbbell => "deadlift-dumbbell", Dumbbell,
        DeclineCrunch => "decline-crunch", Bodyweight,
        DeficitPushups => "deficit-pushups", Bodyweight,
        DownwardDogToPlancheLean => "downward-dog-to-planche-lean", Bodyweight,
        DropSnatch => "drop-snatch", Barbell,
        DumbbellSnatch => "dumbbell-snatch", Dumbbell,
        FloorPressDumbbell => "floor-press-dumbbell", Dumbbell,
        FrontLeverRaise => "front-lever-raise", Bodyweight,
        FrontRaiseBand => "front-raise-band", Band,
        FrontSquat => "front-squat", Barbell,
        GobletSquat => "goblet-squat", Dumbbell,
        GoodMorningBarbell => "good-morning-barbell", Barbell,
        HammerCurlCable => "hammer-curl-cable", Cable,
        HammerCurlDumbbell => "hammer-curl-dumbbell", Dumbbell,
        HammerTwists => "hammer-twists", Bodyweight,
        HangHighPull => "hang-high-pull", Barbell,
        HangSnatch => "hang-snatch", Barbell,
        HangingKneeRaise => "hanging-knee-raise", Bodyweight,
        HipSnatch => "hip-snatch", Barbell,
        InclineBenchPressBarbell => "incline-bench-press-barbell", Barbell,
        InclineBenchPressDumbbell => "incline-bench-press-dumbbell", Dumbbell,
        InvertedRow => "inverted-row", Bodyweight,
        KettlebellClean => "kettlebell-clean", Kettlebell,
        KettlebellCleanAndPress => "kettlebell-clean-and-press", Kettlebell,
        KettlebellSwing => "kettlebell-swing", Kettlebell,
        LatPulldownCable => "lat-pulldown-cable", Cable,
        LatPulldownCloseGripCable => "lat-pulldown-close-grip-cable", Cable,
        LateralRaiseBand => "lateral-raise-band", Band,
        LateralRaiseCable => "lateral-raise-cable", Cable,
        LateralRaiseDumbbell => "lateral-raise-dumbbell", Dumbbell,
        LegExtensionMachine => "leg-extension-machine", Machine,
        LegPressMachine => "leg-press-machine", Machine,
        LowRowSuspension => "low-row-suspension", Bodyweight,
        LungeDumbbell => "lunge-dumbbell", Dumbbell,
        LyingLegCurlMachine => "lying-leg-curl-machine", Machine,
        MuscleSnatchIntoOverheadSquat => "muscle-snatch-into-overhead-squat", Barbell,
        NeutralGripPullUp => "neutral-grip-pull-up", Bodyweight,
        NordicHamstringsCurls => "nordic-hamstrings-curls", Bodyweight,
        OverheadPlateRaise => "overhead-plate-raise", Plate,
        OverheadPressBarbell => "overhead-press-barbell", Barbell,
        OverheadPressDumbbell => "overhead-press-dumbbell", Dumbbell,
        OverheadSquat => "overhead-squat", Barbell,
        OverheadTricepsExtensionCable => "overhead-triceps-extension-cable", Cable,
        PendlayRowBarbell => "pendlay-row-barbell", Barbell,
        PikePullThrough => "pike-pull-through", Bodyweight,
        PlankPushup => "plank-pushup", Bodyweight,
        Pogo => "pogo", Bodyweight,
        PowerClean => "power-clean", Barbell,
        PowerMuscleSnatch => "power-muscle-snatch", Barbell,
        PreacherCurlBarbell => "preacher-curl-barbell", Barbell,
        PreacherCurlDumbbell => "preacher-curl-dumbbell", Dumbbell,
        PullUp => "pull-up", Bodyweight,
        PushPress => "push-press", Barbell,
        PushUp => "push-up", Bodyweight,
        RenegadeRowDumbbell => "renegade-row-dumbbell", Dumbbell,
        RingPushups => "ring-pushups", Bodyweight,
        RingRows => "ring-rows", Bodyweight,
        RomanianDeadliftBarbell => "romanian-deadlift-barbell", Barbell,
        ScapularPullUps => "scapular-pull-ups", Bodyweight,
        SeatedCableRowVGripCable => "seated-cable-row-v-grip-cable", Cable,
        SeatedInclineCurlDumbbell => "seated-incline-curl-dumbbell", Dumbbell,
        SeatedLegCurlMachine => "seated-leg-curl-machine", Machine,
        SeatedWristExtensionBarbell => "seated-wrist-extension-barbell", Barbell,
        SerratusRock => "serratus-rock", Bodyweight,
        ShoulderInternalExternalRotation => "shoulder-internal-external-rotation", Band,
        ShrugDumbbell => "shrug-dumbbell", Dumbbell,
        SingleArmCableRow => "single-arm-cable-row", Cable,
        SingleArmLateralRaiseCable => "single-arm-lateral-raise-cable", Cable,
        SingleArmTricepExtensionDumbbell => "single-arm-tricep-extension-dumbbell", Dumbbell,
        SingleLegExtensions => "single-leg-extensions", Machine,
        SingleLegRomanianDeadliftBarbell => "single-leg-romanian-deadlift-barbell", Barbell,
        SingleLegRomanianDeadliftDumbbell => "single-leg-romanian-deadlift-dumbbell", Dumbbell,
        SissySquat => "sissy-squat", Bodyweight,
        SitUp => "sit-up", Bodyweight,
        SkullcrusherBarbell => "skullcrusher-barbell", Barbell,
        SleeperStretch => "sleeper-stretch", Bodyweight,
        Snatch => "snatch", Barbell,
        SnatchBalance => "snatch-balance", Barbell,
        SnatchGripBehindTheNeckPress => "snatch-grip-behind-the-neck-press", Barbell,
        SplitSquatDumbbell => "split-squat-dumbbell", Dumbbell,
        SquatBarbell => "squat-barbell", Barbell,
        StraightArmLatPulldownCable => "straight-arm-lat-pulldown-cable", Cable,
        ThrusterBarbell => "thruster-barbell", Barbell,
        ThrusterKettlebell => "thruster-kettlebell", Kettlebell,
        ToeTouch => "toe-touch", Bodyweight,
        ToesToBar => "toes-to-bar", Bodyweight,
        TricepsExtensionBarbell => "triceps-extension-barbell", Barbell,
        TricepsExtensionCable => "triceps-extension-cable", Cable,
        VUp => "v-up", Bodyweight,
        WallClimbs => "wall-climbs", Bodyweight,
        WeightedJumpSquat => "weighted-jump-squat", Dumbbell,
        WristExtensionDumbbell => "wrist-extension-dumbbell", Dumbbell,
        WristFlexionDumbbell => "wrist-flexion-dumbbell", Dumbbell,
    }
}

vocabulary! {
    /// Exercises counted in elapsed time.
    ///
    /// `SledPush` is here because our category beats the source's: Hevy calls it
    /// distance-and-duration, and what it holds is thirty seconds and a zero
    /// distance on every one of its nine sets.
    DurationExercise {
        AirBike => "air-bike", Machine,
        CouchStretch => "couch-stretch", Bodyweight,
        DeadHang => "dead-hang", Bodyweight,
        HandstandHold => "handstand-hold", Bodyweight,
        JumpRope => "jump-rope", Bodyweight,
        NinetyNinety => "ninety-ninety", Bodyweight,
        SledPush => "sled-push", Sled,
        SquattingGroinStretch => "squatting-groin-stretch", Bodyweight,
        StandingStraddleFold => "standing-straddle-fold", Bodyweight,
        Stretching => "stretching", Bodyweight,
    }
}

/// Whether a held position works both sides at once or one side at a time.
///
/// **Not the laterality this module refuses.** That refusal is about identity —
/// a suitcase carry is the single-arm farmer's carry, and naming absorbs it, so
/// there is no `laterality` axis making `pull-up × single-arm` constructible.
/// This is a total function *over* identity, the same standing `Implement` has:
/// nothing exists that was not declared, and no exercise gains a variant.
///
/// What makes it matter is that a hold worked one side at a time is only half
/// prescribed when it is issued once. A couch stretch is sixty seconds per leg,
/// so a session naming it once names two minutes of work — and the record has
/// said so all along: every couch stretch and every 90/90 in the corpus is two
/// sets, and every dead hang is one.
///
/// It is declared for held exercises only, and deliberately. A movement counted
/// in repetitions carries its sides inside the set — the corpus prescribes a
/// Bulgarian split squat and a single-leg Romanian deadlift in threes, exactly
/// as it does a back squat — so reading a per-side count onto them would double
/// work nobody asked to double.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Sides {
    /// Both at once. One hold is the whole of it.
    Together,
    /// One at a time, so the position is held once per side.
    Separately,
}

impl Sides {
    /// How many times the position is held to work it through.
    ///
    /// Two is anatomy rather than a parameter: a body has two sides, and the
    /// authored duration in `GenerationParameters::static_hold` is what each of
    /// them is held for.
    pub const fn holds(self) -> u32 {
        match self {
            Self::Together => 1,
            Self::Separately => 2,
        }
    }
}

impl DurationExercise {
    /// Whether this position is held on both sides at once or on each in turn.
    ///
    /// Exhaustive and hand-written rather than a column on the vocabulary
    /// macro: ten members can be read in one screen, adding an eleventh is a
    /// compile error until someone says which it is, and the question is
    /// meaningless for the hundred and twenty-two exercises counted in reps.
    pub const fn sides(self) -> Sides {
        match self {
            // A hip flexor and a hip external rotator belong to one leg, and
            // the operator has never recorded either any other way.
            Self::CouchStretch | Self::NinetyNinety => Sides::Separately,
            // Both legs, both arms, or no side to speak of. A squatting groin
            // stretch and a standing straddle fold open both hips at once.
            Self::AirBike
            | Self::DeadHang
            | Self::HandstandHold
            | Self::JumpRope
            | Self::SledPush
            | Self::SquattingGroinStretch
            | Self::StandingStraddleFold
            | Self::Stretching => Sides::Together,
        }
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
        FarmersWalk => "farmers-walk", Dumbbell,
        Running => "running", Bodyweight,
        WalkingLungeDumbbell => "walking-lunge-dumbbell", Dumbbell,
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

    /// What this exercise is loaded with, whichever vocabulary it came from.
    pub const fn implement(self) -> Implement {
        match self {
            Self::Reps(exercise) => exercise.implement(),
            Self::Duration(exercise) => exercise.implement(),
            Self::Distance(exercise) => exercise.implement(),
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
