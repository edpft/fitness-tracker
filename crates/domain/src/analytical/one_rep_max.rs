//! Estimated one-rep maximum as a trend, from one test to the next (#420).
//!
//! **Absolute strength**: each session's estimate as [`session_estimate`]
//! reads it, with no body weight in it (the operator, 2026-10-13).
//!
//! **And relative strength, drawn the same way** (#253, the operator,
//! 2026-10-27): the same estimate over the body weight or the fat-free mass
//! of the session's own weigh-in, as [`weigh_in_for`] finds it. A session
//! with no weigh-in that day has no ratio, and is a gap rather than a point;
//! the stretches are still the ones the tests mark, whether or not a test was
//! weighed.
//!
//! **The question is how well the submaximal sets predict the test they lead
//! up to** (the operator, 2026-10-14). So a stretch runs from one test to the
//! next: a mesocycle under SBS, a whole macrocycle where nothing tests in
//! between. Its line is fitted to the estimates alone and projected to the
//! next planned test; once that test is performed, the gap between the two is
//! the answer.
//!
//! **A test is a measurement, not an estimate.** It is a point with no band
//! and is never in a fit: a line forced through the tests would only join
//! them.
//!
//! **Sessions across, not days**: the first session after a test is 1. A
//! programme progresses by session, and the next test is a session the plan
//! names, so stretches stack by session whatever their dates.
//!
//! **A maximal attempt is a test** (the operator, 2026-10-14). A test records
//! no reserve, so no estimate is read off it; but a completed set beneath a
//! failed attempt at a heavier load had none left, and is read at
//! [`Rir::Zero`] by the same estimator. It is the reading the prescriber
//! anchors on.
//!
//! **The line comes with its uncertainty, and no verdict.** A band of
//! [`CONFIDENCE`] shows where the line could lie, and the projection gives the
//! range a single test could fall in. Fewer than three estimates give no
//! line, since two fix a line exactly and leave nothing to say how sure it is.

use jiff::civil::Date;

use super::{
    Estimate, OneRepMaxEstimator, RelativeStrength, Weighed, session_estimate, weigh_in_for,
};
use crate::{
    canonical::Attributed,
    gym::{
        CanonicalExercise, CanonicalGymSession, CanonicalSet, Identified, Load, Rir,
        exercise::RepsExercise,
    },
    measure::{Kg, RepCount},
    plan::{Occupies, Plan},
    prescription::GymMesocycle,
    sequence::NonEmpty,
};

/// The lifts shown, in order. Each joins once it has been the focus of a
/// macrocycle (the operator, 2026-10-14).
pub const LIFTS: [RepsExercise; 1] = [RepsExercise::FrontSquatBarbell];

/// How sure the band is that the line lies within it, and the projection
/// that the test will.
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

/// A figure a stretch is drawn through: a session's estimate, or that
/// estimate over a body mass.
pub trait Figure: Copy {
    fn on(&self) -> Date;
    fn reading(&self) -> Reading;
    /// What the line is fitted to: grams for a mass, basis points for a ratio.
    fn value(&self) -> Option<f64>;
}

impl Figure for Point {
    fn on(&self) -> Date {
        self.on
    }

    fn reading(&self) -> Reading {
        self.reading
    }

    fn value(&self) -> Option<f64> {
        grams(self.estimate.one_rep_max.as_grams())
    }
}

/// What a relative figure is read against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Against {
    BodyWeight,
    FatFreeMass,
}

impl Against {
    /// Both, in the order they are shown.
    pub const ALL: [Self; 2] = [Self::BodyWeight, Self::FatFreeMass];

    pub const fn name(self) -> &'static str {
        match self {
            Self::BodyWeight => "body weight",
            Self::FatFreeMass => "fat-free mass",
        }
    }

    /// The mass a weigh-in gives for it, where it gives one.
    pub const fn of(self, weigh_in: &Weighed) -> Option<Kg> {
        match self {
            Self::BodyWeight => Some(weigh_in.mass),
            Self::FatFreeMass => weigh_in.fat_free,
        }
    }
}

