//! Estimated one-rep maximum over the macrocycle, by one line through every
//! session's estimate (#420, #688).
//!
//! **Absolute strength**: each session's estimate as [`session_estimate`]
//! reads it, with no body weight in it (the operator, 2026-10-13).
//!
//! **And relative strength, drawn the same way** (#253, the operator,
//! 2026-10-27): the same estimate over the body weight or the fat-free mass
//! of the session's own weigh-in, as [`weigh_in_for`] finds it. A session
//! with no weigh-in that day has no ratio, and is a gap rather than a point.
//!
//! **One line does both of its uses** (the operator, 2026-11-04, #688):
//! whether underlying strength is rising across a macrocycle designed to raise
//! the lift, and, extended to the next planned test, what that test should
//! find. *"We're fitting a line, which can naturally be extended."* It
//! replaced a line per stretch from one test to the next, which drew the tests
//! as a different thing and dealt only in mesocycles.
//!
//! **Only estimates, every session's**, a test session's included through its
//! best set with a reserve recorded, as any other (2026-11-04). The tests are
//! a record of their own (#706), read by [`read_tests`] and
//! [`macrocycle_line`].
//!
//! **The macrocycle, by day**: the plan in force, as `range` reads it, from
//! its first day. The macrocycle across all of the lift's history waits on
//! the prescriptions from before plans were stored (#302, #303): a line through
//! sessions the programming did not focus on would join unrelated blocks.
//!
//! **The uncertainty is drawn where it is read** ([`Trend`]): a band that
//! narrows as sessions accrue, and the change along the line against its 95%
//! margin. The extension carries no range: the line's own, that far ahead,
//! overstated what the operator reads off it, and a single test's mixed the
//! scatter of an estimate into it (2026-11-04).
//!
//! **A test is a test because it is one** (the operator, 2026-11-03, #688),
//! not because of the shape of its sets: it may end in a failed attempt or at
//! 0 or 0–1 in reserve, with one attempt or several. A failed attempt in a
//! linear progression is not a test. The plan says which sessions are
//! ([`planned_tests`]); a prescription this system did not author is #704.
//!
//! **Its max is the heaviest completed set of the reps it tests**, whatever
//! reserve was recorded on it (the operator, 2026-10-30, #617), read at
//! [`Rir::Zero`] by the same estimator. It is the reading the prescriber
//! anchors on.
//!
//! **A test moves only beyond the noise of two tests** (the operator,
//! 2026-11-03, #688), against the test before ([`read_tests`]) and, over a
//! macrocycle, along a line through all of its tests ([`macrocycle_line`]).
//! A free-weight
//! squat in trained men repeats to a CV of 3.5% (Seo et al., in Grgic et al.
//! 2020, *Sports Medicine – Open*, full text read): the largest figure that
//! applies, as the FTP test took 2.9% over 1.3%. Two tests then differ by
//! chance within 1.96 × √2 × 3.5%, about 9.7% of the earlier: ± 9.0 kg at
//! 92.5 kg. The noise does not shrink with level (Ribeiro et al., same
//! review), so at an advanced level most gains are not told by the tests
//! alone; the line through the estimates tells them.

use jiff::civil::Date;

use super::{
    Estimate, OneRepMaxEstimator, RelativeStrength, Weighed, body_weight::Direction, retest::Moved,
    session_estimate, weigh_in_for,
};
use crate::{
    canonical::Attributed,
    gym::exercise::Exercise,
    gym::{
        CanonicalExercise, CanonicalGymSession, CanonicalSet, Identified, Load, Rir,
        exercise::RepsExercise,
    },
    measure::{Kg, RepCount},
    plan::{Occupies, Plan},
    planner::ESSENTIAL,
    prescription::{GymMesocycle, PrescribedWorkout},
    sequence::NonEmpty,
};

/// The lifts shown, in order. Each joins once it has been the focus of a
/// macrocycle (the operator, 2026-10-14).
pub const LIFTS: [RepsExercise; 1] = [RepsExercise::FrontSquatBarbell];

