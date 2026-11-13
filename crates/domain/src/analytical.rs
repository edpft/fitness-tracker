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

pub mod body_weight;
pub mod composition;
pub mod confidence;
pub mod expected;
pub mod ftp_test;
pub mod heart_rate;
pub mod heart_rate_at_power;
pub mod hrv;
pub mod nutrition_phase;
pub mod one_rep_max;
pub mod per_device;
pub mod pulse_wave_velocity;
pub mod range;
pub mod response;
pub mod resting;
pub mod retest;
pub mod since_entry;
pub mod since_start;
pub mod sleep;
pub mod stress;
pub mod training_done;

use std::fmt;

use jiff::civil::Date;

use crate::{
    body::BodyScanWeighIn,
    canonical::{Attributed, Occurred},
    gym::{CanonicalExercise, CanonicalGymSession, Identified, Load, Rir, exercise::RepsExercise},
    measure::{Kg, RepCount},
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

/// The Reactive Training Systems chart (Tuchscherer), in its own values
/// (the operator, 2026-11-03, #688).
///
/// **The chart, not a formula fitted to it.** Until 2026-11-04 this was
/// `100 − 2.5 × (reps − 1) − 5 × RIR`, which a repetition in reserve cost twice
/// what a repetition did. In the chart a repetition in reserve costs exactly
/// one repetition, and a step is 3 to 4.5% near the top: 82.5 kg × 5 @ 0 and
/// 80 kg × 5 @ 1 both read 95.6 kg, where the straight line read 91.7 and
/// 94.1 (#688).
///
/// **One sequence along the diagonal.** Every cell is the share at its
/// repetitions plus its reserve, so the chart is [`CHART`] read in half-steps:
/// two to a repetition, one to each half a repetition in reserve. A range is
/// a half-step (`1-2` is 1.5), so it needs no interpolation. The values are
/// the chart as fitnessvolt.com reproduces it, 1 to 10 repetitions at RPE 10
/// to 6.5 (/rpe-training/guides/tuchscherer-chart-explained, read 2026-11-04);
/// RTS's own was not read.
///
/// **Reading a recorded RIR is the point**: without it, the heaviest set of
/// *n* is taken to be an *n*-rep maximum. `4+` gives no estimate, since it is
/// not one value but four and beyond (the operator, 2026-11-04), and neither
/// does a set past the chart's last cell.
#[derive(Debug, Clone, Copy, Default)]
pub struct Rts;

/// The chart's shares in basis points, from one repetition at none in
/// reserve, a half-step at a time.
const CHART: [u64; 26] = [
    10_000, 9_780, 9_550, 9_390, 9_220, 9_070, 8_920, 8_780, 8_630, 8_500, 8_370, 8_240, 8_110,
    7_990, 7_860, 7_740, 7_620, 7_510, 7_390, 7_230, 7_070, 6_940, 6_800, 6_670, 6_530, 6_400,
];
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
        let step = u64::from(reps.as_u32())
            .checked_sub(1)?
            .checked_mul(2)?
            .checked_add(Self::half_steps(rir)?)?;
        let share = *CHART.get(usize::try_from(step).ok()?)?;
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
/// rebuilding the whole of it. **A day where no clock was read**: a weight
/// typed into MyFitnessPal is dated to the day it was logged against, and
/// when it was typed is not when it was weighed (#242).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weighed {
    pub at: Occurred,
    pub mass: Kg,
    /// The same weigh-in's fat-free mass (#253): stated, or its mass less its
    /// fat. `None` where it gave neither.
    pub fat_free: Option<Kg>,
}