/// One session's estimate over the body mass it is read against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Relative {
    pub point: Point,
    pub body: Kg,
    pub ratio: RelativeStrength,
}

impl Figure for Relative {
    fn on(&self) -> Date {
        self.point.on
    }

    fn reading(&self) -> Reading {
        self.point.reading
    }

    fn value(&self) -> Option<f64> {
        u32::try_from(self.ratio.as_basis_points())
            .ok()
            .map(f64::from)
    }
}

/// The least-squares line through a stretch's estimates, in the figure's own
/// unit against sessions since its test.
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
    /// The line through `points`, each a session and a value; `None`
    /// with fewer than three, or with all of them in one session.
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

    /// The first session the line runs through.
    pub const fn first(&self) -> i32 {
        self.first
    }

    /// The last.
    pub const fn last(&self) -> i32 {
        self.last
    }

    /// The line's value at a session.
    pub fn at(&self, day: i32) -> f64 {
        self.slope.mul_add(f64::from(day), self.intercept)
    }

    /// How far either side of [`Self::at`] the band runs at a session.
    pub fn band(&self, day: i32) -> f64 {
        self.spread(day, 0.0)
    }

    /// How far either side of [`Self::at`] a single test at a session could
    /// fall: the band widened by the scatter of one reading.
    pub fn predicts(&self, day: i32) -> f64 {
        self.spread(day, 1.0)
    }

    fn spread(&self, day: i32, single: f64) -> f64 {
        let n = f64::from(self.sessions);
        self.margin
            * ((f64::from(day) - self.mean_day).powi(2) / self.spread_of_days + n.recip() + single)
                .sqrt()
    }

    /// The slope, a session.
    pub const fn per_session(&self) -> f64 {
        self.slope
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
        .filter_map(|session| point(estimator, session, lift))
        .collect();
    points.sort_by_key(|point| point.on);
    points
}

/// Every session's estimate of `lift` over the body mass `against` reads off
/// its own weigh-in, oldest first. A session with no weigh-in that day, or
/// one that states no such mass, has none.
pub fn relative<E: OneRepMaxEstimator>(
    estimator: &E,
    sessions: &[CanonicalGymSession],
    lift: RepsExercise,
    weigh_ins: &[Weighed],
    against: Against,
) -> Vec<Relative> {
    let mut relative: Vec<Relative> = sessions
        .iter()
        .filter_map(|session| {
            let point = point(estimator, session, lift)?;
            let body = against.of(weigh_in_for(session, weigh_ins)?)?;
            let ratio = RelativeStrength::of(point.estimate.one_rep_max, body)?;
            Some(Relative { point, body, ratio })
        })
        .collect();
    relative.sort_by_key(|relative| relative.point.on);
    relative
}

/// One session's point: [`points`]' rule.
fn point<E: OneRepMaxEstimator>(
    estimator: &E,
    session: &CanonicalGymSession,
    lift: RepsExercise,
) -> Option<Point> {
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
}

/// The days of every test among `points`, oldest first.
pub fn tests<P: Figure>(points: &[P]) -> Vec<Date> {
    let mut days: Vec<Date> = points
        .iter()
        .filter(|point| matches!(point.reading(), Reading::Tested { .. }))
        .map(Figure::on)
        .collect();
    days.sort_unstable();
    days.dedup();
    days
}