/// How far a 1RM test repeats, as a fraction: a free-weight squat's
/// test–retest coefficient of variation in trained men (Seo et al., in
/// Grgic et al. 2020).
pub const TEST_RETEST: f64 = 0.035;

/// How far either side of `earlier` grams two tests land by chance, at 95%.
pub fn noise(earlier: u32) -> f64 {
    1.96 * std::f64::consts::SQRT_2 * TEST_RETEST * f64::from(earlier)
}

/// How sure the band is that the line lies within it, and the change's
/// margin that chance would not reach it.
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
    /// Off a test: its heaviest completed set, and the heaviest attempt
    /// above it that failed, where one did.
    Tested { failed: Option<Kg> },
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

/// A least-squares line through figures, in the figure's own unit against
/// days.
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
    /// The line through `points`, each a day and a value; `None` with fewer
    /// than three, since two fix a line exactly and leave nothing to say how
    /// sure it is, or with all of them on one day.
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

    /// The first day the line runs through.
    pub const fn first(&self) -> i32 {
        self.first
    }

    /// The last.
    pub const fn last(&self) -> i32 {
        self.last
    }

    /// The line's value on a day.
    pub fn at(&self, day: i32) -> f64 {
        self.slope.mul_add(f64::from(day), self.intercept)
    }

    /// How far either side of [`Self::at`] the line could lie on a day, at
    /// [`CONFIDENCE`].
    pub fn band(&self, day: i32) -> f64 {
        let n = f64::from(self.sessions);
        self.margin
            * ((f64::from(day) - self.mean_day).powi(2) / self.spread_of_days + n.recip()).sqrt()
    }

    /// The slope, a day.
    pub const fn per_day(&self) -> f64 {
        self.slope
    }

    /// How far either side of [`Self::per_day`] the slope could lie, at
    /// [`CONFIDENCE`].
    pub fn slope_margin(&self) -> f64 {
        self.margin / self.spread_of_days.sqrt()
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
    tests: &[(Date, RepCount)],
) -> Vec<Point> {
    let mut points: Vec<Point> = sessions
        .iter()
        .filter_map(|session| point(estimator, session, lift, tests))
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
    tests: &[(Date, RepCount)],
    weigh_ins: &[Weighed],
    against: Against,
) -> Vec<Relative> {
    let mut relative: Vec<Relative> = sessions
        .iter()
        .filter_map(|session| {
            let point = point(estimator, session, lift, tests)?;
            let body = against.of(weigh_in_for(session, weigh_ins)?)?;
            let ratio = RelativeStrength::of(point.estimate.one_rep_max, body)?;
            Some(Relative { point, body, ratio })
        })
        .collect();
    relative.sort_by_key(|relative| relative.point.on);
    relative
}

/// One session's point: [`points`]' rule.
///
/// **A test's session is its test**, whatever its sets estimate: a test is a
/// measurement, and a test day's estimate is not one more.
fn point<E: OneRepMaxEstimator>(
    estimator: &E,
    session: &CanonicalGymSession,
    lift: RepsExercise,
    tests: &[(Date, RepCount)],
) -> Option<Point> {
    let on = session.occurred().day();
    match tests.iter().find(|(day, _)| *day == on) {
        Some((_, reps)) => {
            tested(estimator, session, lift, *reps).map(|(estimate, failed)| Point {
                on,
                estimate,
                reading: Reading::Tested { failed },
            })
        }
        None => session_estimate(estimator, session, lift).map(|estimate| Point {
            on,
            estimate,
            reading: Reading::Estimated,
        }),
    }
}