impl From<&BodyScanWeighIn> for Weighed {
    fn from(weigh_in: &BodyScanWeighIn) -> Self {
        Self {
            at: Occurred::At(weigh_in.measured_at().clone()),
            mass: weigh_in.mass(),
            fat_free: Some(weigh_in.composition().fat_free_mass),
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

/// The weigh-in a session is read against: the same local day, and of several
/// that day the one closest to the session's start. The earlier wins a tie.
///
/// **Where the session states no clock, the earliest of that day.** 61 of the
/// operator's visits are dated to a day and no finer, so there is no moment for
/// a weigh-in to be near; the earliest is a choice a rebuild repeats rather than
/// an account of which reading was closest.
///
/// **A weigh-in dated to a day comes after every one with a clock**, because
/// it cannot be nearer than one that was. No day in the record has both.
fn weigh_in_for<'a>(
    session: &CanonicalGymSession,
    weigh_ins: &'a [Weighed],
) -> Option<&'a Weighed> {
    nearest(session.occurred(), weigh_ins)
}

/// [`weigh_in_for`]'s rule, over when the session happened.
fn nearest<'a>(occurred: &Occurred, weigh_ins: &'a [Weighed]) -> Option<&'a Weighed> {
    let day = occurred.day();
    let on_the_day = weigh_ins.iter().filter(|weigh_in| weigh_in.at.day() == day);
    let clock = |weigh_in: &Weighed| weigh_in.at.instant().map(|at| at.instant().as_second());
    match occurred.instant() {
        Some(started) => on_the_day.min_by_key(|weigh_in| {
            let gap = clock(weigh_in).map(|at| at.abs_diff(started.instant().as_second()));
            (gap.is_none(), gap, clock(weigh_in))
        }),
        None => on_the_day.min_by_key(|weigh_in| (clock(weigh_in).is_none(), clock(weigh_in))),
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
    use super::{OneRepMaxEstimator, RelativeStrength, Rts, Weighed, nearest};
    use crate::{
        canonical::Occurred,
        gym::Rir,
        measure::{Kg, RepCount},
        normalised::{OperatorZone, StartedAt},
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
    fn a_repetition_in_reserve_costs_one_repetition() {
        // 5 @ 0 and 4 @ 1 are both 86.3%.
        assert_eq!(
            Rts.estimate(kg(86_300), reps(5), Rir::Zero),
            Some(kg(100_000))
        );
        assert_eq!(
            Rts.estimate(kg(86_300), reps(4), Rir::One),
            Some(kg(100_000))
        );
    }

    #[test]
    fn the_chart_not_the_straight_line() {
        // 2026-10-09: 82.5 kg × 5 @ 0 and 80 kg × 5 @ 1, both 95.6 kg (#688).
        assert_eq!(
            Rts.estimate(kg(82_500), reps(5), Rir::Zero),
            Some(kg(95_596))
        );
        assert_eq!(
            Rts.estimate(kg(80_000), reps(5), Rir::One),
            Some(kg(95_579))
        );
    }

    #[test]
    fn a_range_is_its_half_step() {
        // 1 @ 0-1 is RPE 9.5, 97.8%; 3 @ 2-3 is RPE 7.5, 85.0%.
        assert_eq!(
            Rts.estimate(kg(97_800), reps(1), Rir::ZeroOrOne),
            Some(kg(100_000))
        );
        assert_eq!(
            Rts.estimate(kg(85_000), reps(3), Rir::TwoOrThree),
            Some(kg(100_000))
        );
    }

    #[test]
    fn grams_round_down() {
        // 100 kg @ 95.5%: 104.7120…kg.
        assert_eq!(
            Rts.estimate(kg(100_000), reps(2), Rir::Zero),
            Some(kg(104_712))
        );
    }

    #[test]
    fn four_or_more_in_reserve_gives_nothing() {
        assert_eq!(Rts.estimate(kg(60_000), reps(5), Rir::FourOrMore), None);
    }

    #[test]
    fn a_set_past_the_charts_last_cell_gives_nothing() {
        // 10 @ 3 is the last cell but one; 10 @ 3.5 would be the last.
        assert!(Rts.estimate(kg(60_000), reps(10), Rir::Three).is_some());
        assert!(Rts.estimate(kg(60_000), reps(13), Rir::Zero).is_some());
        assert_eq!(Rts.estimate(kg(60_000), reps(14), Rir::Zero), None);
        assert_eq!(Rts.estimate(kg(60_000), reps(11), Rir::Three), None);
    }

    fn at(timestamp: &str) -> Occurred {
        let zone = OperatorZone::try_from("Europe/London").expect("a zone");
        Occurred::At(StartedAt::new(timestamp.parse().expect("an instant"), zone))
    }

    fn weighed(at: Occurred, grams: u64) -> Weighed {
        Weighed {
            at,
            mass: kg(grams),
            fat_free: None,
        }
    }

    #[test]
    fn a_weight_dated_to_a_day_is_read_for_a_session_that_day() {
        let day = jiff::civil::date(2018, 7, 12);
        let weigh_ins = [
            weighed(Occurred::On(jiff::civil::date(2018, 7, 11)), 72_800),
            weighed(Occurred::On(day), 72_600),
        ];
        assert_eq!(
            nearest(&at("2018-07-12T17:30:00Z"), &weigh_ins).map(|weighed| weighed.mass),
            Some(kg(72_600))
        );
        assert_eq!(
            nearest(&Occurred::On(day), &weigh_ins).map(|weighed| weighed.mass),
            Some(kg(72_600))
        );
    }

    #[test]
    fn a_weight_with_a_clock_is_nearer_than_one_dated_to_the_day() {
        let weigh_ins = [
            weighed(Occurred::On(jiff::civil::date(2026, 3, 2)), 80_000),
            weighed(at("2026-03-02T21:00:00Z"), 80_400),
        ];
        assert_eq!(
            nearest(&at("2026-03-02T07:00:00Z"), &weigh_ins).map(|weighed| weighed.mass),
            Some(kg(80_400))
        );
    }

    #[test]
    fn relative_strength_is_a_multiple_rounded_down() {
        let ratio = RelativeStrength::of(kg(120_000), kg(85_700)).expect("a body mass");
        assert_eq!(ratio.as_basis_points(), 14_002);
        assert_eq!(ratio.to_string(), "1.40×");
        assert_eq!(RelativeStrength::of(kg(1), Kg::NONE), None);
    }
}
