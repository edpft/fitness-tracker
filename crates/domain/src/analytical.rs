//! Functions over the canonical layer (§ 5). Nothing here is stored.
//!
//! **Estimated one-rep maximum over body weight**, per session, for the
//! headline lifts (#153). The first function that reads two sources: the gym's
//! sets and the scale's weigh-ins.
//!
//! **A canonical session, so a visit is one row however many sources recorded
//! it.** That is what makes the report reach the whole record rather than the
//! stretch one source covers.
//!
//! **An estimate is only ever read off a set whose exercise a record named.**
//! A canonical exercise is [`Identified::Guessed`] where nothing but a watch's
//! classifier placed it, and a number derived from a guess would be an e1RM
//! filed under a lift nobody confirmed. The visit still appears — the watch
//! thinking he squatted is what the record holds (§ 37) — with no figure
//! against it.
//!
//! **The estimator is a parameter, and its name is part of the series.** A
//! figure from [`Rts`] and a figure from another formula are different series
//! (§ 6), so a caller says which it used, and the report prints it.

use std::fmt;

use jiff::civil::Date;

use crate::{
    body::BodyScanWeighIn,
    canonical::Attributed,
    gym::{CanonicalExercise, CanonicalGymSession, Identified, Load, Rir, exercise::RepsExercise},
    measure::{Kg, RepCount},
    normalised::StartedAt,
    sequence::NonEmpty,
};

/// The lifts relative strength is reported for, in the order they print.
///
/// The operator's list, 2026-09-17. Each is its own series: a dumbbell press is
/// a different exercise from a barbell press, not the same one on another
/// implement.
pub const HEADLINE_LIFTS: [RepsExercise; 9] = [
    RepsExercise::BackSquatBarbell,
    RepsExercise::FrontSquatBarbell,
    RepsExercise::DeadliftBarbell,
    RepsExercise::RomanianDeadliftBarbell,
    RepsExercise::BenchPressBarbell,
    RepsExercise::OverheadPressBarbell,
    RepsExercise::OverheadPressDumbbell,
    RepsExercise::BulgarianSplitSquatBarbell,
    RepsExercise::BulgarianSplitSquatDumbbell,
];

/// A formula that reads a one-rep maximum off a set.
///
/// **`rir` is not optional.** A set with nothing recorded never reaches an
/// estimator: the operator leaves it blank to mean "no idea", and so a caller
/// has already decided there is no estimate.
pub trait OneRepMaxEstimator {
    /// What the series is called, so two estimators' figures are never
    /// mistaken for one series.
    const NAME: &'static str;

    fn estimate(&self, load: Kg, reps: RepCount, rir: Rir) -> Option<Kg>;
}

/// The Reactive Training Systems grid, in full.
///
/// ```text
/// %1RM = 100 − 2.5 × (reps − 1) − 5 × RIR
/// ```
///
/// **Every column, not only `RIR = 0`.** Reading a recorded RIR is the point:
/// without it, the heaviest set of *n* is taken to be an *n*-rep maximum. The
/// 2026-08-18 removal of the RIR coefficient from [`crate::prescription::repmax`]
/// was about a plan *stating* RIR, which this does not do.
///
/// Counted in half-steps of reserve, each 250 basis points, the same as one
/// repetition: a range is read at its midpoint (`1-2` is 1.5) and needs no
/// fractions. `4+` gives no estimate (the operator's "no idea"), and neither
/// does a set so long the share reaches zero.
#[derive(Debug, Clone, Copy, Default)]
pub struct Rts;

/// What a repetition costs, and what half a repetition in reserve costs.
const STEP_BASIS_POINTS: u64 = 250;
const WHOLE_BASIS_POINTS: u64 = 10_000;

impl Rts {
    /// Reserve in half-steps. `None` for `4+`.
    const fn half_steps(rir: Rir) -> Option<u64> {
        match rir {
            Rir::Zero => Some(0),
            Rir::ZeroOrOne => Some(1),
            Rir::One => Some(2),
            Rir::OneOrTwo => Some(3),
            Rir::Two => Some(4),
            Rir::TwoOrThree => Some(5),
            Rir::Three => Some(6),
            Rir::FourOrMore => None,
        }
    }
}

impl OneRepMaxEstimator for Rts {
    const NAME: &'static str = "rts";

    fn estimate(&self, load: Kg, reps: RepCount, rir: Rir) -> Option<Kg> {
        let steps = u64::from(reps.as_u32())
            .checked_sub(1)?
            .checked_add(Self::half_steps(rir)?)?;
        let share = WHOLE_BASIS_POINTS.checked_sub(steps.checked_mul(STEP_BASIS_POINTS)?)?;
        if share == 0 {
            return None;
        }
        // Grams, rounded down, as the wizard's own reading is.
        let grams = load.as_grams().checked_mul(WHOLE_BASIS_POINTS)? / share;
        Some(Kg::from_grams(grams))
    }
}