/// Each test among `points`, oldest first, read against the one before beyond
/// the noise of two tests, with that noise in grams, either side of the test
/// before.
///
/// **Against the test before only** (operator, 2026-11-03). Over a
/// macrocycle the tests are read by a line through all of them,
/// [`macrocycle_line`], which replaced reading a test against the first since
/// the last clear change.
///
/// **In kilograms only.** The ruling is on the test's mass (#688); a ratio
/// over a body mass carries the weigh-in's error too, and no ruling covers it.
pub fn read_tests(points: &[Point]) -> Vec<TestRead> {
    let tests: Vec<(Date, u32)> = tested_grams(points);
    let before = std::iter::once(None).chain(tests.iter().map(|(_, grams)| Some(*grams)));
    tests
        .iter()
        .zip(before)
        .map(|(&(on, now), before)| TestRead {
            on,
            moved: before.map(|earlier| {
                let by = i64::from(now) - i64::from(earlier);
                let margin = noise(earlier);
                let direction = if f64::from(now) - f64::from(earlier) > margin {
                    Some(Direction::Rising)
                } else if f64::from(earlier) - f64::from(now) > margin {
                    Some(Direction::Falling)
                } else {
                    None
                };
                Moved {
                    by,
                    direction,
                    since: None,
                }
            }),
            noise: before.map(noise),
        })
        .collect()
}

/// Each test among `points`, oldest first, in grams.
fn tested_grams(points: &[Point]) -> Vec<(Date, u32)> {
    points
        .iter()
        .filter(|point| matches!(point.reading, Reading::Tested { .. }))
        .filter_map(|point| {
            Some((
                point.on,
                u32::try_from(point.estimate.one_rep_max.as_grams()).ok()?,
            ))
        })
        .collect()
}

/// The tests of a macrocycle, read by a line through all of them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TestLine {
    pub from: Date,
    pub to: Date,
    pub tests: usize,
    /// The change along the line from the first test to the last, in grams.
    pub change: f64,
    /// How far either side of `change` chance could put it, at 95%, in grams.
    pub margin: f64,
    /// Rising or falling where `change` is beyond `margin`; `None` is no
    /// clear change.
    pub direction: Option<Direction>,
}

/// The macrocycle holding the latest test, read by a least-squares line
/// through every test in it, by day (operator, 2026-11-03).
///
/// **A line through all of them**, because it catches a real steady gain more
/// often than first against last, or than requiring every test to rise, at
/// the same 1 in 40 chance of a false reading (#688). Every change need not
/// be above zero. Each test's noise is [`TEST_RETEST`] of the tests' mean, so
/// two tests give the noise of two tests: 1.96 × √2 × 3.5% of their mean.
///
/// `None` with fewer than two tests in it, or where no plan holds the latest.
pub fn macrocycle_line(points: &[Point], plans: &[Plan]) -> Option<TestLine> {
    let last = tested_grams(points).last()?.0;
    let span = plans
        .iter()
        .map(Plan::span)
        .find(|span| span.start() <= last && last < span.end())?;
    line_through(points, span)
}

/// The tests among `points` within `span`, read by a least-squares line
/// through them by day: [`macrocycle_line`]'s reading over one span.
pub fn line_through(points: &[Point], span: crate::plan::Span) -> Option<TestLine> {
    let tests = tested_grams(points);
    let held: Vec<(f64, f64)> = tests
        .iter()
        .filter(|(on, _)| *on >= span.start() && *on < span.end())
        .map(|(on, grams)| {
            let days = span.start().until(*on).map_or(0, |gap| gap.get_days());
            (f64::from(days), f64::from(*grams))
        })
        .collect();
    let within: Vec<Date> = tests
        .iter()
        .map(|(on, _)| *on)
        .filter(|on| *on >= span.start() && *on < span.end())
        .collect();
    let (first_on, last) = (*within.first()?, *within.last()?);
    let first_x = held.first().map(|(x, _)| *x)?;
    if held.len() < 2 {
        return None;
    }
    let count = f64::from(u32::try_from(held.len()).ok()?);
    let mean_x = held.iter().map(|(x, _)| x).sum::<f64>() / count;
    let mean_y = held.iter().map(|(_, y)| y).sum::<f64>() / count;
    let spread: f64 = held.iter().map(|(x, _)| (x - mean_x).powi(2)).sum();
    if spread <= 0.0 {
        return None;
    }
    let slope = held
        .iter()
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum::<f64>()
        / spread;
    let last_x = held.last().map_or(first_x, |(x, _)| *x);
    let along = last_x - first_x;
    let change = slope * along;
    let margin = 1.96 * TEST_RETEST * mean_y * along / spread.sqrt();
    let direction = if change > margin {
        Some(Direction::Rising)
    } else if change < -margin {
        Some(Direction::Falling)
    } else {
        None
    };
    Some(TestLine {
        from: first_on,
        to: last,
        tests: held.len(),
        change,
        margin,
        direction,
    })
}

