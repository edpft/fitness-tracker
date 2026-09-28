//! Which of our exercises a Garmin classifier term is, and how to read its
//! weight.
//!
//! Code, not data (§ 9), and here rather than in `domain` because it is keyed on
//! Garmin's own vocabulary — `SQUAT`, `BENCH_PRESS/BARBELL_BENCH_PRESS` — as
//! [`crate::hevy::mapping`] is keyed on Hevy's template ids. What `domain` owns
//! is the vocabulary this points at.
//!
//! **What it translates is a guess, and that changes what an unmapped term
//! costs.** Hevy's template is the operator saying what he did, so a template
//! this build cannot place is a defect in our vocabulary and stops the run. A
//! Garmin term is the watch's classifier proposing a movement, one per set,
//! whether or not it had any idea — and every set keeps its clock, its reps and
//! its load however the proposal lands. So a term with no member here costs the
//! set nothing: the guess becomes
//! [`domain::gym::GuessedExercise::Undetermined`] and a
//! [`domain::normalised::RefusalReason::UnguessableMovement`] names the term
//! verbatim, which makes the refusals the list of words the vocabulary would
//! need in order to hold them.
//!
//! **A term is a category and, where Garmin narrowed it, a name.** Garmin's
//! categories are groupings of movements rather than movements: `SQUAT` holds
//! `LEG_PRESS` and `ONE_LEGGED_SQUAT` as well as `BARBELL_BACK_SQUAT`, which the
//! operator flagged on 2026-09-18 — *"Its category is not our exercise; ranking
//! 'squats' by category puts a 180 kg leg press on top."* So a name is looked
//! up first — and a bare category *is* used, on his ruling of 2026-09-28:
//! *"I said you could use category when there was only category."*
//!
//! **What "use the category" can reach is decided by our vocabulary.** A bare
//! category is placed where this vocabulary holds the movement that category
//! names unqualified: `SQUAT` is the squat, `DEADLIFT` the deadlift,
//! `BENCH_PRESS` the bench press, `SHOULDER_PRESS` the overhead press,
//! `PULL_UP` the pull-up, and `PUSH_UP` and `SIT_UP` name themselves. `ROW`,
//! `CURL`, `TRICEPS_EXTENSION`, `CALF_RAISE`, `LATERAL_RAISE` and `LEG_RAISE`
//! are not placed, because there is no `row`, no `curl` and no `calf-raise`
//! here to place them on — every member names an implement or a position, and
//! picking one would be inventing an answer rather than using the category.
//! `WARM_UP` names no movement at all.
//!
//! **A placed category is the movement, not the variant, and it will be wrong
//! sometimes.** On 2019-03-07 `SQUAT` puts a back squat over three sets of ten
//! at 40 kg that `Bench day (2019-03-06)` records as front squats, and the
//! loads do not separate the two — the sheets put his front squat at 20–95 kg
//! and his back squat at 0–110. That is the cost of the ruling and not an
//! argument against it: every movement here is the watch's guess, marked as
//! one, and a source that knows better overrides it at the canonical layer
//! (#247).
//!
//! **Weighted and unweighted are one exercise**, which is `domain::gym::exercise`'s
//! own rule and the reason `WEIGHTED_SEATED_CALF_RAISE` and `SEATED_CALF_RAISE`
//! are the same member here.

use domain::gym::exercise::RepsExercise;

/// How to read the number Garmin serves as a set's weight.
///
/// **No negated variant**, which [`crate::hevy::mapping::LoadReading`] needs and
/// this does not: Garmin has no assisted movements in its vocabulary, so no
/// positive number here means assistance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadReading {
    /// External load, and none of it is a real answer.
    Absolute,
    /// A delta against a bodyweight the set does not record, on an axis where
    /// assistance is conventionally available.
    Relative,
}

/// What one classifier term resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mapped {
    pub exercise: RepsExercise,
    pub load: LoadReading,
}