/// The set an estimate was read from, so a figure is never handed over bare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Estimate {
    pub one_rep_max: Kg,
    pub load: Kg,
    pub reps: RepCount,
    pub rir: Rir,
}

impl fmt::Display for Estimate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}kg × {} @ {}", self.load, self.reps, self.rir)
    }
}

/// The highest estimate any set of `lift` gives in `session`.
///
/// **Whatever the set's load, and whether it was tagged warm-up or working**
/// (the operator: *"Highest e1RM is highest e1RM."*). A set with no RIR, a
/// failed set, a set the record states no count or no load for, and a set with
/// a relative load give no estimate; a session with no set that does gives none.
///
/// **Only the exercises a record named.** A watch's guess is not a claim that
/// this was the lift, so no figure is read off one.
pub fn session_estimate<E: OneRepMaxEstimator>(
    estimator: &E,
    session: &CanonicalGymSession,
    lift: RepsExercise,
) -> Option<Estimate> {
    session
        .exercises()
        .filter_map(|performed| match performed {
            CanonicalExercise::ForReps { identified, sets }
                if identified.copied() == Identified::Recorded(lift) =>
            {
                Some(sets)
            }
            _ => None,
        })
        .flat_map(NonEmpty::iter)
        .filter_map(|set| {
            let Some(Load::Absolute(load)) = set.load.as_ref().map(Attributed::copied) else {
                return None;
            };
            let reps = (*set.outcome.value().completed()?)?;
            let rir = set.intensity.as_ref().map(Attributed::copied)?;
            let one_rep_max = estimator.estimate(load, reps, rir)?;
            Some(Estimate {
                one_rep_max,
                load,
                reps,
                rir,
            })
        })
        // The first of equal estimates, so the set shown does not depend on
        // `max_by_key`'s tie-breaking.
        .fold(None, |best: Option<Estimate>, estimate| match best {
            Some(held) if held.one_rep_max >= estimate.one_rep_max => Some(held),
            _ => Some(estimate),
        })
}

/// A lift over a body mass, in basis points: 10 000 is 1.00×.
///
/// Dimensionless, rounded down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativeStrength(u64);

impl RelativeStrength {
    /// `None` for a zero body mass, which no scale serves.
    pub fn of(lift: Kg, body: Kg) -> Option<Self> {
        lift.as_grams()
            .checked_mul(WHOLE_BASIS_POINTS)?
            .checked_div(body.as_grams())
            .map(Self)
    }

    pub const fn as_basis_points(self) -> u64 {
        self.0
    }
}

impl fmt::Display for RelativeStrength {
    /// Two decimal places, as a multiple: `1.42×`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:02}×", self.0 / 10_000, self.0 % 10_000 / 100)
    }
}

/// A body mass, and when it was weighed.
///
/// All the report needs of a weigh-in, so reading one back does not mean
/// rebuilding the whole of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weighed {
    pub at: StartedAt,
    pub mass: Kg,
}

impl From<&BodyScanWeighIn> for Weighed {
    fn from(weigh_in: &BodyScanWeighIn) -> Self {
        Self {
            at: weigh_in.measured_at().clone(),
            mass: weigh_in.mass(),
        }
    }
}

/// One headline lift in one session, and what it comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStrength {
    pub on: Date,
    pub lift: RepsExercise,
    pub estimate: Option<Estimate>,
    /// The weigh-in the session is read against, where there was one that day.
    pub body_mass: Option<Kg>,
    pub relative: Option<RelativeStrength>,
}

/// The local day something happened on, in its own zone.
fn day_of(at: &StartedAt) -> Date {
    at.wall_clock().date()
}

/// The weigh-in a session is read against: the same local day, and of several
/// that day the one closest to the session's start. The earlier wins a tie.
///
/// **Where the session states no clock, the earliest of that day.** 61 of the
/// operator's visits are dated to a day and no finer, so there is no moment for
/// a weigh-in to be near; the earliest is a choice a rebuild repeats rather than
/// an account of which reading was closest.
fn weigh_in_for<'a>(
    session: &CanonicalGymSession,
    weigh_ins: &'a [Weighed],
) -> Option<&'a Weighed> {
    let occurred = session.occurred();
    let day = occurred.day();
    let on_the_day = weigh_ins
        .iter()
        .filter(|weigh_in| day_of(&weigh_in.at) == day);
    match occurred.instant() {
        Some(started) => on_the_day.min_by_key(|weigh_in| {
            let gap = weigh_in
                .at
                .instant()
                .as_second()
                .abs_diff(started.instant().as_second());
            (gap, weigh_in.at.instant())
        }),
        None => on_the_day.min_by_key(|weigh_in| weigh_in.at.instant()),
    }
}

