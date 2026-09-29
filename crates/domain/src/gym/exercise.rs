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
//! so a shared variant vocabulary makes illegal pairs constructible.
//!
//! **The grouping this note said would arrive is [`Movement`], and it does not
//! move identity.** A key is still an exercise and still keeps its
//! conventional name; the movement is declared beside it and only asserts what
//! is true. The operator, 2026-09-29, on why the two coexist: *"on the one
//! hand, RDLs and conventional deadlifts are different exercises, while, on the
//! other hand, they are both deadlift variants"*. Both, because they answer
//! different questions.
//!
//! **A [`Description`] is what a source reaches when it names a movement and
//! no exercise**, which is Garmin's bare `ROW`. It is not a second kind of
//! identity and never becomes an exercise: it describes without defining, so
//! `row (barbell)` is not `bent-over-row-barbell`.
//!
//! **Laterality is not a field.** A suitcase carry is the single-arm farmer's
//! carry; naming absorbs it, and an attribute schema would not have absorbed
//! the safety bar or the Zercher case.
//!
//! **The implement is a field, and it was not always.** It carried no weight
//! while nothing consumed it, and the argument then was that naming absorbs it
//! too. That argument was false and the vocabulary shows it: seventy of the
//! keys below name no implement at all, so `goblet-squat` being a dumbbell and
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
//! **What is here is what has been needed so far, not what exists.** 178 are
//! declared, and they began as the 128 covering the 134 templates one source
//! had served. A new source, or programming that introduces a movement nobody
//! has recorded yet, adds members. Nothing about the vocabulary is closed, and
//! an exercise is added here before anything can map onto it.
//!
//! Some have served nothing yet, which is that sentence doing what it says
//! rather than a gap: four movements the operator had been logging under a
//! stand-in, and three the autumn block's slots name — a barbell bench press, a
//! barbell skullcrusher and a barbell Bulgarian split squat. An exercise exists
//! here before it can be prescribed, and it is prescribed before it can have
//! been performed.
//!
//! Fourteen more arrived with the operator's historical spreadsheets (#274),
//! which is the second source the sentence above expected: a hip thrust, a face
//! pull, two calf raises and ten others that Hevy never served. Their names are
//! the operator's, settled with him on 2026-09-27.
//!
//! Twenty-five more arrived with Beyond The White Board (#285), the gym's own
//! log, whose names are the more accurate record of what the class did
//! (operator, 2026-09-28). Two implements arrived with them: a sandbag and a
//! medicine ball.
//!
//! **The keys stopped being Hevy's on 2026-09-29 (#305).** Until then they were
//! its template titles kebab-cased, which is why
//! `single-arm-tricep-extension-dumbbell` carried Hevy's singular "Tricep"
//! where every neighbour had "Triceps" — inherited rather than chosen, and the
//! clearest evidence the target was not ours. Seven were renamed to say what
//! they mean, and each is a migration over the six columns that hold a key:
//!
//! - `squat-barbell` and `front-squat` became `back-squat-barbell` and
//!   `front-squat-barbell`, which is what the operator writes (#93).
//! - **Every triceps extension is overhead**, so all three now say so:
//!   `triceps-extension-cable` and `single-arm-tricep-extension-dumbbell`
//!   became `single-arm-overhead-triceps-extension-cable` and
//!   `single-arm-overhead-triceps-extension-dumbbell`, and
//!   `triceps-extension-barbell` became `overhead-triceps-extension-barbell`.
//!   The first also never said it meant the single-arm movement, which was
//!   #300's third fault. What separates the family's members is laterality and
//!   implement, and the names now show only that.
//! - `single-arm-lateral-raise-cable` became `lean-away-lateral-raise-cable`.
//!   The operator: *"I've been doing Lean-Away Cable Lateral Raises"*, and the
//!   lean is what performing them one at a time buys — the resistance profile
//!   changes because you can lean away from the stack. The old name stated
//!   laterality, which a lateral raise always has.
//! - `lu-raise` became `overhead-lateral-raise`. The operator, 2026-09-29: *"Lu
//!   Raises and overhead lateral raises are the same thing, but Lu Raises are
//!   done with Plates because Lu does them with plates."* It was the only key in
//!   the vocabulary named after a person, and the plate is incidental to the
//!   movement, so it stays the implement and leaves the name.
//!
//! **A pushdown is not an extension**, and they are separate movements rather
//! than two keys in one family. The operator, 2026-09-29: *"the important thing
//! is whether the long head is lengthened or not in the stretch position."* One
//! of the triceps heads crosses the shoulder, so an overhead extension
//! lengthens it and a pushdown, performed with the elbows at the side, does
//! not. Nothing could record a pushdown until `triceps-pushdown-cable` arrived,
//! and six sets were filed as overhead extensions.
//!
//! Nine more arrived that no source could name. A `triceps-dip`, performed
//! upright, which the operator distinguishes from the angled `chest-dip` —
//! *"a dip isn't a triceps extension, it's a dip"*. And eight he named on
//! 2026-09-29 so the watch's own terms had somewhere to land: a `warm-up`,
//! which is an unspecified amount of warming up exactly as `stretching` is an
//! unspecified amount of stretching; `plank-with-oblique-crunch` and
//! `bridge-with-leg-extension`, compound bodyweight movements in their own
//! right; a `handstand-push-up`; a `jump-lunge`, whose source term says
//! "alternating" redundantly, because a lunge is performed by each leg
//! independently and alternating is a prescription rather than a different
//! movement; a `pullover-barbell`, which is the straight-arm pulldown performed
//! lying on a bench and declares that movement — the bar is irrelevant, since
//! dumbbells load it the same way; and `single-leg-deadlift-barbell` and
//! `straight-leg-deadlift-barbell`.
//!
//! A source's own words never change: Hevy's `Overhead Plate Raise`, BTWB's
//! `Lu Raise` and a spreadsheet's `pushdown` all still map, onto keys that now
//! say what they are.
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
    Sandbag,
    MedicineBall,
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
        Self::Sandbag,
        Self::MedicineBall,
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
            Self::Sandbag => "sandbag",
            Self::MedicineBall => "medicine-ball",
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