/// A maximal attempt at `lift` in `session`, and the load that failed above it.
///
/// **The heaviest completed set beneath the heaviest failed one, where it has
/// no reserve recorded**, read at none in reserve. On 2026-09-25 that is
/// 92.5 kg × 1, with 95 kg failed twice. On 2026-07-03, 90 kg × 1 @ 1 sits
/// beneath a failure at 95 kg: its reserve says it was not maximal, so a
/// lighter warm-up is not read as the test either. A set whose reserve was
/// recorded is already read by [`session_estimate`]; a set with a relative
/// load, or one the record states no count for, gives nothing, as there.
pub fn tested<E: OneRepMaxEstimator>(
    estimator: &E,
    session: &CanonicalGymSession,
    lift: RepsExercise,
) -> Option<(Estimate, Kg)> {
    let sets = sets_of(session, lift);
    let failed = sets
        .iter()
        .filter(|set| set.outcome.value().is_failed())
        .filter_map(|set| absolute(set))
        .max_by_key(|load| load.as_grams())?;
    let (load, reps, recorded) = sets
        .iter()
        .filter_map(|set| {
            let load = absolute(set).filter(|load| load.as_grams() < failed.as_grams())?;
            let reps = (*set.outcome.value().completed()?)?;
            Some((load, reps, set.intensity.is_some()))
        })
        // The first of equal loads, as `session_estimate` keeps the first.
        .fold(None, |best: Option<(Kg, RepCount, bool)>, set| match best {
            Some(held) if held.0.as_grams() >= set.0.as_grams() => Some(held),
            _ => Some(set),
        })?;
    // A reserve recorded on the set beneath the failure says it was not maximal.
    if recorded {
        return None;
    }
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

/// Every set of `lift` a record named in `session`.
fn sets_of(session: &CanonicalGymSession, lift: RepsExercise) -> Vec<&CanonicalSet<RepCount>> {
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
        .collect()
}

fn absolute(set: &CanonicalSet<RepCount>) -> Option<Kg> {
    match set.load.as_ref().map(Attributed::copied) {
        Some(Load::Absolute(load)) => Some(load),
        _ => None,
    }
}

/// The days a session recorded `lift`, oldest first, once each.
pub fn sessions_of(sessions: &[CanonicalGymSession], lift: RepsExercise) -> Vec<Date> {
    let mut days: Vec<Date> = sessions
        .iter()
        .filter(|session| !sets_of(session, lift).is_empty())
        .map(|session| session.occurred().day())
        .collect();
    days.sort_unstable();
    days.dedup();
    days
}

/// The next planned test, and how many planned sessions lead to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NextTest {
    pub on: Date,
    /// Programmed days after the day counted from, the test's own included.
    pub sessions: u32,
}

/// The first test any plan programmes after `after`, counting the programmed
/// days on the way.
///
/// **The test is the last programmed day of a week whose essential session
/// tests**: SBS week 4's second session, or a test week's heavy day.
pub fn next_test(plans: &[Plan], after: Date) -> Option<NextTest> {
    let mut mesocycles: Vec<&GymMesocycle> = plans
        .iter()
        .filter_map(Plan::gym)
        .flat_map(crate::plan::Mesocycles::mesocycles)
        .collect();
    mesocycles.sort_by_key(|mesocycle| mesocycle.span().start());
    let mut sessions = 0_u32;
    for mesocycle in mesocycles {
        let span = mesocycle.span();
        let first = after.tomorrow().ok()?.max(span.start());
        for on in first
            .series(jiff::Span::new().days(1))
            .take_while(|on| *on < span.end())
        {
            let Ok((week, _)) = mesocycle.calendar().place(on) else {
                continue;
            };
            sessions = sessions.saturating_add(1);
            if mesocycle.tests_in(week) && last_of_its_week(mesocycle, on) {
                return Some(NextTest { on, sessions });
            }
        }
    }
    None
}

/// Whether no later day of the same calendar week of `mesocycle` is programmed.
fn last_of_its_week(mesocycle: &GymMesocycle, on: Date) -> bool {
    let start = mesocycle.span().start();
    let week = |day: Date| start.until(day).map_or(0, |span| span.get_days()) / 7;
    on.series(jiff::Span::new().days(1))
        .skip(1)
        .take_while(|day| week(*day) == week(on))
        .all(|day| mesocycle.calendar().place(day).is_err())
}