/// Whether any account of the session names `lift`, a watch's guess included.
///
/// **A guess earns a row and never a figure.** The row says the record holds an
/// account of that lift that day, which is true of a watch's classification;
/// [`session_estimate`] is where the distinction bites, and it reads recorded
/// exercises only.
fn named(session: &CanonicalGymSession, lift: RepsExercise) -> bool {
    session.exercises().any(|performed| {
        matches!(
            performed,
            CanonicalExercise::ForReps { identified, .. }
                if identified.copied().exercise() == Some(lift)
        )
    })
}

/// Relative strength for every session in which a headline lift was performed.
///
/// One row per session per lift, in session order and then in
/// [`HEADLINE_LIFTS`] order. A session with no usable set, or no weigh-in that
/// day, is a row with no figure rather than no row: the gap is the finding.
pub fn relative_strength<E: OneRepMaxEstimator>(
    estimator: &E,
    sessions: &[CanonicalGymSession],
    weigh_ins: &[Weighed],
) -> Vec<SessionStrength> {
    let mut rows = Vec::new();
    for session in sessions {
        let body_mass = weigh_in_for(session, weigh_ins).map(|weigh_in| weigh_in.mass);
        for lift in HEADLINE_LIFTS {
            if !named(session, lift) {
                continue;
            }
            let estimate = session_estimate(estimator, session, lift);
            let relative = estimate
                .zip(body_mass)
                .and_then(|(estimate, body)| RelativeStrength::of(estimate.one_rep_max, body));
            rows.push(SessionStrength {
                on: session.occurred().day(),
                lift,
                estimate,
                body_mass,
                relative,
            });
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::{OneRepMaxEstimator, RelativeStrength, Rts};
    use crate::{
        gym::Rir,
        measure::{Kg, RepCount},
    };

    fn kg(grams: u64) -> Kg {
        Kg::from_grams(grams)
    }

    fn reps(count: u32) -> RepCount {
        RepCount::new(count).expect("a real count")
    }

    #[test]
    fn a_single_at_zero_in_reserve_is_the_maximum() {
        assert_eq!(
            Rts.estimate(kg(100_000), reps(1), Rir::Zero),
            Some(kg(100_000))
        );
    }

    #[test]
    fn reserve_costs_what_a_repetition_costs_twice() {
        // 5 reps @ 2 in reserve: 100 − 10 − 10 = 80%.
        assert_eq!(
            Rts.estimate(kg(80_000), reps(5), Rir::Two),
            Some(kg(100_000))
        );
        // 3 reps @ 0: 95%.
        assert_eq!(
            Rts.estimate(kg(95_000), reps(3), Rir::Zero),
            Some(kg(100_000))
        );
    }

    #[test]
    fn a_range_is_read_at_its_midpoint() {
        // 1 rep @ 1-2: 100 − 7.5 = 92.5%.
        assert_eq!(
            Rts.estimate(kg(92_500), reps(1), Rir::OneOrTwo),
            Some(kg(100_000))
        );
        // 2 reps @ 0-1: 100 − 2.5 − 2.5 = 95%.
        assert_eq!(
            Rts.estimate(kg(95_000), reps(2), Rir::ZeroOrOne),
            Some(kg(100_000))
        );
    }

    #[test]
    fn grams_round_down() {
        // 100kg @ 97.5%: 102.5641…kg.
        assert_eq!(
            Rts.estimate(kg(100_000), reps(2), Rir::Zero),
            Some(kg(102_564))
        );
    }

    #[test]
    fn four_or_more_in_reserve_gives_nothing() {
        assert_eq!(Rts.estimate(kg(60_000), reps(5), Rir::FourOrMore), None);
    }

    #[test]
    fn a_set_long_enough_to_leave_the_grid_gives_nothing() {
        assert!(Rts.estimate(kg(20_000), reps(40), Rir::Zero).is_some());
        assert_eq!(Rts.estimate(kg(20_000), reps(41), Rir::Zero), None);
        assert_eq!(Rts.estimate(kg(20_000), reps(35), Rir::Three), None);
    }

    #[test]
    fn relative_strength_is_a_multiple_rounded_down() {
        let ratio = RelativeStrength::of(kg(120_000), kg(85_700)).expect("a body mass");
        assert_eq!(ratio.as_basis_points(), 14_002);
        assert_eq!(ratio.to_string(), "1.40×");
        assert_eq!(RelativeStrength::of(kg(1), Kg::NONE), None);
    }
}