const fn absolute(exercise: RepsExercise) -> Mapped {
    Mapped {
        exercise,
        load: LoadReading::Absolute,
    }
}

/// The pull-up family, which is the whole of the relative axis here as it is in
/// Hevy's table: the movements a gym routinely makes easier as well as harder.
const fn relative(exercise: RepsExercise) -> Mapped {
    Mapped {
        exercise,
        load: LoadReading::Relative,
    }
}

/// Which of our exercises Garmin's term names, if this vocabulary holds it.
///
/// `name` is Garmin's `exercises[].name`, absent where its classifier reached a
/// category and no further.
///
/// The counts in the comments are the operator's corpus as of 2026-09-28. They
/// are there to be read, not matched — a term with no sets yet is as valid an
/// entry as the busiest one.
#[must_use]
pub fn lookup(category: &str, name: Option<&str>) -> Option<Mapped> {
    if let Some(name) = name {
        return by_name(category, name);
    }
    by_category(category)
}

fn by_name(category: &str, name: &str) -> Option<Mapped> {
    let mapped = match (category, name) {
        ("BANDED_EXERCISES", "LATERAL_RAISE") => absolute(RepsExercise::LateralRaiseBand), // 3
        ("BENCH_PRESS", "BARBELL_BENCH_PRESS") => absolute(RepsExercise::BenchPressBarbell), // 23
        ("BENCH_PRESS", "CLOSE_GRIP_BARBELL_BENCH_PRESS") => {
            absolute(RepsExercise::CloseGripBenchPressBarbell) // 9
        }
        ("BENCH_PRESS", "INCLINE_BARBELL_BENCH_PRESS") => {
            absolute(RepsExercise::InclineBenchPressBarbell) // 6
        }
        ("BENCH_PRESS", "INCLINE_DUMBBELL_BENCH_PRESS") => {
            absolute(RepsExercise::InclineBenchPressDumbbell) // 12
        }
        // Weighted and unweighted are one exercise, so these are two members
        // and not four.
        ("CALF_RAISE", "SEATED_CALF_RAISE" | "WEIGHTED_SEATED_CALF_RAISE") => {
            absolute(RepsExercise::SeatedCalfRaiseMachine) // 3 + 78
        }
        ("CALF_RAISE", "STANDING_CALF_RAISE" | "WEIGHTED_STANDING_CALF_RAISE") => {
            absolute(RepsExercise::StandingCalfRaiseDumbbell) // 66 + 12
        }
        ("CRUNCH", "CRUNCH") => absolute(RepsExercise::Crunch), // 19
        ("CURL", "DUMBBELL_REVERSE_WRIST_CURL") => absolute(RepsExercise::WristExtensionDumbbell), // 3
        ("CURL", "DUMBBELL_WRIST_CURL") => absolute(RepsExercise::WristFlexionDumbbell), // 3
        ("CURL", "STANDING_ALTERNATING_DUMBBELL_CURLS") => {
            absolute(RepsExercise::BicepCurlDumbbell) // 6
        }
        ("DEADLIFT", "BARBELL_DEADLIFT") => absolute(RepsExercise::DeadliftBarbell), // 392
        ("DEADLIFT", "ROMANIAN_DEADLIFT") => absolute(RepsExercise::RomanianDeadliftBarbell), // 3
        ("FLYE", "CABLE_CROSSOVER") => absolute(RepsExercise::CableCrossover),       // 9
        ("HIP_RAISE", "BARBELL_HIP_THRUST_ON_FLOOR" | "BARBELL_HIP_THRUST_WITH_BENCH") => {
            absolute(RepsExercise::HipThrustBarbell) // 13 + 51
        }
        ("HIP_RAISE", "KETTLEBELL_SWING") => absolute(RepsExercise::KettlebellSwing), // 10
        ("HIP_STABILITY", "DEAD_BUG") => absolute(RepsExercise::DeadBug),             // 3
        ("LUNGE", "BARBELL_BULGARIAN_SPLIT_SQUAT") => {
            absolute(RepsExercise::BulgarianSplitSquatBarbell) // 23
        }
        ("LUNGE", "DUMBBELL_BULGARIAN_SPLIT_SQUAT") => {
            absolute(RepsExercise::BulgarianSplitSquatDumbbell) // 15
        }
        ("LUNGE", "WEIGHTED_LUNGE") => absolute(RepsExercise::LungeDumbbell), // 27
        ("PLYO", "BODY_WEIGHT_JUMP_SQUAT") => absolute(RepsExercise::WeightedJumpSquat), // 17
        ("PULL_UP", "CHIN_UP") => relative(RepsExercise::ChinUp),             // 4
        ("PULL_UP", "LAT_PULLDOWN") => absolute(RepsExercise::LatPulldownCable), // 69
        ("PULL_UP", "PULL_UP") => relative(RepsExercise::PullUp),             // 4
        ("PUSH_UP", "PUSH_UP") => absolute(RepsExercise::PushUp),             // 20
        ("ROW", "ALTERNATING_DUMBBELL_ROW" | "DUMBBELL_ROW") => {
            absolute(RepsExercise::SingleArmRowDumbbell) // 15 + 40
        }
        ("ROW", "BARBELL_ROW") => absolute(RepsExercise::BentOverRowBarbell), // 10
        ("ROW", "FACE_PULL") => absolute(RepsExercise::FacePullCable),        // 37
        ("ROW", "INVERTED_ROW") => absolute(RepsExercise::InvertedRow),       // 1
        ("ROW", "SINGLE_ARM_CABLE_ROW") => absolute(RepsExercise::SingleArmCableRow), // 3
        ("SHOULDER_PRESS", "DUMBBELL_SHOULDER_PRESS" | "OVERHEAD_DUMBBELL_PRESS") => {
            absolute(RepsExercise::OverheadPressDumbbell) // 3 + 6
        }
        ("SHOULDER_PRESS", "OVERHEAD_BARBELL_PRESS") => {
            absolute(RepsExercise::OverheadPressBarbell) // 59
        }
        // Four of Garmin's names for one lift, and the operator's record holds
        // no squat of any other kind in the years these were recorded.
        ("SQUAT", "BACK_SQUATS" | "BARBELL_BACK_SQUAT" | "SQUAT" | "WEIGHTED_SQUAT") => {
            absolute(RepsExercise::SquatBarbell) // 6 + 38 + 21 + 236
        }
        ("SQUAT", "BARBELL_FRONT_SQUAT") => absolute(RepsExercise::FrontSquat), // 15
        ("SQUAT", "LEG_PRESS") => absolute(RepsExercise::LegPressMachine),      // 4
        ("TRICEPS_EXTENSION", "BODY_WEIGHT_DIP") => relative(RepsExercise::ChestDip), // 15
        _ => return None,
    };
    Some(mapped)
}

/// A category Garmin reached and went no further with.
///
/// Placed where this vocabulary holds the movement the category names
/// unqualified. Everything else is deliberately absent: see the module note.
fn by_category(category: &str) -> Option<Mapped> {
    let mapped = match category {
        "BENCH_PRESS" => absolute(RepsExercise::BenchPressBarbell), // 432
        "SQUAT" => absolute(RepsExercise::SquatBarbell),            // 183
        "DEADLIFT" => absolute(RepsExercise::DeadliftBarbell),      // 70
        "PULL_UP" => relative(RepsExercise::PullUp),                // 57
        "PUSH_UP" => absolute(RepsExercise::PushUp),                // 54
        "SHOULDER_PRESS" => absolute(RepsExercise::OverheadPressBarbell), // 14
        "SIT_UP" => absolute(RepsExercise::SitUp),                  // 10
        _ => return None,
    };
    Some(mapped)
}
