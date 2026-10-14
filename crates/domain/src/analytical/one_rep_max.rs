//! Estimated one-rep maximum as a trend, one planned macrocycle at a time
//! (#420).
//!
//! **Absolute strength**: each session's estimate as [`session_estimate`]
//! reads it, with no body weight in it (the operator, 2026-10-13). Relative
//! strength is #253.
//!
//! **A straight line per macrocycle, fitted by least squares** (the operator,
//! 2026-10-14): *"that's literally what the macrocycle is intended to do."* A
//! macrocycle builds towards a test, so whether the estimate climbed across it
//! is one slope. Every macrocycle is drawn against the weeks since its start,
//! so as each one improves on the last their lines stack.
//!
//! **Only a planned macrocycle counts.** A school term shapes a macrocycle,
//! but that a term happened does not mean a macrocycle did; autumn 2026 is the
//! first, and it starts on its plan's first day rather than on the first day
//! of term. A session outside every plan is in no macrocycle here.
//!
//! **The line comes with its uncertainty, and no verdict.** A band of
//! [`CONFIDENCE`] either side shows where the line could lie; with few
//! sessions, or noisy ones, it is wide, and the band says so rather than a
//! label. Fewer than three sessions give no line, since two fix a line exactly
//! and leave nothing to say how sure it is.
//!
//! **A maximal attempt is a point too** (the operator, 2026-10-14). A test
//! records no reserve, so no estimate is read off it; but a completed set
//! beneath a failed attempt at a heavier load had none left, and is read at
//! [`Rir::Zero`] by the same estimator. It is the same reading the prescriber
//! anchors on, and the chart marks it as tested.
//!
//! **The line runs from the macrocycle's first session to its last**, and
//! never past either (§ 37).

use jiff::civil::Date;

use super::{Estimate, OneRepMaxEstimator, session_estimate};
use crate::{
    canonical::Attributed,
    gym::{
        CanonicalExercise, CanonicalGymSession, CanonicalSet, Identified, Load, Rir,
        exercise::RepsExercise,
    },
    measure::{Kg, RepCount},
    plan::{PlanName, PlanWindow, Span},
    sequence::NonEmpty,
};

/// The lifts shown, in order. Each joins once it has been the focus of a
/// macrocycle (the operator, 2026-10-14).
pub const LIFTS: [RepsExercise; 1] = [RepsExercise::FrontSquatBarbell];

/// How sure the band is that the line lies within it.
pub const CONFIDENCE: &str = "95%";

/// Student's t at 95% two-sided, for 1 to 30 degrees of freedom.
const T_95: [f64; 30] = [
    12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
    2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
    2.052, 2.048, 2.045, 2.042,
];

/// One session's estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub on: Date,
    pub estimate: Estimate,
    pub reading: Reading,
}

/// How a point was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// Off a set with its reserve recorded.
    Estimated,
    /// Off a maximal attempt: the set beneath a failure at `failed`.
    Tested { failed: Kg },
}

/// The least-squares line through a macrocycle's estimates, in grams against
/// days since the macrocycle started.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    first: i32,
    last: i32,
    intercept: f64,
    slope: f64,
    mean_day: f64,
    spread_of_days: f64,
    sessions: u32,
    /// Student's t times the residual standard deviation.
    margin: f64,
}

impl Line {
    /// The line through `points`, each a day and a mass in grams; `None` with
    /// fewer than three, or with all of them on one day.
    pub fn through(points: &[(i32, f64)]) -> Option<Self> {
        let count = u32::try_from(points.len())
            .ok()
            .filter(|count| *count >= 3)?;
        let n = f64::from(count);
        let mean_day = points.iter().map(|(day, _)| f64::from(*day)).sum::<f64>() / n;
        let mean_grams = points.iter().map(|(_, grams)| grams).sum::<f64>() / n;
        let spread_of_days: f64 = points
            .iter()
            .map(|(day, _)| (f64::from(*day) - mean_day).powi(2))
            .sum();
        if spread_of_days <= 0.0 {
            return None;
        }
        let slope = points
            .iter()
            .map(|(day, grams)| (f64::from(*day) - mean_day) * (grams - mean_grams))
            .sum::<f64>()
            / spread_of_days;
        let intercept = slope.mul_add(-mean_day, mean_grams);
        let residuals: f64 = points
            .iter()
            .map(|(day, grams)| (grams - slope.mul_add(f64::from(*day), intercept)).powi(2))
            .sum();
        let freedom = count - 2;
        let deviation = (residuals / f64::from(freedom)).sqrt();
        Some(Self {
            first: points.iter().map(|(day, _)| *day).min()?,
            last: points.iter().map(|(day, _)| *day).max()?,
            intercept,
            slope,
            mean_day,
            spread_of_days,
            sessions: count,
            margin: t_95(freedom) * deviation,
        })
    }

    /// The day of the first session, counted from the macrocycle's start.
    pub const fn first(&self) -> i32 {
        self.first
    }

    /// The day of the last session.
    pub const fn last(&self) -> i32 {
        self.last
    }

    /// The line's mass on a day, in grams.
    pub fn at(&self, day: i32) -> f64 {
        self.slope.mul_add(f64::from(day), self.intercept)
    }

    /// How far either side of [`Self::at`] the band runs on a day, in grams.
    pub fn band(&self, day: i32) -> f64 {
        let n = f64::from(self.sessions);
        self.margin
            * ((f64::from(day) - self.mean_day).powi(2) / self.spread_of_days + n.recip()).sqrt()
    }