/// One test, read against its noise.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TestRead {
    pub on: Date,
    /// `None` for the first test, which has nothing to be read against.
    pub moved: Option<Moved>,
    /// How far either side of the test before two tests land by chance, in
    /// grams.
    pub noise: Option<f64>,
}

/// The sessions prescribed as tests of `lift`, and the reps each tests:
/// `prescribed` holds the prescription in force for each day it was
/// performed.
///
/// **A session is a test where its prescription is the essential session of a
/// week its mesocycle tests in**: SBS week 4's second session, a test week's
/// heavy day, a block's entry or exit test. The prescription says which week
/// it was, rather than the calendar: on 2026-09-25 the entry test was done
/// after its calendar week, and its prescription still names it.
///
/// **The mesocycle is the latest of `lift`'s to have started by the day**, so
/// a test done late, before the next mesocycle opens, is still its own.
pub fn planned_tests(
    plans: &[Plan],
    prescribed: &[PrescribedWorkout],
    lift: RepsExercise,
) -> Vec<(Date, RepCount)> {
    let mesocycles: Vec<&GymMesocycle> = plans
        .iter()
        .filter_map(Plan::gym)
        .flat_map(crate::plan::Mesocycles::mesocycles)
        .filter(|mesocycle| mesocycle.primary_exercise() == Exercise::Reps(lift))
        .collect();
    let mut tests: Vec<(Date, RepCount)> = prescribed
        .iter()
        .filter(|prescription| prescription.session_role() == ESSENTIAL)
        .filter_map(|prescription| {
            let on = prescription.issued_for();
            let mesocycle = mesocycles
                .iter()
                .filter(|mesocycle| mesocycle.span().start() <= on)
                .max_by_key(|mesocycle| mesocycle.span().start())?;
            let reps = mesocycle.tested_reps(prescription.week(), on)?;
            Some((on, reps))
        })
        .collect();
    tests.sort_by_key(|(on, _)| *on);
    tests.dedup_by_key(|(on, _)| *on);
    tests
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

/// A test of `lift` for `reps` in `session`: its max, and the heaviest
/// attempt that failed above it, where one did.
///
/// **The heaviest completed set of at least `reps`, whatever reserve was
/// recorded on it**, read at none in reserve (the operator, 2026-10-30,
/// #617). In a 1RM test of 95 kg × 1 @ 1 and 97.5 kg failed, it is 95 kg; in
/// a 3RM test of 3 × 92.5 kg then 2 × 95 kg, it is 92.5 kg. A set with a
/// relative load, or one the record states no count for, gives nothing, as
/// in [`session_estimate`].
pub fn tested<E: OneRepMaxEstimator>(
    estimator: &E,
    session: &CanonicalGymSession,
    lift: RepsExercise,
    reps: RepCount,
) -> Option<(Estimate, Option<Kg>)> {
    let sets = sets_of(session, lift);
    let load = sets
        .iter()
        .filter_map(|set| {
            let load = absolute(set)?;
            let done = (*set.outcome.value().completed()?)?;
            (done >= reps).then_some(load)
        })
        .max_by_key(|load| load.as_grams())?;
    let failed = sets
        .iter()
        .filter(|set| set.outcome.value().is_failed())
        .filter_map(|set| absolute(set))
        .filter(|failed| failed.as_grams() > load.as_grams())
        .max_by_key(|failed| failed.as_grams());
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

/// The first test any plan programmes after `after`.
///
/// **The test is the last programmed day of a week whose essential session
/// tests**: SBS week 4's second session, or a test week's heavy day.
pub fn next_test(plans: &[Plan], after: Date) -> Option<Date> {
    let mut mesocycles: Vec<&GymMesocycle> = plans
        .iter()
        .filter_map(Plan::gym)
        .flat_map(crate::plan::Mesocycles::mesocycles)
        .collect();
    mesocycles.sort_by_key(|mesocycle| mesocycle.span().start());
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
            if mesocycle.tests_in(week) && last_of_its_week(mesocycle, on) {
                return Some(on);
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

/// A macrocycle's estimates, read by one line through them all.
#[derive(Debug, Clone, PartialEq)]
pub struct Trend<P = Point> {
    /// The macrocycle's first day. The line runs in days since it.
    pub from: Date,
    /// Every session's figure in the macrocycle, oldest first.
    pub figures: Vec<P>,
    /// Through every one of them.
    pub line: Option<Line>,
    /// From the third figure, where the line through the figures up to each
    /// one stood that day, and how far either side it could lie.
    pub narrowing: Vec<Narrowing>,
    /// From the first figure to the latest, along [`Self::line`].
    pub change: Option<Change>,
    /// The line carried on to the next planned test.
    pub next: Option<Projected>,
}

/// What the line knew on one day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Narrowing {
    pub on: Date,
    /// In the figure's own unit, as [`Figure::value`].
    pub value: f64,
    /// How far either side of `value`, at [`CONFIDENCE`].
    pub band: f64,
}

/// The change along a line, read against its margin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Change {
    /// In the figure's own unit.
    pub by: f64,
    /// How far either side of `by` chance could put it, at [`CONFIDENCE`].
    pub margin: f64,
    /// Rising or falling where `by` is beyond `margin`; `None` is no clear
    /// change.
    pub direction: Option<Direction>,
}

/// Where the line puts a day still to come.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projected {
    pub on: Date,
    pub value: f64,
}

impl<P: Figure> Trend<P> {
    /// The line's value at the latest figure: the current estimate.
    pub fn now(&self) -> Option<f64> {
        self.line.map(|line| line.at(line.last()))
    }

    /// Days since [`Self::from`].
    pub fn day(&self, on: Date) -> i32 {
        self.from.until(on).map_or(0, |gap| gap.get_days())
    }
}

/// The figures from `from`, read by a least-squares line through all of them
/// by day, and carried on to `next` (the operator, 2026-11-04, #688).
///
/// **The band narrows as figures accrue**, because each day's is the line
/// through the figures up to that day, read that day. The line's own band is
/// widest at its ends; the operator's reading of it was *"I would expect that
/// our estimate would narrow with more information"*.
///
/// **The change is read as the 1RM tests' macrocycle line is**: rising or
/// falling where the change along the line, first figure to latest, is beyond
/// its margin.
pub fn trend<P: Figure>(figures: &[P], from: Date, next: Option<Date>) -> Trend<P> {
    let mut trend = Trend {
        from,
        figures: figures
            .iter()
            .filter(|figure| figure.on() >= from)
            .copied()
            .collect(),
        line: None,
        narrowing: Vec::new(),
        change: None,
        next: None,
    };
    let valued: Vec<(Date, i32, f64)> = trend
        .figures
        .iter()
        .filter_map(|figure| Some((figure.on(), trend.day(figure.on()), figure.value()?)))
        .collect();
    let through = |count: usize| {
        Line::through(
            &valued
                .iter()
                .take(count)
                .map(|(_, day, value)| (*day, *value))
                .collect::<Vec<_>>(),
        )
    };
    trend.narrowing = (1..=valued.len())
        .filter_map(|count| {
            let line = through(count)?;
            let (on, day, _) = valued.get(count - 1)?;
            Some(Narrowing {
                on: *on,
                value: line.at(*day),
                band: line.band(*day),
            })
        })
        .collect();
    trend.line = through(valued.len());
    trend.change = trend.line.map(|line| {
        let along = f64::from(line.last() - line.first());
        let by = line.per_day() * along;
        let margin = line.slope_margin() * along;
        Change {
            by,
            margin,
            direction: if by > margin {
                Some(Direction::Rising)
            } else if by < -margin {
                Some(Direction::Falling)
            } else {
                None
            },
        }
    });
    trend.next = trend.line.zip(next).map(|(line, on)| Projected {
        on,
        value: line.at(trend.day(on)),
    });
    trend
}

fn grams(grams: u64) -> Option<f64> {
    u32::try_from(grams).ok().map(f64::from)
}