/// Why a movement could not be read back.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} does not name a movement in the vocabulary")]
pub struct UnknownMovement {
    value: String,
}

impl UnknownMovement {
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Declare an enum whose members are a key and nothing else.
///
/// What [`vocabulary`] does without the columns an exercise carries, and for
/// the same reason: the variant and its key are written once so the two cannot
/// drift.
macro_rules! keyed {
    ($(#[$meta:meta])* $name:ident, $error:ident { $($variant:ident => $key:literal,)+ }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $name {
            $($variant,)+
        }

        impl $name {
            /// Every member, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant,)+];

            /// The stable key. Persisted, so it outlives a rename.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $key,)+
                }
            }
        }

        impl TryFrom<String> for $name {
            type Error = $error;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                match value.as_str() {
                    $($key => Ok(Self::$variant),)+
                    _ => Err($error { value }),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        crate::newtype::from_str_via_string!($name, $error);
    };
}

keyed! {
    /// The movement a set was of, which is what several exercises can share.
    ///
    /// **A grouping declared beside the key, not a level of identity.** A key is
    /// an exercise and keeps its conventional name; this says which movement
    /// that exercise is one of. The operator, 2026-09-29, on why both are true
    /// at once:
    ///
    /// > on the one hand, RDLs and conventional deadlifts are different
    /// > exercises, while, on the other hand, they are both deadlift variants
    ///
    /// So `romanian-deadlift-barbell` and `deadlift-barbell` are two exercises
    /// with two keys, and they answer `deadlift` to a different question. Two
    /// keys collapse only when they name the same exercise — assisted and
    /// unassisted pull-ups — which is § 8 and has nothing to do with this.
    ///
    /// **It exists because a source can state this much and no more.** Garmin
    /// serves 285 active sets on a bare category — `ROW`, `CURL`,
    /// `TRICEPS_EXTENSION`, `CALF_RAISE`, `LEG_RAISE`, `LATERAL_RAISE` — and
    /// every member of those families names an implement or a posture, so a
    /// flat vocabulary had nothing to place them on and refused all 285. What
    /// the watch stated is the movement, and this is the thing it stated.
    ///
    /// **Total over the vocabulary, and declared rather than derived.** Every
    /// key answers, on the same line as its key and its implement, so adding an
    /// exercise without saying which movement it is is a compile error. There is
    /// no criterion generating the answers: the operator, 2026-09-29, ruled that
    /// taxonomies built on resistance profile, lengthened against shortened
    /// position, isolation or plane of movement have all been tried and always
    /// found exceptions, and that some distinctions are tradition with nothing
    /// behind them.
    ///
    /// **Thirty-eight of the seventy-eight hold one key**, which is not waste.
    /// A movement with one member still says what an under-specified source
    /// term resolves to, and the families that do the work — `squat` with
    /// twelve keys, `row` with ten, `curl` with nine — are the same ones the
    /// watch's bare categories land on.
    ///
    /// The list is the operator's, settled on 2026-09-29 by correcting a draft:
    /// a renegade row is not a row but *"a thing in itself, like clean and jerk
    /// or clean and press"*, scapular pull-ups are not pull-ups but *"just the
    /// initial scapular shrug"*, toes to bar are not leg raises being *"a more
    /// dynamic movement"*, and a clean and jerk is not a clean and press. He
    /// named `curl` rather than `bicep-curl` because it is *"the conventional
    /// name for any movement that involves isolated elbow flexion"*, which is
    /// also what settles the hammer curl.
    ///
    /// Three names here are an agent's rather than his and are open on #305:
    /// `scapular-control`, `snatch-complex` and `serratus-activation`. Four more
    /// — the stretches — borrow names from the `SlotId` vocabulary, and whether
    /// a prescription slot and a movement may share a name has not been put to
    /// him.
    Movement, UnknownMovement {
        AirBike => "air-bike",
        BackExtension => "back-extension",
        BenchPress => "bench-press",
        BirdDog => "bird-dog",
        BoxJump => "box-jump",
        BridgeWithLegExtension => "bridge-with-leg-extension",
        BroadJump => "broad-jump",
        Burpee => "burpee",
        CalfRaise => "calf-raise",
        Carry => "carry",
        Clean => "clean",
        CleanAndJerk => "clean-and-jerk",
        CleanAndPress => "clean-and-press",
        Crunch => "crunch",
        Curl => "curl",
        DeadBug => "dead-bug",
        DeadHang => "dead-hang",
        Deadlift => "deadlift",
        DevilPress => "devil-press",
        Dip => "dip",
        FacePull => "face-pull",
        ForearmRotation => "forearm-rotation",
        FrontLeverRaise => "front-lever-raise",
        FrontRaise => "front-raise",
        GoodMorning => "good-morning",
        GroinStretch => "groin-stretch",
        HamstringStretch => "hamstring-stretch",
        HandstandHold => "handstand-hold",
        HandstandPushUp => "handstand-push-up",
        HandstandShoulderTap => "handstand-shoulder-tap",
        HighPull => "high-pull",
        HipExternalRotatorStretch => "hip-external-rotator-stretch",
        HipFlexorStretch => "hip-flexor-stretch",
        HipThrust => "hip-thrust",
        JumpLunge => "jump-lunge",
        JumpRope => "jump-rope",
        JumpSquat => "jump-squat",
        KettlebellSwing => "kettlebell-swing",
        LSit => "l-sit",
        LatPulldown => "lat-pulldown",
        LateralRaise => "lateral-raise",
        LegCurl => "leg-curl",
        LegExtension => "leg-extension",
        LegPress => "leg-press",
        LegRaise => "leg-raise",
        Lunge => "lunge",
        OverheadPress => "overhead-press",
        PecFly => "pec-fly",
        PikeCompression => "pike-compression",
        PlankWithObliqueCrunch => "plank-with-oblique-crunch",
        Pogo => "pogo",
        PullUp => "pull-up",
        PushPress => "push-press",
        PushUp => "push-up",
        RearDeltRaise => "rear-delt-raise",
        RenegadeRow => "renegade-row",
        RomanianDeadlift => "romanian-deadlift",
        Row => "row",
        Running => "running",
        SandbagToShoulder => "sandbag-to-shoulder",
        ScapularControl => "scapular-control",
        SerratusActivation => "serratus-activation",
        ShoulderRotation => "shoulder-rotation",
        ShoulderStretch => "shoulder-stretch",
        Shrug => "shrug",
        SitUp => "sit-up",
        SkiErg => "ski-erg",
        SledPush => "sled-push",
        Snatch => "snatch",
        SnatchComplex => "snatch-complex",
        Squat => "squat",
        StepUp => "step-up",
        StraightArmPulldown => "straight-arm-pulldown",
        Stretching => "stretching",
        Thruster => "thruster",
        ToesToBar => "toes-to-bar",
        TricepsExtension => "triceps-extension",
        TricepsPushdown => "triceps-pushdown",
        TrunkRotation => "trunk-rotation",
        WallBall => "wall-ball",
        WallClimb => "wall-climb",
        WarmUp => "warm-up",
        WristExtension => "wrist-extension",
        WristFlexion => "wrist-flexion",
    }
}

/// Declare one vocabulary.
///
/// The variant and its key are written once, on one line, so the two cannot
/// drift apart — which they would if `as_str` and `TryFrom` were two match
/// blocks of 119 arms each.
macro_rules! vocabulary {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $key:literal, $implement:ident, $movement:ident,)+ }) => {
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

            /// Which movement this exercise is one of.
            ///
            /// Total, and declared on the same line as the key and the
            /// implement so the three cannot drift. It is a grouping over
            /// identity rather than a level of it: see [`Movement`].
            pub const fn movement(self) -> Movement {
                match self {
                    $(Self::$variant => Movement::$movement,)+
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
        AboveAndBelowTheKneePauseSnatch => "above-and-below-the-knee-pause-snatch", Barbell, Snatch,
        BackExtensionHyperextension => "back-extension-hyperextension", Bodyweight, BackExtension,
        BackExtensionMachine => "back-extension-machine", Machine, BackExtension,
        BackSquatBarbell => "back-squat-barbell", Barbell, Squat,
        BackSquatWithSnatchPushPress => "back-squat-with-snatch-push-press", Barbell, SnatchComplex,
        BandPullaparts => "band-pullaparts", Band, ScapularControl,
        BandedScapulaProtraction => "banded-scapula-protraction", Band, ScapularControl,
        BatWings => "bat-wings", Bodyweight, ScapularControl,
        BehindTheBackCurlCable => "behind-the-back-curl-cable", Cable, Curl,
        BehindTheBackWristCurlBarbell => "behind-the-back-wrist-curl-barbell", Barbell, WristFlexion,
        BenchPressBarbell => "bench-press-barbell", Barbell, BenchPress,
        BentOverCableChop => "bent-over-cable-chop", Cable, TrunkRotation,
        BentOverRowBarbell => "bent-over-row-barbell", Barbell, Row,
        BicepCurlBarbell => "bicep-curl-barbell", Barbell, Curl,
        BicepCurlDumbbell => "bicep-curl-dumbbell", Dumbbell, Curl,
        BirdDog => "bird-dog", Bodyweight, BirdDog,
        BoxJump => "box-jump", Bodyweight, BoxJump,
        BridgeWithLegExtension => "bridge-with-leg-extension", Bodyweight, BridgeWithLegExtension,
        BroadJump => "broad-jump", Bodyweight, BroadJump,
        BulgarianSplitSquatBarbell => "bulgarian-split-squat-barbell", Barbell, Squat,
        BulgarianSplitSquatDumbbell => "bulgarian-split-squat-dumbbell", Dumbbell, Squat,
        Burpee => "burpee", Bodyweight, Burpee,
        BurpeeOverTheBar => "burpee-over-the-bar", Bodyweight, Burpee,
        ButterflyPecDeck => "butterfly-pec-deck", Machine, PecFly,
        CableCrossover => "cable-crossover", Cable, PecFly,
        CableTwistUpToDown => "cable-twist-up-to-down", Cable, TrunkRotation,
        ChestDip => "chest-dip", Bodyweight, Dip,
        ChestPressMachine => "chest-press-machine", Machine, BenchPress,
        ChestSupportedInclineRowDumbbell => "chest-supported-incline-row-dumbbell", Dumbbell, Row,
        ChestSupportedYRaiseDumbbell => "chest-supported-y-raise-dumbbell", Dumbbell, RearDeltRaise,
        ChinUp => "chin-up", Bodyweight, PullUp,
        ClapPushUp => "clap-push-up", Bodyweight, PushUp,
        CleanAndPress => "clean-and-press", Barbell, CleanAndPress,
        CloseGripBenchPressBarbell => "close-grip-bench-press-barbell", Barbell, BenchPress,
        CloseGripPushUp => "close-grip-push-up", Bodyweight, PushUp,
        Crunch => "crunch", Bodyweight, Crunch,
        CrushGripCurlKettlebell => "crush-grip-curl-kettlebell", Kettlebell, Curl,
        CyclistSquat => "cyclist-squat", Bodyweight, Squat,
        DeadBug => "dead-bug", Bodyweight, DeadBug,
        DeadliftBarbell => "deadlift-barbell", Barbell, Deadlift,
        DeadliftDumbbell => "deadlift-dumbbell", Dumbbell, Deadlift,
        DeclineCrunch => "decline-crunch", Bodyweight, Crunch,
        DeficitPushups => "deficit-pushups", Bodyweight, PushUp,
        DevilPressDumbbell => "devil-press-dumbbell", Dumbbell, DevilPress,
        DownwardDogToPlancheLean => "downward-dog-to-planche-lean", Bodyweight, SerratusActivation,
        DropSnatch => "drop-snatch", Barbell, Snatch,
        DumbbellSnatch => "dumbbell-snatch", Dumbbell, Snatch,
        FacePullCable => "face-pull-cable", Cable, FacePull,
        FloorPressDumbbell => "floor-press-dumbbell", Dumbbell, BenchPress,
        FrontLeverRaise => "front-lever-raise", Bodyweight, FrontLeverRaise,
        FrontRaiseBand => "front-raise-band", Band, FrontRaise,
        FrontSquatBarbell => "front-squat-barbell", Barbell, Squat,
        FrontSquatDumbbell => "front-squat-dumbbell", Dumbbell, Squat,
        GobletSquat => "goblet-squat", Dumbbell, Squat,
        GoodMorningBarbell => "good-morning-barbell", Barbell, GoodMorning,
        HammerCurlCable => "hammer-curl-cable", Cable, Curl,
        HammerCurlDumbbell => "hammer-curl-dumbbell", Dumbbell, Curl,
        HammerTwists => "hammer-twists", Bodyweight, ForearmRotation,
        HandstandPushUp => "handstand-push-up", Bodyweight, HandstandPushUp,
        HandstandShoulderTap => "handstand-shoulder-tap", Bodyweight, HandstandShoulderTap,
        HangHighPull => "hang-high-pull", Barbell, HighPull,
        HangSnatch => "hang-snatch", Barbell, Snatch,
        HangSnatchDumbbell => "hang-snatch-dumbbell", Dumbbell, Snatch,
        HangingKneeRaise => "hanging-knee-raise", Bodyweight, LegRaise,
        HangingLSitComplex => "hanging-l-sit-complex", Bodyweight, LSit,
        HipSnatch => "hip-snatch", Barbell, Snatch,
        HipThrustBarbell => "hip-thrust-barbell", Barbell, HipThrust,
        InclineBenchPressBarbell => "incline-bench-press-barbell", Barbell, BenchPress,
        InclineBenchPressDumbbell => "incline-bench-press-dumbbell", Dumbbell, BenchPress,
        InvertedRow => "inverted-row", Bodyweight, Row,
        JumpLunge => "jump-lunge", Bodyweight, JumpLunge,
        KettlebellClean => "kettlebell-clean", Kettlebell, Clean,
        KettlebellCleanAndPress => "kettlebell-clean-and-press", Kettlebell, CleanAndPress,
        KettlebellSwing => "kettlebell-swing", Kettlebell, KettlebellSwing,
        LandmineRotation => "landmine-rotation", Barbell, TrunkRotation,
        LatPulldownCable => "lat-pulldown-cable", Cable, LatPulldown,
        LatPulldownCloseGripCable => "lat-pulldown-close-grip-cable", Cable, LatPulldown,
        LateralRaiseBand => "lateral-raise-band", Band, LateralRaise,
        LateralRaiseCable => "lateral-raise-cable", Cable, LateralRaise,
        LateralRaiseDumbbell => "lateral-raise-dumbbell", Dumbbell, LateralRaise,
        LeanAwayLateralRaiseCable => "lean-away-lateral-raise-cable", Cable, LateralRaise,
        LegExtensionMachine => "leg-extension-machine", Machine, LegExtension,
        LegPressMachine => "leg-press-machine", Machine, LegPress,
        LowRowSuspension => "low-row-suspension", Bodyweight, Row,
        LungeDumbbell => "lunge-dumbbell", Dumbbell, Lunge,
        LyingLegCurlMachine => "lying-leg-curl-machine", Machine, LegCurl,
        MuscleSnatchIntoOverheadSquat => "muscle-snatch-into-overhead-squat", Barbell, SnatchComplex,
        NeutralGripPullUp => "neutral-grip-pull-up", Bodyweight, PullUp,
        NordicHamstringsCurls => "nordic-hamstrings-curls", Bodyweight, LegCurl,
        OverheadLateralRaise => "overhead-lateral-raise", Plate, LateralRaise,
        OverheadPressBarbell => "overhead-press-barbell", Barbell, OverheadPress,
        OverheadPressDumbbell => "overhead-press-dumbbell", Dumbbell, OverheadPress,
        OverheadSquat => "overhead-squat", Barbell, Squat,
        OverheadTricepsExtensionBarbell => "overhead-triceps-extension-barbell", Barbell, TricepsExtension,
        OverheadTricepsExtensionCable => "overhead-triceps-extension-cable", Cable, TricepsExtension,
        PauseSquatBarbell => "pause-squat-barbell", Barbell, Squat,
        PendlayRowBarbell => "pendlay-row-barbell", Barbell, Row,
        PikeCompression => "pike-compression", Bodyweight, PikeCompression,
        PikePullThrough => "pike-pull-through", Bodyweight, PikeCompression,
        PlankPushup => "plank-pushup", Bodyweight, PushUp,
        PlankWithObliqueCrunch => "plank-with-oblique-crunch", Bodyweight, PlankWithObliqueCrunch,
        Pogo => "pogo", Bodyweight, Pogo,
        PowerClean => "power-clean", Barbell, Clean,
        PowerMuscleSnatch => "power-muscle-snatch", Barbell, Snatch,
        PreacherCurlBarbell => "preacher-curl-barbell", Barbell, Curl,
        PreacherCurlDumbbell => "preacher-curl-dumbbell", Dumbbell, Curl,
        PullUp => "pull-up", Bodyweight, PullUp,
        PullUpNegative => "pull-up-negative", Bodyweight, PullUp,
        PulloverBarbell => "pullover-barbell", Barbell, StraightArmPulldown,
        PushPress => "push-press", Barbell, PushPress,
        PushPressDumbbell => "push-press-dumbbell", Dumbbell, PushPress,
        PushUp => "push-up", Bodyweight, PushUp,
        RearDeltRaiseDumbbell => "rear-delt-raise-dumbbell", Dumbbell, RearDeltRaise,
        RenegadeRowDumbbell => "renegade-row-dumbbell", Dumbbell, RenegadeRow,
        ReverseLungeBarbell => "reverse-lunge-barbell", Barbell, Lunge,
        RingDip => "ring-dip", Bodyweight, Dip,
        RingPushups => "ring-pushups", Bodyweight, PushUp,
        RingRows => "ring-rows", Bodyweight, Row,
        RomanianDeadliftBarbell => "romanian-deadlift-barbell", Barbell, RomanianDeadlift,
        RowKettlebell => "row-kettlebell", Kettlebell, Row,
        SandbagGoodMorning => "sandbag-good-morning", Sandbag, GoodMorning,
        SandbagSquat => "sandbag-squat", Sandbag, Squat,
        SandbagToShoulder => "sandbag-to-shoulder", Sandbag, SandbagToShoulder,
        ScapularPullUps => "scapular-pull-ups", Bodyweight, ScapularControl,
        SeatedCableRowVGripCable => "seated-cable-row-v-grip-cable", Cable, Row,
        SeatedCalfRaiseMachine => "seated-calf-raise-machine", Machine, CalfRaise,
        SeatedInclineCurlDumbbell => "seated-incline-curl-dumbbell", Dumbbell, Curl,
        SeatedLegCurlMachine => "seated-leg-curl-machine", Machine, LegCurl,
        SeatedWristExtensionBarbell => "seated-wrist-extension-barbell", Barbell, WristExtension,
        SerratusRock => "serratus-rock", Bodyweight, SerratusActivation,
        ShoulderInternalExternalRotation => "shoulder-internal-external-rotation", Band, ShoulderRotation,
        ShrugDumbbell => "shrug-dumbbell", Dumbbell, Shrug,
        SingleArmCableRow => "single-arm-cable-row", Cable, Row,
        SingleArmCleanAndJerkKettlebell => "single-arm-clean-and-jerk-kettlebell", Kettlebell, CleanAndJerk,
        SingleArmDevilPressDumbbell => "single-arm-devil-press-dumbbell", Dumbbell, DevilPress,
        SingleArmOverheadTricepsExtensionCable => "single-arm-overhead-triceps-extension-cable", Cable, TricepsExtension,
        SingleArmOverheadTricepsExtensionDumbbell => "single-arm-overhead-triceps-extension-dumbbell", Dumbbell, TricepsExtension,
        SingleArmRowDumbbell => "single-arm-row-dumbbell", Dumbbell, Row,
        SingleLegDeadliftBarbell => "single-leg-deadlift-barbell", Barbell, Deadlift,
        SingleLegExtensions => "single-leg-extensions", Machine, LegExtension,
        SingleLegRomanianDeadliftBarbell => "single-leg-romanian-deadlift-barbell", Barbell, RomanianDeadlift,
        SingleLegRomanianDeadliftDumbbell => "single-leg-romanian-deadlift-dumbbell", Dumbbell, RomanianDeadlift,
        SissySquat => "sissy-squat", Bodyweight, Squat,
        SitUp => "sit-up", Bodyweight, SitUp,
        SkullcrusherBarbell => "skullcrusher-barbell", Barbell, TricepsExtension,
        SkullcrusherKettlebell => "skullcrusher-kettlebell", Kettlebell, TricepsExtension,
        SleeperStretch => "sleeper-stretch", Bodyweight, ShoulderStretch,
        Snatch => "snatch", Barbell, Snatch,
        SnatchBalance => "snatch-balance", Barbell, Snatch,
        SnatchGripBehindTheNeckPress => "snatch-grip-behind-the-neck-press", Barbell, OverheadPress,
        SplitSquatDumbbell => "split-squat-dumbbell", Dumbbell, Squat,
        StandingCalfRaiseDumbbell => "standing-calf-raise-dumbbell", Dumbbell, CalfRaise,
        StepUpDumbbell => "step-up-dumbbell", Dumbbell, StepUp,
        StraightArmLatPulldownCable => "straight-arm-lat-pulldown-cable", Cable, StraightArmPulldown,
        StraightLegDeadliftBarbell => "straight-leg-deadlift-barbell", Barbell, Deadlift,
        ThrusterBarbell => "thruster-barbell", Barbell, Thruster,
        ThrusterDumbbell => "thruster-dumbbell", Dumbbell, Thruster,
        ThrusterKettlebell => "thruster-kettlebell", Kettlebell, Thruster,
        ToeTouch => "toe-touch", Bodyweight, HamstringStretch,
        ToesToBar => "toes-to-bar", Bodyweight, ToesToBar,
        TricepsDip => "triceps-dip", Bodyweight, Dip,
        TricepsPushdownCable => "triceps-pushdown-cable", Cable, TricepsPushdown,
        VUp => "v-up", Bodyweight, SitUp,
        WallBall => "wall-ball", MedicineBall, WallBall,
        WallClimbs => "wall-climbs", Bodyweight, WallClimb,
        WeightedJumpSquat => "weighted-jump-squat", Dumbbell, JumpSquat,
        WristExtensionDumbbell => "wrist-extension-dumbbell", Dumbbell, WristExtension,
        WristFlexionDumbbell => "wrist-flexion-dumbbell", Dumbbell, WristFlexion,
    }
}

vocabulary! {
    /// Exercises counted in elapsed time.
    ///
    /// `SledPush` is here because our category beats the source's: Hevy calls it
    /// distance-and-duration, and what it holds is thirty seconds and a zero
    /// distance on every one of its nine sets.
    DurationExercise {
        AirBike => "air-bike", Machine, AirBike,
        CouchStretch => "couch-stretch", Bodyweight, HipFlexorStretch,
        DeadHang => "dead-hang", Bodyweight, DeadHang,
        HandstandHold => "handstand-hold", Bodyweight, HandstandHold,
        JumpRope => "jump-rope", Bodyweight, JumpRope,
        NinetyNinety => "ninety-ninety", Bodyweight, HipExternalRotatorStretch,
        PigeonStretch => "pigeon-stretch", Bodyweight, HipExternalRotatorStretch,
        SkiErg => "ski-erg", Machine, SkiErg,
        SledPush => "sled-push", Sled, SledPush,
        SquattingGroinStretch => "squatting-groin-stretch", Bodyweight, GroinStretch,
        StandingStraddleFold => "standing-straddle-fold", Bodyweight, GroinStretch,
        Stretching => "stretching", Bodyweight, Stretching,
        WarmUp => "warm-up", Bodyweight, WarmUp,
        SuitcaseHold => "suitcase-hold", Dumbbell, Carry,
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
    /// macro: thirteen members can be read in one screen, adding a fourteenth is
    /// a compile error until someone says which it is, and the question is
    /// meaningless for the exercises counted in reps.
    pub const fn sides(self) -> Sides {
        match self {
            // A hip flexor and a hip external rotator belong to one leg, and
            // the operator has never recorded either any other way. The pigeon
            // is a second external rotator stretch and is held the same way.
            // A suitcase hold has one hand on the weight, so each side is held
            // in turn too.
            Self::CouchStretch | Self::NinetyNinety | Self::PigeonStretch | Self::SuitcaseHold => {
                Sides::Separately
            }
            // Both legs, both arms, or no side to speak of. A squatting groin
            // stretch and a standing straddle fold open both hips at once.
            Self::AirBike
            | Self::SkiErg
            | Self::DeadHang
            | Self::HandstandHold
            | Self::JumpRope
            | Self::SledPush
            | Self::SquattingGroinStretch
            | Self::StandingStraddleFold
            | Self::Stretching
            // An unspecified amount of warming up, which is what the operator
            // says Garmin's `WARM_UP` is: a collection of warm-up exercises
            // nothing will recover. Not a position held per side.
            | Self::WarmUp => Sides::Together,
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
        FarmersWalk => "farmers-walk", Dumbbell, Carry,
        Running => "running", Bodyweight, Running,
        SuitcaseCarry => "suitcase-carry", Dumbbell, Carry,
        WalkingLungeDumbbell => "walking-lunge-dumbbell", Dumbbell, Lunge,
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
    /// The exercise a vocabulary key names, whichever of the three it is in.
    ///
    /// **The one place the three vocabularies are tried in turn.** A key belongs
    /// to exactly one of them, so the order is not a precedence — but having two
    /// callers each write their own sequence is how a key that stops parsing in
    /// one of them starts silently reading as `None` in the other.
    ///
    /// It was in `cli::wizard` until 2026-08-30, which put the vocabulary
    /// lookup in a transport.
    #[must_use]
    pub fn named(key: &str) -> Option<Self> {
        if let Ok(reps) = RepsExercise::try_from(key.to_owned()) {
            return Some(Self::Reps(reps));
        }
        if let Ok(duration) = DurationExercise::try_from(key.to_owned()) {
            return Some(Self::Duration(duration));
        }
        DistanceExercise::try_from(key.to_owned())
            .ok()
            .map(Self::Distance)
    }

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

    /// Which movement this exercise is one of, whichever vocabulary it came
    /// from.
    pub const fn movement(self) -> Movement {
        match self {
            Self::Reps(exercise) => exercise.movement(),
            Self::Duration(exercise) => exercise.movement(),
            Self::Distance(exercise) => exercise.movement(),
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

/// A movement, described as far as a source stated it.
///
/// **What a source reaches when it names a movement and no exercise.** Garmin's
/// `ROW` is a row; which row it was is not in the record, and the vocabulary has
/// no `row` to place it on because every member of that family names an
/// implement or a posture. Before this existed the adapter refused all 285 such
/// sets and their loads, reps and clock went with them.
///
/// **It describes without defining.** A description is never an exercise and
/// never becomes one: `row (barbell)` does not name `bent-over-row-barbell`,
/// because "bent over" is a posture the watch did not state and this layer does
/// not get to supply. What it supports is matching — a source that does know
/// the exercise can be reconciled against it at the canonical layer (#247) —
/// and reading, so a set is no longer blank.
///
/// **A facet is stated or absent, never guessed.** The operator, 2026-09-29,
/// set the order the two mechanisms compose in: recorded values first, because
/// *"a row at 15.875 kg is not a barbell row whatever the default says"*, then
/// convention where the movement has one, and unstated otherwise. Frequency is
/// not a third mechanism and he rejected it by name — `preacher-curl-barbell`
/// is his most-performed curl and *"a curl"* still does not mean a preacher
/// curl.
///
/// **The implement is the only facet so far**, because it is the only one the
/// watch's bare categories turn on. Posture and laterality are facets the same
/// shape takes when a source states them (#294); adding one is a field, not a
/// new kind of thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Description {
    movement: Movement,
    implement: Option<Implement>,
}

impl Description {
    /// A movement and nothing else, which is what a bare category states.
    #[must_use]
    pub const fn of(movement: Movement) -> Self {
        Self {
            movement,
            implement: None,
        }
    }

    /// The same movement, with the implement the record separates.
    #[must_use]
    pub const fn loaded_with(self, implement: Implement) -> Self {
        Self {
            implement: Some(implement),
            ..self
        }
    }

    pub const fn movement(self) -> Movement {
        self.movement
    }

    /// What it was loaded with, where the record says. `None` is the source
    /// stating nothing, not bodyweight — a bodyweight movement says so.
    pub const fn implement(self) -> Option<Implement> {
        self.implement
    }
}

impl fmt::Display for Description {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.implement {
            Some(implement) => write!(f, "{} ({implement})", self.movement),
            None => write!(f, "{}", self.movement),
        }
    }
}