    /// The slope, in grams a week.
    pub fn per_week(&self) -> f64 {
        self.slope * 7.0
    }

    /// The slope's interval, in grams a week, lowest first.
    pub fn per_week_interval(&self) -> (f64, f64) {
        let half = self.margin / self.spread_of_days.sqrt() * 7.0;
        (self.per_week() - half, self.per_week() + half)
    }
}

/// Student's t for a 95% interval. Past the table, the value at the next
/// tabulated freedom below, which errs wide.
fn t_95(freedom: u32) -> f64 {
    let index = usize::try_from(freedom).unwrap_or(usize::MAX);
    if let Some(t) = index.checked_sub(1).and_then(|index| T_95.get(index)) {
        return *t;
    }
    match freedom {
        0 => f64::INFINITY,
        31..=39 => 2.042,
        40..=59 => 2.021,
        60..=119 => 2.000,
        _ => 1.980,
    }
}

/// One planned macrocycle's estimates, and the line through them.
#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    pub name: PlanName,
    pub span: Span,
    /// Oldest first.
    pub points: Vec<Point>,
    pub line: Option<Line>,
}

/// Every session's estimate of `lift`, oldest first.
///
/// The higher of what its sets estimate and what a maximal attempt in it
/// measured, the attempt where they are equal. A session that gives neither
/// has no point.
pub fn points<E: OneRepMaxEstimator>(
    estimator: &E,
    sessions: &[CanonicalGymSession],
    lift: RepsExercise,
) -> Vec<Point> {
    let mut points: Vec<Point> = sessions
        .iter()
        .filter_map(|session| {
            let on = session.occurred().day();
            let estimated = session_estimate(estimator, session, lift).map(|estimate| Point {
                on,
                estimate,
                reading: Reading::Estimated,
            });
            let tested = tested(estimator, session, lift).map(|(estimate, failed)| Point {
                on,
                estimate,
                reading: Reading::Tested { failed },
            });
            match (estimated, tested) {
                (Some(estimated), Some(tested))
                    if estimated.estimate.one_rep_max > tested.estimate.one_rep_max =>
                {
                    Some(estimated)
                }
                (_, Some(tested)) => Some(tested),
                (estimated, None) => estimated,
            }
        })
        .collect();
    points.sort_by_key(|point| point.on);
    points
}

/// A maximal attempt at `lift` in `session`, and the load that failed above it.
///
/// **The heaviest completed set with no reserve recorded, beneath the heaviest
/// failed one**, read at none in reserve. On 2026-09-25 that is 92.5 kg × 1,
/// with 95 kg failed twice. A set whose reserve was recorded is already read by
/// [`session_estimate`]; a set with a relative load, or one the record states no
/// count for, gives nothing, as there.
pub fn tested<E: OneRepMaxEstimator>(
    estimator: &E,
    session: &CanonicalGymSession,
    lift: RepsExercise,
) -> Option<(Estimate, Kg)> {
    let sets: Vec<&CanonicalSet<RepCount>> = session
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
        .collect();
    let failed = sets
        .iter()
        .filter(|set| set.outcome.value().is_failed())
        .filter_map(|set| absolute(set))
        .max_by_key(|load| load.as_grams())?;
    let (load, reps) = sets
        .iter()
        .filter(|set| set.intensity.is_none())
        .filter_map(|set| {
            let load = absolute(set).filter(|load| load.as_grams() < failed.as_grams())?;
            let reps = (*set.outcome.value().completed()?)?;
            Some((load, reps))
        })
        // The first of equal loads, as `session_estimate` keeps the first.
        .fold(
            None,
            |best: Option<(Kg, RepCount)>, (load, reps)| match best {
                Some(held) if held.0.as_grams() >= load.as_grams() => Some(held),
                _ => Some((load, reps)),
            },
        )?;
    let one_rep_max = estimator.estimate(load, reps, Rir::Zero)?;
    Some((
        Estimate {
            one_rep_max,
            load,
            reps,
            rir: Rir::Zero,
        },
        failed,
    ))
}

fn absolute(set: &CanonicalSet<RepCount>) -> Option<Kg> {
    match set.load.as_ref().map(Attributed::copied) {
        Some(Load::Absolute(load)) => Some(load),
        _ => None,
    }
}

/// The points in each planned macrocycle, oldest macrocycle first.
///
/// **One macrocycle per plan name**: a plan re-authored is the same plan, so
/// only its latest window counts.
pub fn by_macrocycle(points: &[Point], plans: &[PlanWindow]) -> Vec<Planned> {
    let mut latest: Vec<&PlanWindow> = Vec::new();
    for plan in plans {
        latest.retain(|held| held.name() != plan.name());
        latest.push(plan);
    }
    latest.sort_by_key(|plan| plan.span().start());
    latest
        .into_iter()
        .map(|plan| {
            let span = plan.span();
            let points: Vec<Point> = points
                .iter()
                .filter(|point| span.covers(point.on))
                .copied()
                .collect();
            let line = Line::through(
                &points
                    .iter()
                    .filter_map(|point| {
                        Some((
                            span.start().until(point.on).ok()?.get_days(),
                            grams(point.estimate.one_rep_max.as_grams())?,
                        ))
                    })
                    .collect::<Vec<_>>(),
            );
            Planned {
                name: plan.name().clone(),
                span,
                points,
                line,
            }
        })
        .collect()
}

fn grams(grams: u64) -> Option<f64> {
    u32::try_from(grams).ok().map(f64::from)
}