/// A stretch from one test to the next.
#[derive(Debug, Clone, PartialEq)]
pub struct Stretch<P = Point> {
    /// The day of the test it starts from.
    pub from: Date,
    /// That test, where it gives a figure: a ratio needs a weigh-in that day.
    pub test: Option<P>,
    /// Each estimate after it, and which session after the test it came in:
    /// 1 is the first.
    pub estimates: Vec<(u32, P)>,
    pub line: Option<Line>,
    /// The day of the test that ended it, once performed.
    pub to: Option<Date>,
    /// That test, and its session, where it gives a figure.
    pub closed_by: Option<(u32, P)>,
    /// Where the line puts the test that ends the stretch: the one performed,
    /// or the next planned while it is still to come.
    pub projection: Option<Projection>,
}

/// The line's account of a test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    pub on: Date,
    pub session: u32,
    /// In the figure's own unit, as [`Figure::value`].
    pub value: f64,
    /// The range a single test could fall in, lowest first.
    pub low: f64,
    pub high: f64,
}

/// Every stretch, oldest first.
///
/// One from each of `tests`, through the next, over the figures and the days
/// `lift` was performed; `next` is the next planned test, counted from the
/// last of those days. A stretch is projected to the test that ended it, and
/// the one still open to `next`.
///
/// **The tests mark the stretches, not the figures**, so a test with no
/// figure of its own (a ratio's, with no weigh-in that day) still ends one
/// stretch and starts the next.
pub fn stretches<P: Figure>(
    figures: &[P],
    tests: &[Date],
    days: &[Date],
    next: Option<NextTest>,
) -> Vec<Stretch<P>> {
    let ordinal = |from: Date, to: Date| {
        u32::try_from(
            days.iter()
                .filter(|day| **day > from && **day <= to)
                .count(),
        )
        .unwrap_or(u32::MAX)
    };
    let test_on = |on: Date| {
        figures
            .iter()
            .find(|figure| figure.on() == on && matches!(figure.reading(), Reading::Tested { .. }))
            .copied()
    };
    tests
        .iter()
        .enumerate()
        .map(|(index, from)| {
            let from = *from;
            let closing = tests.get(index + 1).copied();
            let estimates: Vec<(u32, P)> = figures
                .iter()
                .filter(|figure| figure.reading() == Reading::Estimated)
                .filter(|figure| figure.on() > from)
                .filter(|figure| closing.is_none_or(|closing| figure.on() < closing))
                .map(|figure| (ordinal(from, figure.on()), *figure))
                .collect();
            let line = Line::through(
                &estimates
                    .iter()
                    .filter_map(|(session, figure)| {
                        Some((i32::try_from(*session).ok()?, figure.value()?))
                    })
                    .collect::<Vec<_>>(),
            );
            let performed = days.last().map_or(0, |last| ordinal(from, *last));
            // The test that closed the stretch, or the next one planned for
            // the stretch still open.
            let target = match (closing, next) {
                (Some(closing), _) => Some((ordinal(from, closing), closing)),
                (None, Some(next)) => Some((performed.saturating_add(next.sessions), next.on)),
                (None, None) => None,
            };
            let projection = line.zip(target).and_then(|(line, (session, on))| {
                let at = i32::try_from(session).ok()?;
                Some(Projection {
                    on,
                    session,
                    value: line.at(at),
                    low: line.at(at) - line.predicts(at),
                    high: line.at(at) + line.predicts(at),
                })
            });
            Stretch {
                from,
                test: test_on(from),
                estimates,
                line,
                to: closing,
                closed_by: closing
                    .and_then(|closing| Some((ordinal(from, closing), test_on(closing)?))),
                projection,
            }
        })
        .collect()
}

fn grams(grams: u64) -> Option<f64> {
    u32::try_from(grams).ok().map(f64::from)
}
